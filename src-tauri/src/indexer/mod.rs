//! Indexer — parses vault markdown files and builds a full in-memory index.
//!
//! Extracts frontmatter, wikilinks, inline tags, headings, graph edges,
//! and a reverse backlink index. Uses rayon for parallel file parsing.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use regex::Regex;
use std::sync::LazyLock;

static RE_WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]]+?)\]\]").unwrap());
static RE_INLINE_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?<!\w)#[A-Za-z][\w/-]*").unwrap());
static RE_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+)$").unwrap());
use serde::Serialize;
use walkdir::WalkDir;

use crate::vault::VaultManager;

// ---------------------------------------------------------------------------
// IndexerError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum IndexerError {
    Io(io::Error),
    Parse(String),
}

impl fmt::Display for IndexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "indexer I/O error: {e}"),
            Self::Parse(msg) => write!(f, "indexer parse error: {msg}"),
        }
    }
}

impl std::error::Error for IndexerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(_) => None,
        }
    }
}

impl From<io::Error> for IndexerError {
    fn from(e: io::Error) -> Self {
        IndexerError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Data structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct NoteData {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub frontmatter: Frontmatter,
    pub outgoing_links: Vec<Wikilink>,
    pub headings: Vec<Heading>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub word_count: usize,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Frontmatter {
    pub raw: HashMap<String, serde_json::Value>,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Wikilink {
    pub raw: String,
    pub target: String,
    pub heading: Option<String>,
    pub display_text: Option<String>,
    pub block_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Backlink {
    pub source_path: String,
    pub source_title: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct VaultIndex {
    pub notes: HashMap<String, NoteData>,
    pub backlinks: HashMap<String, Vec<Backlink>>,
    pub graph_edges: Vec<GraphEdge>,
    pub tag_index: HashMap<String, Vec<String>>,
}

// ---------------------------------------------------------------------------
// Indexer
// ---------------------------------------------------------------------------

pub struct Indexer {
    vault_path: PathBuf,
    index: VaultIndex,
}

impl Indexer {
    pub fn new(vault_path: PathBuf) -> Self {
        Self {
            vault_path,
            index: VaultIndex::default(),
        }
    }

    /// Full index rebuild. Walks all `.md` files, parses in parallel with
    /// rayon, then builds graph edges and backlinks in a single pass.
    pub fn rebuild(&mut self, vault_root: &Path) -> Result<(), IndexerError> {
        // Collect all .md file absolute paths, skip hidden dirs/files
        let md_files: Vec<PathBuf> = WalkDir::new(vault_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                if !e.file_type().is_file() {
                    return false;
                }
                if !e.path().extension().is_some_and(|ext| ext == "md") {
                    return false;
                }
                !e.path()
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        // Parse all files in parallel. Keep body for context extraction.
        let root = vault_root.to_path_buf();
        let parsed: Vec<(String, NoteData, String)> = md_files
            .par_iter()
            .filter_map(|abs_path| {
                let relative = abs_path
                    .strip_prefix(&root)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())?;
                let content = std::fs::read_to_string(abs_path).ok()?;
                let (note_data, body) = parse_note(&relative, &content);
                Some((relative, note_data, body))
            })
            .collect();

        // Build notes map and tag index
        let mut notes: HashMap<String, NoteData> = HashMap::with_capacity(parsed.len());
        let mut tag_index: HashMap<String, Vec<String>> = HashMap::new();

        for (path, note_data, _body) in &parsed {
            let mut all_tags: Vec<String> = note_data.frontmatter.tags.clone();
            for tag in &note_data.tags {
                if !all_tags.contains(tag) {
                    all_tags.push(tag.clone());
                }
            }
            for tag in &all_tags {
                tag_index.entry(tag.clone()).or_default().push(path.clone());
            }
            notes.insert(path.clone(), note_data.clone());
        }

        // Build graph edges + backlinks in single pass
        let mut graph_edges: Vec<GraphEdge> = Vec::new();
        let mut backlinks: HashMap<String, Vec<Backlink>> = HashMap::new();

        for (source_path, note, body) in &parsed {
            for link in &note.outgoing_links {
                if let Some(target_path) = resolve_target(&link.target, &notes) {
                    graph_edges.push(GraphEdge {
                        source: source_path.clone(),
                        target: target_path.clone(),
                    });

                    let context = extract_link_context(body, link);
                    backlinks.entry(target_path).or_default().push(Backlink {
                        source_path: source_path.clone(),
                        source_title: note.title.clone(),
                        context,
                    });
                }
            }
        }

        self.index = VaultIndex {
            notes,
            backlinks,
            graph_edges,
            tag_index,
        };

        Ok(())
    }

    /// Index or re-index a single file by relative path.
    pub fn index_file(&mut self, vault: &VaultManager, path: &str) -> Result<(), IndexerError> {
        let full_path = vault.resolve_path(path).map_err(IndexerError::Io)?;
        self.index_file_at(&full_path, path)
    }

    /// Index or re-index a single file given its pre-resolved absolute path.
    /// This avoids holding an immutable vault reference while borrowing self mutably.
    pub fn index_file_at(&mut self, full_path: &Path, path: &str) -> Result<(), IndexerError> {
        let content = std::fs::read_to_string(full_path).map_err(IndexerError::Io)?;
        let (note_data, body) = parse_note(path, &content);

        // Clean up old outgoing link graph/backlink entries
        if let Some(old) = self.index.notes.get(path) {
            for link in &old.outgoing_links {
                if let Some(target) = resolve_target(&link.target, &self.index.notes) {
                    if let Some(bl) = self.index.backlinks.get_mut(&target) {
                        bl.retain(|b| b.source_path != path);
                    }
                    self.index
                        .graph_edges
                        .retain(|e| !(e.source == path && e.target == target));
                }
            }
            // Remove old tags from tag_index
            let mut old_all_tags: Vec<String> = old.frontmatter.tags.clone();
            for t in &old.tags {
                if !old_all_tags.contains(t) {
                    old_all_tags.push(t.clone());
                }
            }
            for tag in old_all_tags {
                if let Some(paths) = self.index.tag_index.get_mut(&tag) {
                    paths.retain(|p| p != path);
                }
            }
        }

        // Add new tags
        let mut new_all_tags: Vec<String> = note_data.frontmatter.tags.clone();
        for t in &note_data.tags {
            if !new_all_tags.contains(t) {
                new_all_tags.push(t.clone());
            }
        }
        for tag in &new_all_tags {
            self.index
                .tag_index
                .entry(tag.clone())
                .or_default()
                .push(path.to_string());
        }

        // Add new outgoing links → graph edges + backlinks
        for link in &note_data.outgoing_links {
            if let Some(target_path) = resolve_target(&link.target, &self.index.notes) {
                self.index.graph_edges.push(GraphEdge {
                    source: path.to_string(),
                    target: target_path.clone(),
                });
                let context = extract_link_context(&body, link);
                self.index
                    .backlinks
                    .entry(target_path)
                    .or_default()
                    .push(Backlink {
                        source_path: path.to_string(),
                        source_title: note_data.title.clone(),
                        context,
                    });
            }
        }

        self.index.notes.insert(path.to_string(), note_data);
        Ok(())
    }

    /// Remove a file from the index entirely.
    pub fn remove_file(&mut self, path: &str) {
        if let Some(old) = self.index.notes.remove(path) {
            // Remove backlinks pointing TO this file
            self.index.backlinks.remove(path);

            // Remove outgoing link graph edges + backlink entries on targets
            for link in &old.outgoing_links {
                if let Some(target) = resolve_target(&link.target, &self.index.notes) {
                    if let Some(bl) = self.index.backlinks.get_mut(&target) {
                        bl.retain(|b| b.source_path != path);
                    }
                    self.index
                        .graph_edges
                        .retain(|e| !(e.source == path && e.target == target));
                }
            }

            // Clean up tag_index
            let mut all_tags: Vec<String> = old.frontmatter.tags.clone();
            for t in &old.tags {
                if !all_tags.contains(t) {
                    all_tags.push(t.clone());
                }
            }
            for tag in all_tags {
                if let Some(paths) = self.index.tag_index.get_mut(&tag) {
                    paths.retain(|p| p != path);
                }
            }
        }
    }

    /// Resolve a wikilink to a note path in the index.
    pub fn resolve_link(&self, link: &Wikilink) -> Option<String> {
        resolve_target(&link.target, &self.index.notes)
    }

    /// Get all backlinks for a note by relative path.
    pub fn get_backlinks(&self, path: &str) -> Vec<&Backlink> {
        self.index
            .backlinks
            .get(path)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get graph data as (node_ids, edges) for D3.js rendering.
    pub fn get_graph_data(&self) -> (Vec<String>, Vec<GraphEdge>) {
        let nodes: Vec<String> = self.index.notes.keys().cloned().collect();
        (nodes, self.index.graph_edges.clone())
    }

    /// Get all tags with their note counts, sorted by count descending.
    pub fn get_tags(&self) -> Vec<(String, usize)> {
        let mut tags: Vec<(String, usize)> = self
            .index
            .tag_index
            .iter()
            .map(|(tag, paths)| (tag.clone(), paths.len()))
            .collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1));
        tags
    }

    /// Get all notes tagged with `tag`.
    pub fn get_notes_by_tag(&self, tag: &str) -> Vec<&NoteData> {
        self.index
            .tag_index
            .get(tag)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| self.index.notes.get(p))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Read-only access to the vault index.
    pub fn index(&self) -> &VaultIndex {
        &self.index
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a markdown file into [`NoteData`] and the extracted body string
/// (everything after frontmatter).
fn parse_note(path: &str, content: &str) -> (NoteData, String) {
    let (frontmatter, body) = split_and_parse_frontmatter(content);

    let headings = parse_headings(&body);
    let wikilinks = parse_wikilinks(&body);
    let inline_tags = parse_inline_tags(&body);

    // Merge tags: frontmatter first, then inline (deduplicated)
    let mut all_tags: Vec<String> = frontmatter.tags.clone();
    for tag in inline_tags {
        if !all_tags.contains(&tag) {
            all_tags.push(tag);
        }
    }

    // Title priority: frontmatter title > first H1 > filename stem
    let filename = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let title = frontmatter
        .title
        .clone()
        .or_else(|| {
            headings
                .iter()
                .find(|h| h.level == 1)
                .map(|h| h.text.clone())
        })
        .unwrap_or(filename);

    // Dates from frontmatter raw
    let created = frontmatter
        .raw
        .get("created")
        .and_then(|v| v.as_str())
        .map(String::from);
    let modified = frontmatter
        .raw
        .get("modified")
        .and_then(|v| v.as_str())
        .map(String::from);

    let word_count = body.split_whitespace().count();
    let line_count = content.lines().count();

    let note = NoteData {
        path: path.to_string(),
        title,
        tags: all_tags,
        frontmatter,
        outgoing_links: wikilinks,
        headings,
        created,
        modified,
        word_count,
        line_count,
    };

    (note, body)
}

/// Split content into frontmatter + body. Returns (Frontmatter, body_string).
///
/// Handles BOM, `\r\n` line endings. No yaml crate — hand-rolled parser.
fn split_and_parse_frontmatter(content: &str) -> (Frontmatter, String) {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    let mut lines = content.split('\n');
    if lines.next() != Some("---") {
        return (Frontmatter::default(), content.to_string());
    }

    let mut fm_lines: Vec<&str> = Vec::new();

    for line in lines.by_ref() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed == "---" {
            // Calculate byte offset past the closing ---
            let total = fm_lines.len() + 2; // opening + closing + fm lines
            let offset = content
                .split('\n')
                .take(total)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(content.len());
            let body = content[offset..].to_string();
            let fm = parse_frontmatter_lines(&fm_lines);
            return (fm, body);
        }
        fm_lines.push(line.trim_end_matches('\r'));
    }

    // No closing --- found — treat entire file as body
    (Frontmatter::default(), content.to_string())
}

/// Parse frontmatter key:value lines into a [`Frontmatter`].
///
/// Handles `[a, b]` inline arrays, `- item` multi-line lists, and quoted
/// strings. No yaml crate.
fn parse_frontmatter_lines(fm_lines: &[&str]) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let mut current_key: Option<String> = None;

    for &line in fm_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // List item: "- value"
        if let Some(value) = trimmed.strip_prefix("- ") {
            if let Some(ref key) = current_key {
                let clean = clean_yaml_value(value.trim());
                match key.as_str() {
                    "tags" => fm.tags.push(clean),
                    "aliases" => fm.aliases.push(clean),
                    _ => {} // unknown list keys ignored
                }
            }
            continue;
        }

        // Key: value
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_lowercase();
            let value = trimmed[colon_pos + 1..].trim();
            current_key = Some(key.clone());

            if value.is_empty() {
                // Multi-line list follows on subsequent lines
                continue;
            }

            match key.as_str() {
                "title" => {
                    let clean = clean_yaml_value(value);
                    fm.title = Some(clean.clone());
                    fm.raw
                        .insert("title".into(), serde_json::Value::String(clean));
                }
                "tags" => {
                    let items = if value.starts_with('[') {
                        parse_inline_array(value)
                    } else {
                        vec![clean_yaml_value(value)]
                    };
                    fm.tags = items.clone();
                    fm.raw.insert("tags".into(), serde_json::json!(items));
                }
                "aliases" => {
                    let items = if value.starts_with('[') {
                        parse_inline_array(value)
                    } else {
                        vec![clean_yaml_value(value)]
                    };
                    fm.aliases = items.clone();
                    fm.raw.insert("aliases".into(), serde_json::json!(items));
                }
                _ => {
                    let clean = clean_yaml_value(value);
                    fm.raw.insert(key, serde_json::Value::String(clean));
                }
            }
        }
    }

    fm
}

/// Strip surrounding quotes from a YAML string value.
fn clean_yaml_value(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

/// Parse an inline YAML array like `[a, b, "c d"]`.
fn parse_inline_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Vec::new();
    }
    s[1..s.len() - 1]
        .split(',')
        .map(|item| clean_yaml_value(item.trim()))
        .filter(|v| !v.is_empty())
        .collect()
}

/// Extract wikilinks from body text.
///
/// Supports: `[[Note]]`, `[[Note#Heading]]`, `[[Note^block]]`,
/// `[[Note|Display]]`, and combinations `[[Note#H^b|D]]`.
fn parse_wikilinks(body: &str) -> Vec<Wikilink> {
    let re = &*RE_WIKILINK;

    re.captures_iter(body)
        .filter_map(|cap| {
            let inner = cap.get(1)?.as_str();
            let raw = cap.get(0)?.as_str().to_string();

            // Split off display text (after last |)
            let (main, display_text) = if let Some(pipe_pos) = inner.rfind('|') {
                (
                    inner[..pipe_pos].trim(),
                    Some(inner[pipe_pos + 1..].trim().to_string()),
                )
            } else {
                (inner, None)
            };

            // Split off block ID (after ^)
            let (main, block_id) = if let Some(caret_pos) = main.find('^') {
                (
                    main[..caret_pos].trim(),
                    Some(main[caret_pos + 1..].trim().to_string()),
                )
            } else {
                (main, None)
            };

            // Split off heading (after #)
            let (target, heading) = if let Some(hash_pos) = main.find('#') {
                (
                    main[..hash_pos].trim(),
                    Some(main[hash_pos + 1..].trim().to_string()),
                )
            } else {
                (main.trim(), None)
            };

            if target.is_empty() {
                return None;
            }

            Some(Wikilink {
                raw,
                target: target.to_string(),
                heading: heading.map(String::from),
                display_text: display_text.map(String::from),
                block_id: block_id.map(String::from),
            })
        })
        .collect()
}

/// Extract inline `#tag` from body, skipping fenced code blocks.
fn parse_inline_tags(body: &str) -> Vec<String> {
    let without_code = strip_code_blocks(body);
    let re = &*RE_INLINE_TAG;
    re.captures_iter(&without_code)
        .map(|cap| cap[0][1..].to_string())
        .collect()
}

/// Remove fenced code blocks (` ``` … ``` `) from text.
fn strip_code_blocks(text: &str) -> String {
    let mut result_lines: Vec<&str> = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if !in_block {
            result_lines.push(line);
        }
    }
    result_lines.join("\n")
}

/// Extract markdown headings from body text.
fn parse_headings(body: &str) -> Vec<Heading> {
    let re = &*RE_HEADING;
    re.captures_iter(body)
        .map(|cap| {
            let level = cap[1].len() as u8;
            let text = cap[2].trim().to_string();
            let slug = slugify(&text);
            Heading { level, text, slug }
        })
        .collect()
}

/// Create a URL-friendly slug from heading text.
fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Resolve a wikilink target string to a note path in the index.
///
/// Resolution order:
/// 1. Exact path match with `.md` appended
/// 2. Exact path match as-is
/// 3. Match by note title
/// 4. Match by filename stem
fn resolve_target(target: &str, notes: &HashMap<String, NoteData>) -> Option<String> {
    let target = target.trim();

    // 1. Exact path with .md
    let with_ext = if target.ends_with(".md") {
        target.to_string()
    } else {
        format!("{target}.md")
    };
    if notes.contains_key(&with_ext) {
        return Some(with_ext);
    }

    // 2. Exact path as-is
    if notes.contains_key(target) {
        return Some(target.to_string());
    }

    // 3. Match by title
    for (path, note) in notes {
        if note.title == target {
            return Some(path.clone());
        }
    }

    // 4. Match by filename stem
    for (path, _note) in notes {
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if stem == target {
            return Some(path.clone());
        }
    }

    // 5. Case-insensitive fallback (for case-insensitive filesystems)
    let target_lower = target.to_lowercase();
    for (path, _note) in notes {
        let path_lower = path.to_lowercase();
        if path_lower == target_lower || path_lower == format!("{target_lower}.md") {
            return Some(path.clone());
        }
    }
    for (path, note) in notes {
        if note.title.to_lowercase() == target_lower {
            return Some(path.clone());
        }
    }
    for (path, _note) in notes {
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if stem.to_lowercase() == target_lower {
            return Some(path.clone());
        }
    }

    None
}

/// Extract the line containing a wikilink as context snippet.
fn extract_link_context(body: &str, link: &Wikilink) -> String {
    for line in body.lines() {
        if line.contains(&link.raw) {
            return line.trim().to_string();
        }
    }
    link.raw.clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_vault() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // ---- test_parse_frontmatter -------------------------------------------

    #[test]
    fn test_parse_frontmatter() {
        let content = String::from("---\ntitle: My Note\ntags: [rust, programming]\n")
            + "aliases: [My Note, MN]\\ncreated: 2024-01-01\\n"
            + "---\\n# Hello\\nContent here";
        let (fm, body) = split_and_parse_frontmatter(&content);
        assert_eq!(fm.title, Some("My Note".to_string()));
        assert_eq!(fm.tags, vec!["rust", "programming"]);
        assert_eq!(fm.aliases, vec!["My Note", "MN"]);
        assert_eq!(
            fm.raw.get("created").and_then(|v| v.as_str()),
            Some("2024-01-01")
        );
        assert!(!body.contains("title:"));
        assert!(body.contains("# Hello"));
    }

    // ---- test_parse_frontmatter_none ---------------------------------------

    #[test]
    fn test_parse_frontmatter_none() {
        let content = "# No Frontmatter\nJust some content";
        let (fm, body) = split_and_parse_frontmatter(content);
        assert!(fm.title.is_none());
        assert!(fm.tags.is_empty());
        assert!(fm.aliases.is_empty());
        assert_eq!(body, content);
    }

    // ---- test_parse_frontmatter_multiline_list ----------------------------

    #[test]
    fn test_parse_frontmatter_multiline_list() {
        let content = "---\ntitle: List Test\ntags:\n  - rust\n  - web\n  - dev\n---\nBody";
        let (fm, _body) = split_and_parse_frontmatter(content);
        assert_eq!(fm.title, Some("List Test".to_string()));
        assert_eq!(fm.tags, vec!["rust", "web", "dev"]);
    }

    // ---- test_parse_frontmatter_quoted ------------------------------------

    #[test]
    fn test_parse_frontmatter_quoted() {
        let content = "---\ntitle: \"Quoted Title\"\n---\nBody";
        let (fm, _body) = split_and_parse_frontmatter(content);
        assert_eq!(fm.title, Some("Quoted Title".to_string()));
    }

    // ---- test_parse_frontmatter_no_closing_delim --------------------------

    #[test]
    fn test_parse_frontmatter_no_closing_delim() {
        let content = "---\ntitle: Broken\nNo closing delimiter";
        let (fm, body) = split_and_parse_frontmatter(content);
        assert!(fm.title.is_none());
        assert_eq!(body, content);
    }

    // ---- test_parse_wikilinks ---------------------------------------------

    #[test]
    fn test_parse_wikilinks() {
        let body = "See [[Note A]] and [[Note B#Heading]] and [[Note C|Display]]".to_string()
            + " and [[Note D^block]] and [[Note E#Sec^blk|Disp]]";
        let links = parse_wikilinks(&body);
        assert_eq!(links.len(), 5);

        assert_eq!(links[0].target, "Note A");
        assert!(links[0].heading.is_none());
        assert!(links[0].display_text.is_none());
        assert!(links[0].block_id.is_none());

        assert_eq!(links[1].target, "Note B");
        assert_eq!(links[1].heading.as_deref(), Some("Heading"));

        assert_eq!(links[2].target, "Note C");
        assert_eq!(links[2].display_text.as_deref(), Some("Display"));

        assert_eq!(links[3].target, "Note D");
        assert_eq!(links[3].block_id.as_deref(), Some("block"));

        // Combined: target + heading + block + display
        assert_eq!(links[4].target, "Note E");
        assert_eq!(links[4].heading.as_deref(), Some("Sec"));
        assert_eq!(links[4].block_id.as_deref(), Some("blk"));
        assert_eq!(links[4].display_text.as_deref(), Some("Disp"));
    }

    // ---- test_parse_inline_tags -------------------------------------------

    #[test]
    fn test_parse_inline_tags() {
        let body = "Some #rust and #web-dev content\n```code\n#notatag here\n```\n".to_string()
            + "More #programming";
        let tags = parse_inline_tags(&body);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"web-dev".to_string()));
        assert!(tags.contains(&"programming".to_string()));
        assert!(!tags.contains(&"notatag".to_string()));
    }

    // ---- test_parse_headings ----------------------------------------------

    #[test]
    fn test_parse_headings() {
        let body = "# Title\n## Section\n### Subsection\nSome text\n#### Deep";
        let headings = parse_headings(body);
        assert_eq!(headings.len(), 4);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Title");
        assert_eq!(headings[0].slug, "title");
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].slug, "section");
        assert_eq!(headings[2].slug, "subsection");
        assert_eq!(headings[3].level, 4);
    }

    // ---- test_headings_skip_frontmatter -----------------------------------

    #[test]
    fn test_headings_skip_frontmatter() {
        let content = "---\ntitle: T\n---\n# Real Heading";
        let (note, _) = parse_note("file.md", content);
        assert_eq!(note.headings.len(), 1);
        assert_eq!(note.headings[0].text, "Real Heading");
    }

    // ---- test_title_priority ----------------------------------------------

    #[test]
    fn test_title_priority() {
        // Frontmatter title wins
        let (note, _) = parse_note("file.md", "---\ntitle: FM Title\n---\n# H1 Title\nContent");
        assert_eq!(note.title, "FM Title");

        // H1 wins when no frontmatter title
        let (note, _) = parse_note("file.md", "---\ntags: [a]\n---\n# H1 Title\nContent");
        assert_eq!(note.title, "H1 Title");

        // Filename wins when neither frontmatter title nor H1
        let (note, _) = parse_note("my-file.md", "Just content");
        assert_eq!(note.title, "my-file");
    }

    // ---- test_rebuild_creates_graph ----------------------------------------

    #[test]
    fn test_rebuild_creates_graph() {
        let dir = setup_vault();
        fs::write(dir.path().join("a.md"), "[[b]]\n").unwrap();
        fs::write(dir.path().join("b.md"), "# B\nLinks to [[a]]\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        let (nodes, edges) = indexer.get_graph_data();
        assert_eq!(nodes.len(), 2);
        assert!(
            edges
                .iter()
                .any(|e| e.source == "a.md" && e.target == "b.md")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source == "b.md" && e.target == "a.md")
        );
    }

    // ---- test_backlinks_reverse_index --------------------------------------

    #[test]
    fn test_backlinks_reverse_index() {
        let dir = setup_vault();
        fs::write(dir.path().join("a.md"), "Link to [[b]] here\n").unwrap();
        fs::write(dir.path().join("b.md"), "# B Note\nNo outgoing links\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        let backlinks = indexer.get_backlinks("b.md");
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].source_path, "a.md");
        assert!(backlinks[0].context.contains("[[b]]"));
    }

    // ---- test_remove_file -------------------------------------------------

    #[test]
    fn test_remove_file() {
        let dir = setup_vault();
        fs::write(dir.path().join("a.md"), "Link to [[b]]\n").unwrap();
        fs::write(dir.path().join("b.md"), "# B\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        assert_eq!(indexer.get_backlinks("b.md").len(), 1);
        assert_eq!(indexer.get_graph_data().0.len(), 2);

        indexer.remove_file("a.md");

        // a.md gone from notes
        assert_eq!(indexer.get_graph_data().0.len(), 1);
        // backlinks to b.md cleaned up
        assert_eq!(indexer.get_backlinks("b.md").len(), 0);
        // graph edges from a gone
        assert!(indexer.get_graph_data().1.is_empty());
    }

    // ---- test_resolve_link ------------------------------------------------

    #[test]
    fn test_resolve_link() {
        let dir = setup_vault();
        fs::write(dir.path().join("my-note.md"), "# My Note\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        // Resolve by filename stem
        let link = Wikilink {
            raw: "[[my-note]]".into(),
            target: "my-note".into(),
            heading: None,
            display_text: None,
            block_id: None,
        };
        assert_eq!(indexer.resolve_link(&link), Some("my-note.md".to_string()));

        // Resolve by title
        let link_by_title = Wikilink {
            raw: "[[My Note]]".into(),
            target: "My Note".into(),
            heading: None,
            display_text: None,
            block_id: None,
        };
        assert_eq!(
            indexer.resolve_link(&link_by_title),
            Some("my-note.md".to_string())
        );

        // Unresolvable returns None
        let link_ghost = Wikilink {
            raw: "[[nonexistent]]".into(),
            target: "nonexistent".into(),
            heading: None,
            display_text: None,
            block_id: None,
        };
        assert_eq!(indexer.resolve_link(&link_ghost), None);
    }

    // ---- test_tags_index --------------------------------------------------

    #[test]
    fn test_tags_index() {
        let dir = setup_vault();
        fs::write(
            dir.path().join("a.md"),
            "---\ntags: [rust, web]\n---\n# A\n",
        )
        .unwrap();
        fs::write(dir.path().join("b.md"), "# B\nSome #rust content\n").unwrap();
        fs::write(dir.path().join("c.md"), "# C\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        let tags = indexer.get_tags();
        assert!(tags.iter().any(|(t, c)| t == "rust" && *c == 2));
        assert!(tags.iter().any(|(t, c)| t == "web" && *c == 1));

        let rust_notes = indexer.get_notes_by_tag("rust");
        assert_eq!(rust_notes.len(), 2);
    }

    // ---- test_index_file_incremental ---------------------------------------

    #[test]
    fn test_index_file_incremental() {
        let dir = setup_vault();
        fs::write(dir.path().join("a.md"), "# A\n").unwrap();
        fs::write(dir.path().join("b.md"), "# B\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();
        assert_eq!(indexer.get_graph_data().0.len(), 2);
        assert!(indexer.get_graph_data().1.is_empty());

        // Add link from a → b via incremental update
        fs::write(dir.path().join("a.md"), "# A\nLinks to [[b]]\n").unwrap();
        indexer.index_file(&vault, "a.md").unwrap();

        let edges = &indexer.get_graph_data().1;
        assert!(
            edges
                .iter()
                .any(|e| e.source == "a.md" && e.target == "b.md")
        );
        assert_eq!(indexer.get_backlinks("b.md").len(), 1);
    }

    // ---- test_word_and_line_count -----------------------------------------

    #[test]
    fn test_word_and_line_count() {
        let content = "---\ntitle: T\n---\nhello world\nfoo bar baz";
        let (note, _) = parse_note("test.md", content);
        assert_eq!(note.word_count, 5); // "hello world foo bar baz"
        assert_eq!(note.line_count, 4); // ---, title: T, ---, hello world, foo bar baz
    }

    // ---- test_slugify -----------------------------------------------------

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Rust & Programming"), "rust-programming");
        assert_eq!(slugify("  Extra   Spaces  "), "extra-spaces");
        assert_eq!(slugify("CamelCase"), "camelcase");
    }

    // ---- test_indexer_error_display ----------------------------------------

    #[test]
    fn test_indexer_error_display() {
        let err = IndexerError::Parse("bad frontmatter".into());
        assert_eq!(format!("{err}"), "indexer parse error: bad frontmatter");

        let io_err = io::Error::new(io::ErrorKind::NotFound, "file gone");
        let err = IndexerError::Io(io_err);
        assert!(format!("{err}").contains("file gone"));
    }

    // ---- test_nested_dirs_rebuild ------------------------------------------

    #[test]
    fn test_nested_dirs_rebuild() {
        let dir = setup_vault();
        fs::create_dir_all(dir.path().join("sub/deep")).unwrap();
        fs::write(dir.path().join("sub/a.md"), "# A\n[[b]]\n").unwrap();
        fs::write(dir.path().join("sub/deep/b.md"), "# B\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        let (nodes, edges) = indexer.get_graph_data();
        assert_eq!(nodes.len(), 2);
        assert!(
            edges
                .iter()
                .any(|e| e.source == "sub/a.md" && e.target == "sub/deep/b.md")
        );
    }

    // ---- test_hidden_files_skipped -----------------------------------------

    #[test]
    fn test_hidden_files_skipped() {
        let dir = setup_vault();
        fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        fs::write(dir.path().join(".hidden/secret.md"), "# Secret\n").unwrap();
        fs::write(dir.path().join("visible.md"), "# Visible\n").unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        let (nodes, _) = indexer.get_graph_data();
        assert_eq!(nodes.len(), 1);
        assert!(nodes.contains(&"visible.md".to_string()));
    }

    // ---- test_unresolved_links_no_panic -----------------------------------

    #[test]
    fn test_unresolved_links_no_panic() {
        let dir = setup_vault();
        fs::write(
            dir.path().join("orphan.md"),
            "Links to [[ghost]] and [[phantom#Sec]]\n",
        )
        .unwrap();

        let vault = VaultManager::new(dir.path());
        let mut indexer = Indexer::new(dir.path().to_path_buf());
        indexer.rebuild(vault.root()).unwrap();

        assert_eq!(indexer.get_graph_data().0.len(), 1);
        assert!(indexer.get_graph_data().1.is_empty());
        assert!(indexer.get_backlinks("ghost.md").is_empty());
    }
}
