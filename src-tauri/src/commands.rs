//! IPC command layer — thin Tauri command wrappers over vault, indexer, search, watcher.
//!
//! Every command is an `async fn` decorated with `#[tauri::command]`.
//! State is held in [`AppState`] behind `tokio::sync::Mutex`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::indexer::{Backlink, GraphEdge, Indexer, NoteData, Wikilink};
use crate::search::SearchEngine;
use crate::vault::{TreeEntry, VaultManager};
use crate::watcher::VaultWatcher;

// ---------------------------------------------------------------------------
// CommandError
// ---------------------------------------------------------------------------
#[derive(Debug, Error)]
pub enum CommandError {
    #[error("vault not open")]
    VaultNotOpen,
    #[error("indexer not available — try rebuilding the index")]
    IndexerNotReady,
    #[error("search engine not available")]
    SearchNotReady,
    #[error("vault error: {0}")]
    Vault(String),
    #[error("indexer error: {0}")]
    Indexer(String),
    #[error("search error: {0}")]
    Search(String),
    #[error("watcher error: {0}")]
    Watcher(String),
    #[error("io error: {0}")]
    Io(String),
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Serializable tree (TreeEntry has no Serialize)
// ---------------------------------------------------------------------------

/// JSON-serialisable directory tree mirror.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum TreeEntryJson {
    #[serde(rename = "file")]
    File { name: String },
    #[serde(rename = "dir")]
    Dir {
        name: String,
        children: Vec<TreeEntryJson>,
    },
}

impl TreeEntryJson {
    fn from_entry(e: &TreeEntry) -> Self {
        match e {
            TreeEntry::File { name } => TreeEntryJson::File { name: name.clone() },
            TreeEntry::Dir { name, children } => TreeEntryJson::Dir {
                name: name.clone(),
                children: children.iter().map(Self::from_entry).collect(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Watcher event payload (serialisable for Tauri events)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum WatchEventPayload {
    #[serde(rename = "created")]
    Created { path: String },
    #[serde(rename = "modified")]
    Modified { path: String },
    #[serde(rename = "removed")]
    Removed { path: String },
    #[serde(rename = "bulk_change")]
    BulkChange { count: usize },
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// Managed state shared across all Tauri commands.
#[derive(Default)]
pub struct AppState {
    pub vault: Option<VaultManager>,
    pub indexer: Option<Indexer>,
    pub search: Option<SearchEngine>,
    pub watcher: Option<VaultWatcher>,
    pub app_handle: Option<AppHandle<tauri::Wry>>,
    /// Cancellation token for the watcher event-loop task (if running).
    watcher_cancel: Option<CancellationToken>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sanitize a user-supplied path string.
///
/// Rejects: empty strings, strings containing null bytes, relative paths,
/// path traversal components (`..`), and paths under `/tmp`, `/proc`,
/// `/dev`, `/sys`, `/etc`, `/home`, `/var`, `/root`.
fn sanitize_path(input: &str) -> Result<PathBuf, CommandError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CommandError::Vault("path must not be empty".into()));
    }
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let path = PathBuf::from(trimmed);
    if path.is_relative() {
        return Err(CommandError::Vault("path must be absolute".into()));
    }
    // Reject path traversal via path components (not substring).
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(CommandError::Vault(
            "path must not contain path traversal (..)".into(),
        ));
    }
    for prefix in &[
        "/tmp", "/proc", "/dev", "/sys", "/etc", "/home", "/var", "/root",
    ] {
        if trimmed.starts_with(prefix) {
            return Err(CommandError::Vault(format!(
                "path under {prefix} is not allowed"
            )));
        }
    }
    Ok(path)
}

/// Strip the vault root prefix, returning a relative path string.
///
/// If the path is outside the vault, returns `"[outside-vault]"` to avoid
/// leaking absolute filesystem paths to the frontend webview.
fn strip_vault_prefix(absolute: &Path, vault_root: &Path) -> String {
    absolute
        .strip_prefix(vault_root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "[outside-vault]".to_string())
}

/// Re-index a single note into the search engine from vault + indexer state.
fn reindex_search_from_note(s: &mut AppState, path: &str) -> Result<(), CommandError> {
    let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let body = vault
        .read_note(path)
        .map_err(|e| CommandError::Io(e.to_string()))?;

    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    let note = indexer
        .index()
        .notes
        .get(path)
        .ok_or_else(|| CommandError::Indexer(format!("note not in index: {path}")))?
        .clone();

    let mod_dt = tantivy::DateTime::from_timestamp_micros(parse_iso_to_micros(
        note.modified.as_deref().unwrap_or(""),
    ));
    let cre_dt = tantivy::DateTime::from_timestamp_micros(parse_iso_to_micros(
        note.created.as_deref().unwrap_or(""),
    ));

    let search = s.search.as_mut().ok_or(CommandError::SearchNotReady)?;
    search
        .index_note(path, &note.title, &body, &note.tags, mod_dt, cre_dt)
        .map_err(|e| CommandError::Search(e.to_string()))?;
    search
        .commit()
        .map_err(|e| CommandError::Search(e.to_string()))?;

    Ok(())
}

/// Best-effort parse of an ISO-8601 date string to microseconds since epoch.
///
/// Returns 0 on parse failure (search ordering degrades gracefully).
fn parse_iso_to_micros(s: &str) -> i64 {
    if s.is_empty() {
        return 0;
    }
    // Expect format: YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS or with fractional seconds.
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return 0;
    }

    // Parse date portion YYYY-MM-DD.
    let year: i32 = match bytes[0..4]
        .iter()
        .map(|b| *b as char)
        .collect::<String>()
        .parse()
    {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let month: u32 = match bytes[5..7]
        .iter()
        .map(|b| *b as char)
        .collect::<String>()
        .parse()
    {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let day: u32 = match bytes[8..10]
        .iter()
        .map(|b| *b as char)
        .collect::<String>()
        .parse()
    {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let mut hour: u32 = 0;
    let mut minute: u32 = 0;
    let mut second: u32 = 0;

    // Parse optional time portion THH:MM:SS.
    if bytes.len() >= 19 && bytes[10] == b'T' {
        hour = bytes[11..13]
            .iter()
            .map(|b| *b as char)
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        minute = bytes[14..16]
            .iter()
            .map(|b| *b as char)
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        second = bytes[17..19]
            .iter()
            .map(|b| *b as char)
            .collect::<String>()
            .parse()
            .unwrap_or(0);
    }

    // Compute days since Unix epoch using civil calendar algorithm.
    let days = days_from_civil(year, month, day);
    // Microseconds from the date + time components.
    days * 86_400_000_000
        + hour as i64 * 3_600_000_000
        + minute as i64 * 60_000_000
        + second as i64 * 1_000_000
}

/// Convert a civil (year, month, day) date to days since 1970-01-01.
///
/// Uses Howard Hinnant's algorithm.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = (yoe as i64) * 365 + (yoe / 4) as i64 - (yoe / 100) as i64 + (doy as i64);
    (era as i64) * 146097 + doe - 719468
}

// ===========================================================================
// ---- Vault Commands -------------------------------------------------------
// ===========================================================================

/// Open a vault at the given absolute directory path.
///
/// Validates the path, creates a [`VaultManager`], builds a fresh index,
/// opens the search engine, and initialises (but does not start) the watcher.
#[tauri::command]
pub async fn open_vault(
    app_handle: AppHandle<tauri::Wry>,
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
) -> Result<String, CommandError> {
    let vault_path = sanitize_path(&path)?;

    let vault =
        VaultManager::open_vault(&vault_path).map_err(|e| CommandError::Vault(e.to_string()))?;

    let root = vault.root().to_string_lossy().into_owned();

    let search_index_path = vault_path.join(".vault-index").join("search");

    let mut indexer = Indexer::new();
    indexer
        .rebuild(&vault_path)
        .map_err(|e| CommandError::Indexer(e.to_string()))?;

    let mut search =
        SearchEngine::new(&search_index_path).map_err(|e| CommandError::Search(e.to_string()))?;

    // Index all notes into tantivy.
    let idx = indexer.index();
    for (rel_path, note) in &idx.notes {
        let body = vault
            .read_note(rel_path)
            .map_err(|e| CommandError::Io(e.to_string()))?;

        let mod_dt = tantivy::DateTime::from_timestamp_micros(parse_iso_to_micros(
            note.modified.as_deref().unwrap_or(""),
        ));
        let cre_dt = tantivy::DateTime::from_timestamp_micros(parse_iso_to_micros(
            note.created.as_deref().unwrap_or(""),
        ));

        search
            .index_note(rel_path, &note.title, &body, &note.tags, mod_dt, cre_dt)
            .map_err(|e| CommandError::Search(e.to_string()))?;
    }
    search
        .commit()
        .map_err(|e| CommandError::Search(e.to_string()))?;

    let watcher = VaultWatcher::new(&vault_path, None);

    let mut s = state.lock().await;
    s.vault = Some(vault);
    s.indexer = Some(indexer);
    s.search = Some(search);
    s.watcher = Some(watcher);
    s.app_handle = Some(app_handle);

    Ok(root)
}

/// Close the current vault, releasing all state and stopping the watcher.
#[tauri::command]
pub async fn close_vault(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), CommandError> {
    let mut s = state.lock().await;
    if let Some(ref mut w) = s.watcher {
        w.stop();
    }
    s.vault = None;
    s.indexer = None;
    s.search = None;
    s.watcher = None;
    s.app_handle = None;
    Ok(())
}

/// Return the directory tree of the vault.
#[tauri::command]
pub async fn get_tree(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<TreeEntryJson, CommandError> {
    let s = state.lock().await;
    let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let tree = vault
        .build_tree()
        .map_err(|e| CommandError::Io(e.to_string()))?;
    Ok(TreeEntryJson::from_entry(&tree))
}

/// Read the content of a note at the given relative path.
#[tauri::command]
pub async fn read_note(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
) -> Result<String, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let s = state.lock().await;
    let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    vault
        .read_note(trimmed)
        .map_err(|e| CommandError::Io(e.to_string()))
}

/// Create a new note with the given content, then index it.
#[tauri::command]
pub async fn create_note(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
    content: String,
) -> Result<String, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let mut s = state.lock().await;
    {
        let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
        vault
            .create_note(trimmed, &content)
            .map_err(|e| CommandError::Io(e.to_string()))?;
    }

    {
        let full_path = {
            let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
            vault
                .resolve_path(trimmed)
                .map_err(|e| CommandError::Io(e.to_string()))?
        };
        let indexer = s.indexer.as_mut().ok_or(CommandError::VaultNotOpen)?;
        indexer
            .index_file_at(&full_path, trimmed)
            .map_err(|e| CommandError::Indexer(e.to_string()))?;
    }

    reindex_search_from_note(&mut s, trimmed)?;

    Ok(trimmed.to_string())
}

/// Update an existing note with new content, then re-index.
#[tauri::command]
pub async fn update_note(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
    content: String,
) -> Result<String, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let mut s = state.lock().await;
    {
        let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
        vault
            .update_note(trimmed, &content)
            .map_err(|e| CommandError::Io(e.to_string()))?;
    }

    // Re-index.
    {
        let full_path = {
            let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
            vault
                .resolve_path(trimmed)
                .map_err(|e| CommandError::Io(e.to_string()))?
        };
        let indexer = s.indexer.as_mut().ok_or(CommandError::VaultNotOpen)?;
        indexer
            .index_file_at(&full_path, trimmed)
            .map_err(|e| CommandError::Indexer(e.to_string()))?;
    }

    reindex_search_from_note(&mut s, trimmed)?;

    Ok(trimmed.to_string())
}

/// Delete a note and remove it from both the index and search engine.
#[tauri::command]
pub async fn delete_note(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
) -> Result<String, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let mut s = state.lock().await;
    let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    vault
        .delete_note(trimmed)
        .map_err(|e| CommandError::Io(e.to_string()))?;

    let indexer = s.indexer.as_mut().ok_or(CommandError::VaultNotOpen)?;
    indexer.remove_file(trimmed);

    let search = s.search.as_mut().ok_or(CommandError::VaultNotOpen)?;
    search
        .delete_note(trimmed)
        .map_err(|e| CommandError::Search(e.to_string()))?;
    search
        .commit()
        .map_err(|e| CommandError::Search(e.to_string()))?;

    Ok(trimmed.to_string())
}

/// Rename a note, re-indexing both the old and new paths.
#[tauri::command]
pub async fn rename_note(
    state: State<'_, Arc<Mutex<AppState>>>,
    old_path: String,
    new_path: String,
) -> Result<String, CommandError> {
    let old_trimmed = old_path.trim();
    let new_trimmed = new_path.trim();
    if old_trimmed.contains('\0') || new_trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let mut s = state.lock().await;
    {
        let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
        vault
            .rename_note(old_trimmed, new_trimmed)
            .map_err(|e| CommandError::Io(e.to_string()))?;
    }

    // Remove old from index + search.
    {
        let indexer = s.indexer.as_mut().ok_or(CommandError::VaultNotOpen)?;
        indexer.remove_file(old_trimmed);
    }

    {
        let search = s.search.as_mut().ok_or(CommandError::VaultNotOpen)?;
        search
            .delete_note(old_trimmed)
            .map_err(|e| CommandError::Search(e.to_string()))?;
    }

    // Index new file.
    {
        let full_path = {
            let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
            vault
                .resolve_path(new_trimmed)
                .map_err(|e| CommandError::Io(e.to_string()))?
        };
        let indexer = s.indexer.as_mut().ok_or(CommandError::VaultNotOpen)?;
        indexer
            .index_file_at(&full_path, new_trimmed)
            .map_err(|e| CommandError::Indexer(e.to_string()))?;
    }

    reindex_search_from_note(&mut s, new_trimmed)?;

    Ok(new_trimmed.to_string())
}

/// Create a folder at the given relative path.
#[tauri::command]
pub async fn create_folder(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
) -> Result<String, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let s = state.lock().await;
    let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    vault
        .create_folder(trimmed)
        .map_err(|e| CommandError::Io(e.to_string()))?;
    Ok(trimmed.to_string())
}

// ===========================================================================
// ---- Indexer Commands -----------------------------------------------------
// ===========================================================================

/// Full rebuild of both the vault index and search engine.
#[tauri::command]
pub async fn rebuild_index(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), CommandError> {
    let mut s = state.lock().await;

    // Step 1: Rebuild indexer  extract vault root path first, then mut borrow
    let vault_root = s
        .vault
        .as_ref()
        .ok_or(CommandError::VaultNotOpen)?
        .root()
        .to_path_buf();
    {
        let indexer = s.indexer.as_mut().ok_or(CommandError::VaultNotOpen)?;
        indexer
            .rebuild(&vault_root)
            .map_err(|e| CommandError::Indexer(e.to_string()))?;
    }

    // Step 2: Rebuild search (need vault for read_note + mut indexer for index())
    let vault = s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let search_index_path = vault.root().join(".vault-index").join("search");
    let mut search =
        SearchEngine::new(&search_index_path).map_err(|e| CommandError::Search(e.to_string()))?;

    let idx = indexer.index();
    for (rel_path, note) in &idx.notes {
        let body = vault
            .read_note(rel_path)
            .map_err(|e| CommandError::Io(e.to_string()))?;

        let mod_dt = tantivy::DateTime::from_timestamp_micros(parse_iso_to_micros(
            note.modified.as_deref().unwrap_or(""),
        ));
        let cre_dt = tantivy::DateTime::from_timestamp_micros(parse_iso_to_micros(
            note.created.as_deref().unwrap_or(""),
        ));

        search
            .index_note(rel_path, &note.title, &body, &note.tags, mod_dt, cre_dt)
            .map_err(|e| CommandError::Search(e.to_string()))?;
    }
    search
        .commit()
        .map_err(|e| CommandError::Search(e.to_string()))?;

    s.search = Some(search);
    Ok(())
}

/// Return metadata for a single note.
#[tauri::command]
pub async fn get_note(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
) -> Result<NoteData, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    let note = indexer
        .index()
        .notes
        .get(trimmed)
        .ok_or_else(|| CommandError::Indexer(format!("note not found: {trimmed}")))?
        .clone();
    Ok(note)
}

/// Return backlinks for a note.
#[tauri::command]
pub async fn get_backlinks(
    state: State<'_, Arc<Mutex<AppState>>>,
    path: String,
) -> Result<Vec<Backlink>, CommandError> {
    let trimmed = path.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Vault("path contains null byte".into()));
    }
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    let backlinks: Vec<Backlink> = indexer
        .get_backlinks(trimmed)
        .into_iter()
        .cloned()
        .collect();
    Ok(backlinks)
}

/// Return graph data (node IDs + edges) for graph visualisation.
#[tauri::command]
pub async fn get_graph_data(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(Vec<String>, Vec<GraphEdge>), CommandError> {
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    Ok(indexer.get_graph_data())
}

/// Return all tags with their note counts.
#[tauri::command]
pub async fn get_tags(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<(String, usize)>, CommandError> {
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    Ok(indexer.get_tags())
}

/// Return all notes that carry the given tag.
#[tauri::command]
pub async fn get_notes_by_tag(
    state: State<'_, Arc<Mutex<AppState>>>,
    tag: String,
) -> Result<Vec<NoteData>, CommandError> {
    let trimmed = tag.trim();
    if trimmed.contains('\0') {
        return Err(CommandError::Indexer("tag contains null byte".into()));
    }
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    let notes: Vec<NoteData> = indexer
        .get_notes_by_tag(trimmed)
        .into_iter()
        .cloned()
        .collect();
    Ok(notes)
}

/// Resolve a wikilink target (with optional heading) to a file path.
#[tauri::command]
pub async fn resolve_wikilink(
    state: State<'_, Arc<Mutex<AppState>>>,
    target: String,
    heading: Option<String>,
) -> Result<Option<String>, CommandError> {
    let trimmed_target = target.trim();
    if trimmed_target.contains('\0') {
        return Err(CommandError::Indexer("target contains null byte".into()));
    }
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let indexer = s.indexer.as_ref().ok_or(CommandError::IndexerNotReady)?;
    let link = Wikilink {
        raw: format!("[[{trimmed_target}]]"),
        target: trimmed_target.to_string(),
        heading,
        display_text: None,
        block_id: None,
    };
    Ok(indexer.resolve_link(&link))
}

// ===========================================================================
// ---- Search Commands ------------------------------------------------------
// ===========================================================================

/// Full-text search over vault notes. Default limit 20.
#[tauri::command]
pub async fn search(
    state: State<'_, Arc<Mutex<AppState>>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::search::SearchResult>, CommandError> {
    if query.trim().contains('\0') {
        return Err(CommandError::Search("query contains null byte".into()));
    }
    let limit = limit.unwrap_or(20).min(100);
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let search = s.search.as_ref().ok_or(CommandError::SearchNotReady)?;
    search
        .search(query.trim(), limit)
        .map_err(|e| CommandError::Search(e.to_string()))
}

/// Full-text search filtered to notes with a specific tag.
#[tauri::command]
pub async fn search_by_tag(
    state: State<'_, Arc<Mutex<AppState>>>,
    query: String,
    tag: String,
    limit: Option<usize>,
) -> Result<Vec<crate::search::SearchResult>, CommandError> {
    if query.trim().contains('\0') || tag.trim().contains('\0') {
        return Err(CommandError::Search("input contains null byte".into()));
    }
    let limit = limit.unwrap_or(20).min(100);
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let search = s.search.as_ref().ok_or(CommandError::SearchNotReady)?;
    search
        .search_with_filter(query.trim(), limit, Some(tag.trim()))
        .map_err(|e| CommandError::Search(e.to_string()))
}

/// Autocomplete suggestions for a title prefix.
#[tauri::command]
pub async fn suggest(
    state: State<'_, Arc<Mutex<AppState>>>,
    prefix: String,
    limit: Option<usize>,
) -> Result<Vec<String>, CommandError> {
    if prefix.trim().contains('\0') {
        return Err(CommandError::Search("prefix contains null byte".into()));
    }
    let limit = limit.unwrap_or(10).min(100);
    let s = state.lock().await;
    s.vault.as_ref().ok_or(CommandError::VaultNotOpen)?;
    let search = s.search.as_ref().ok_or(CommandError::SearchNotReady)?;
    search
        .suggest(prefix.trim(), limit)
        .map_err(|e| CommandError::Search(e.to_string()))
}

// ===========================================================================
// ---- Watcher Commands -----------------------------------------------------
// ===========================================================================

/// Start the file watcher. Events are emitted as Tauri events on "vault-change".
#[tauri::command]
pub async fn start_watcher(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), CommandError> {
    let mut s = state.lock().await;

    // If a previous watcher task is running, cancel it first.
    if let Some(token) = s.watcher_cancel.take() {
        token.cancel();
    }

    let vault_root = s
        .vault
        .as_ref()
        .map(|v| v.root().to_path_buf())
        .ok_or(CommandError::VaultNotOpen)?;

    let watcher = s.watcher.as_mut().ok_or(CommandError::VaultNotOpen)?;
    let rx = watcher
        .subscribe()
        .ok_or_else(|| CommandError::Watcher("watcher already subscribed".into()))?;
    watcher
        .start()
        .map_err(|e| CommandError::Watcher(e.to_string()))?;

    let app_handle = s.app_handle.clone().ok_or(CommandError::VaultNotOpen)?;

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    s.watcher_cancel = Some(cancel);

    // Clone the Arc so the spawned task can lock state independently.
    let state_arc = Arc::clone(state.inner());

    // Spawn task: read watcher events → emit Tauri events + update indexer/search.
    // The task exits when the receiver is dropped OR the cancellation token fires.
    tokio::spawn(async move {
        let mut rx = rx;
        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(event) => {
                            // --- Phase 1: Update backend (indexer + search) ---
                            match &event {
                                crate::watcher::WatchEvent::Created(p)
                                | crate::watcher::WatchEvent::Modified(p) => {
                                    let rel = strip_vault_prefix(p, &vault_root);
                                    if rel.ends_with(".md") {
                                        let full = p.clone();
                                        let mut s = state_arc.lock().await;
                                        if let Some(indexer) = s.indexer.as_mut() {
                                            let _ = indexer.index_file_at(&full, &rel);
                                        }
                                        if let Err(e) = reindex_search_from_note(&mut s, &rel) {
                                            eprintln!("watcher: reindex_search error: {e}");
                                        }
                                    }
                                }
                                crate::watcher::WatchEvent::Removed(p) => {
                                    let rel = strip_vault_prefix(p, &vault_root);
                                    if rel.ends_with(".md") {
                                        let mut s = state_arc.lock().await;
                                        if let Some(indexer) = s.indexer.as_mut() {
                                            indexer.remove_file(&rel);
                                        }
                                        if let Some(search) = s.search.as_mut() {
                                            let _ = search.delete_note(&rel);
                                            let _ = search.commit();
                                        }
                                    }
                                }
                                crate::watcher::WatchEvent::Rename { old, new } => {
                                    let rel_old = strip_vault_prefix(old, &vault_root);
                                    let rel_new = strip_vault_prefix(new, &vault_root);
                                    let md_old = rel_old.ends_with(".md");
                                    let md_new = rel_new.ends_with(".md");
                                    if md_old || md_new {
                                        let full_new = new.clone();
                                        let mut s = state_arc.lock().await;
                                        // Remove old path from index + search.
                                        if md_old {
                                            if let Some(indexer) = s.indexer.as_mut() {
                                                indexer.remove_file(&rel_old);
                                            }
                                            if let Some(search) = s.search.as_mut() {
                                                let _ = search.delete_note(&rel_old);
                                            }
                                        }
                                        // Index new path into index + search.
                                        if md_new {
                                            if let Some(indexer) = s.indexer.as_mut() {
                                                let _ = indexer.index_file_at(&full_new, &rel_new);
                                            }
                                            if let Err(e) = reindex_search_from_note(&mut s, &rel_new) {
                                                eprintln!("watcher: reindex_search error: {e}");
                                            }
                                        }
                                        // Single commit for both removals and additions.
                                        if let Some(search) = s.search.as_mut() {
                                            let _ = search.commit();
                                        }
                                    }
                                }
                                crate::watcher::WatchEvent::BulkChange { .. } => {
                                    let mut s = state_arc.lock().await;
                                    let root = match s.vault.as_ref() {
                                        Some(v) => v.root().to_path_buf(),
                                        None => continue,
                                    };

                                    // Snapshot old indexed paths before rebuild so we can
                                    // purge stale search entries for externally deleted notes.
                                    let old_paths: HashSet<String> = s
                                        .indexer
                                        .as_ref()
                                        .map(|idx| idx.index().notes.keys().cloned().collect())
                                        .unwrap_or_default();

                                    if let Some(indexer) = s.indexer.as_mut() {
                                        let _ = indexer.rebuild(&root);
                                    }
                                    // Rebuild search from the refreshed indexer.
                                    // Split borrows: collect immutable data first, then mutably borrow search.
                                    let vault_snap = s.vault.as_ref().map(|v| v.root().to_path_buf());
                                    let idx_snap = s.indexer.as_ref().map(|idx| idx.index().notes.clone());
                                    if let (Some(vault_path), Some(idx_notes), Some(search)) =
                                        (vault_snap, idx_snap, s.search.as_mut())
                                    {
                                        let new_paths: HashSet<&String> = idx_notes.keys().collect();
                                        let vault = crate::vault::VaultManager::new(&vault_path);

                                        // Remove stale search entries for notes no longer in index.
                                        for old_path in &old_paths {
                                            if !new_paths.contains(old_path) {
                                                let _ = search.delete_note(old_path);
                                            }
                                        }
                                        // Re-index all notes into search.
                                        for (rel, note) in &idx_notes {
                                            if let Ok(body) = vault.read_note(rel) {
                                                let mod_dt = tantivy::DateTime::from_timestamp_micros(
                                                    parse_iso_to_micros(note.modified.as_deref().unwrap_or("")),
                                                );
                                                let cre_dt = tantivy::DateTime::from_timestamp_micros(
                                                    parse_iso_to_micros(note.created.as_deref().unwrap_or("")),
                                                );
                                                let _ = search.index_note(
                                                    rel, &note.title, &body, &note.tags, mod_dt, cre_dt,
                                                );
                                            }
                                        }
                                        let _ = search.commit();
                                    }
                                }
                            }

                            // --- Phase 2: Emit frontend events AFTER backend is in sync ---
                            match &event {
                                crate::watcher::WatchEvent::Created(p) => {
                                    let rel = strip_vault_prefix(p, &vault_root);
                                    let _ = app_handle.emit("vault-change", WatchEventPayload::Created { path: rel });
                                }
                                crate::watcher::WatchEvent::Modified(p) => {
                                    let rel = strip_vault_prefix(p, &vault_root);
                                    let _ = app_handle.emit("vault-change", WatchEventPayload::Modified { path: rel });
                                }
                                crate::watcher::WatchEvent::Removed(p) => {
                                    let rel = strip_vault_prefix(p, &vault_root);
                                    let _ = app_handle.emit("vault-change", WatchEventPayload::Removed { path: rel });
                                }
                                crate::watcher::WatchEvent::Rename { old, new } => {
                                    let rel_old = strip_vault_prefix(old, &vault_root);
                                    let rel_new = strip_vault_prefix(new, &vault_root);
                                    let _ = app_handle.emit(
                                        "vault-change",
                                        WatchEventPayload::Removed { path: rel_old },
                                    );
                                    let _ = app_handle.emit(
                                        "vault-change",
                                        WatchEventPayload::Created { path: rel_new },
                                    );
                                }
                                crate::watcher::WatchEvent::BulkChange { count } => {
                                    let _ = app_handle.emit("vault-change", WatchEventPayload::BulkChange { count: *count });
                                }
                            }
                        }
                        None => break, // channel closed
                    }
                }
                _ = cancel_clone.cancelled() => {
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Stop the file watcher and cancel the event-loop task.
#[tauri::command]
pub async fn stop_watcher(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), CommandError> {
    let mut s = state.lock().await;

    // Cancel the event-loop task if present.
    if let Some(token) = s.watcher_cancel.take() {
        token.cancel();
    }

    let watcher = s.watcher.as_mut().ok_or(CommandError::VaultNotOpen)?;
    watcher.stop();
    Ok(())
}
