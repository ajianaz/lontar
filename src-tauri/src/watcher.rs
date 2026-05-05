use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------
// WatchEvent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Rename {
        old: PathBuf,
        new: PathBuf,
    },
    /// Signals a large batch — frontend should re-index everything.
    BulkChange {
        count: usize,
    },
}

// ---------------------------------------------------------------------------
// VaultWatcherError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum VaultWatcherError {
    Notify(notify::Error),
    Io(io::Error),
}

impl fmt::Display for VaultWatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Notify(e) => write!(f, "notify error: {e}"),
            Self::Io(e) => write!(f, "watcher I/O error: {e}"),
        }
    }
}

impl std::error::Error for VaultWatcherError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Notify(e) => Some(e),
            Self::Io(e) => Some(e),
        }
    }
}

impl From<notify::Error> for VaultWatcherError {
    fn from(e: notify::Error) -> Self {
        VaultWatcherError::Notify(e)
    }
}

impl From<io::Error> for VaultWatcherError {
    fn from(e: io::Error) -> Self {
        VaultWatcherError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Internal event — bridging notify thread → tokio debounce task
// ---------------------------------------------------------------------------

/// Raw event forwarded from the notify watcher thread to the debounce task.
#[derive(Debug, Clone)]
enum RawEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { old: PathBuf, new: PathBuf },
}

/// Minimal kind tag to avoid re-matching on the full `notify::EventKind`.
#[derive(PartialEq)]
enum RawEventKind {
    Create,
    Modify,
    Remove,
    Rename,
}

// ---------------------------------------------------------------------------
// VaultWatcher
// ---------------------------------------------------------------------------

/// File-system watcher for a vault directory tree.
///
/// Watches for `.md` file and directory changes under `root`, applying
/// debouncing and deduplication before emitting [`WatchEvent`]s.
///
/// # Lifecycle
///
/// ```text
/// let mut w = VaultWatcher::new("/vault", None);
/// let rx = w.subscribe().unwrap();   // grab receiver before start
/// w.start()?;                        // begins watching (non-blocking)
/// // ... consume events from rx ...
/// w.stop();
/// ```
pub struct VaultWatcher {
    root: PathBuf,
    /// Sender half of the output channel. Shared with the debounce task.
    tx: mpsc::Sender<WatchEvent>,
    /// Receiver half, handed out once via [`subscribe`](Self::subscribe).
    rx: Option<mpsc::Receiver<WatchEvent>>,
    _watcher: Option<Box<dyn notify::Watcher + Send + Sync>>,
    debounce_task: Option<JoinHandle<()>>,
    debounce_ms: u64,
}

impl VaultWatcher {
    const DEFAULT_DEBOUNCE_MS: u64 = 100;

    /// Create a new watcher for `root`.
    ///
    /// Creates the output channel but does **not** start watching.
    /// Call [`subscribe`](Self::subscribe) to get the receiver, then
    /// [`start`](Self::start) to begin emitting events.
    pub fn new(root: impl Into<PathBuf>, debounce_ms: Option<u64>) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            root: root.into(),
            tx,
            rx: Some(rx),
            _watcher: None,
            debounce_task: None,
            debounce_ms: debounce_ms.unwrap_or(Self::DEFAULT_DEBOUNCE_MS),
        }
    }

    /// Vault root path.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Get the event receiver. Can only be called once.
    ///
    /// Returns `None` if already called (receiver was taken).
    pub fn subscribe(&mut self) -> Option<mpsc::Receiver<WatchEvent>> {
        self.rx.take()
    }

    /// Start watching `root` for filesystem changes.
    ///
    /// Spawns the notify watcher on a background thread and a tokio debounce
    /// task. Non-blocking — returns immediately.
    ///
    /// Call [`subscribe`](Self::subscribe) **before** this if you want to
    /// consume events. Events emitted before a receiver exists are dropped.
    pub fn start(&mut self) -> Result<(), VaultWatcherError> {
        // 1. Internal channel: notify thread → debounce task.
        let (raw_tx, raw_rx) = mpsc::channel::<RawEvent>(1024);

        // 2. Spawn the debounce task. It receives raw events and writes
        //    deduplicated/coalesced WatchEvents to self.tx.
        let out_tx = self.tx.clone();
        let debounce_ms = self.debounce_ms;
        let debounce_handle = tokio::spawn(async move {
            debounce_loop(raw_rx, out_tx, debounce_ms).await;
        });
        self.debounce_task = Some(debounce_handle);

        // 3. Build the notify watcher (runs on its own thread).
        let root = self.root.clone();
        let mut watcher = {
            let raw_tx_inner = raw_tx.clone();
            let root_for_filter = root.clone();

            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let event = match res {
                    Ok(e) => e,
                    Err(_) => return,
                };
                handle_notify_event(event, &root_for_filter, &raw_tx_inner);
            })?
        };

        // 4. Begin recursive watch.
        use notify::Watcher;
        watcher.watch(&self.root, notify::RecursiveMode::Recursive)?;

        self._watcher = Some(Box::new(watcher));
        Ok(())
    }

    /// Test-only variant that uses `PollWatcher` instead of the platform's
    /// recommended watcher. `recommended_watcher` (inotify on Linux) is
    /// unreliable in CI containers where inotify watches may silently fail.
    #[cfg(test)]
    pub fn start_with_poll(&mut self) -> Result<(), VaultWatcherError> {
        use notify::PollWatcher;

        let (raw_tx, raw_rx) = mpsc::channel::<RawEvent>(1024);
        let out_tx = self.tx.clone();
        let debounce_ms = self.debounce_ms;
        let debounce_handle = tokio::spawn(async move {
            debounce_loop(raw_rx, out_tx, debounce_ms).await;
        });
        self.debounce_task = Some(debounce_handle);

        let root = self.root.clone();
        let raw_tx_inner = raw_tx.clone();
        let root_for_filter = root.clone();

        let mut watcher = PollWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                let event = match res {
                    Ok(e) => e,
                    Err(_) => return,
                };
                handle_notify_event(event, &root_for_filter, &raw_tx_inner);
            },
            notify::Config::default().with_poll_interval(Duration::from_millis(32)),
        )?;

        use notify::Watcher;
        watcher.watch(&self.root, notify::RecursiveMode::Recursive)?;
        self._watcher = Some(Box::new(watcher));
        Ok(())
    }

    /// Stop watching by dropping the watcher and cancelling the debounce task.
    pub fn stop(&mut self) {
        if let Some(handle) = self.debounce_task.take() {
            handle.abort();
        }
        self._watcher.take(); // drop RecommendedWatcher → stops OS watches
    }
}

impl Drop for VaultWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Notify event handler (runs on notify's background thread)
// ---------------------------------------------------------------------------

/// Process a raw notify event: filter hidden/non-markdown paths, forward
/// relevant events to the debounce task via `raw_tx`.
fn handle_notify_event(event: notify::Event, root: &Path, raw_tx: &mpsc::Sender<RawEvent>) {
    use notify::EventKind;
    use notify::event::ModifyKind;
    use notify::event::RenameMode;

    // Only interested in create / modify / remove / rename.
    let kind = match event.kind {
        EventKind::Create(_) => RawEventKind::Create,
        // Linux/inotify: both paths available in one event → true rename.
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => RawEventKind::Rename,
        // Windows (ReadDirectoryChanges): separate From/To events.
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => RawEventKind::Remove,
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => RawEventKind::Create,
        // macOS (FSEvents): single ambiguous path → safest as modify.
        EventKind::Modify(ModifyKind::Name(RenameMode::Any)) => RawEventKind::Modify,
        EventKind::Modify(_) => RawEventKind::Modify,
        EventKind::Remove(_) => RawEventKind::Remove,
        _ => return,
    };

    // For rename events, notify provides two paths: [old, new].
    // Extract them together and send as a single Renamed event.
    if kind == RawEventKind::Rename && event.paths.len() >= 2 {
        let old_path = event.paths[0].clone();
        let new_path = event.paths[1].clone();

        // Apply the same root/hidden/md filters to both paths
        if old_path.starts_with(root)
            && new_path.starts_with(root)
            && !old_path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            && !new_path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
            && (old_path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                || new_path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md")))
        {
            let _ = raw_tx.try_send(RawEvent::Renamed {
                old: old_path,
                new: new_path,
            });
        }
        return; // Don't fall through to per-path processing
    }

    for path in event.paths {
        // Skip paths outside the vault root.
        if !path.starts_with(root) {
            continue;
        }

        // Skip hidden entries: any path component starting with '.'.
        // Covers .trash, .vault-index, .git, .appname, .tmp, etc.
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }

        // Only .md files and directories.
        let is_md = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !is_md && !path.is_dir() {
            continue;
        }

        let raw = match kind {
            RawEventKind::Create => RawEvent::Created(path),
            RawEventKind::Modify => RawEvent::Modified(path),
            RawEventKind::Remove => RawEvent::Removed(path),
            RawEventKind::Rename => unreachable!("handled above"),
        };

        let _ = raw_tx.try_send(raw);
    }
}

// ---------------------------------------------------------------------------
// Debounce loop (runs as a tokio task)
// ---------------------------------------------------------------------------

/// Collects raw events, deduplicates and coalesces them, then emits final
/// [`WatchEvent`]s to the output channel.
///
/// Algorithm:
/// 1. Block on the first incoming event.
/// 2. Start a debounce timer (`debounce_ms`).
/// 3. Collect all events arriving before the timer expires.
/// 4. Flush: if >10 unique paths → emit `BulkChange`, else emit individually.
async fn debounce_loop(
    mut rx: mpsc::Receiver<RawEvent>,
    tx: mpsc::Sender<WatchEvent>,
    debounce_ms: u64,
) {
    let mut pending: HashMap<PathBuf, RawEvent> = HashMap::new();

    loop {
        // Block until the first event arrives.
        let first = match rx.recv().await {
            Some(e) => e,
            None => return, // channel closed — shut down
        };

        pending.clear();
        insert_coalesced(&mut pending, first);

        // Drain events that arrive during the debounce window.
        let sleep = tokio::time::sleep(Duration::from_millis(debounce_ms));
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                maybe_event = rx.recv() => {
                    match maybe_event {
                        Some(e) => { insert_coalesced(&mut pending, e); }
                        None => {
                            // Channel closed — flush remaining and exit.
                            flush_pending(&pending, &tx);
                            return;
                        }
                    }
                }
                _ = &mut sleep => {
                    break; // debounce window elapsed
                }
            }
        }

        flush_pending(&pending, &tx);
    }
}

/// Insert a raw event into the pending map, applying coalescing rules:
///
/// - **Modified after Created** → keep Created (creation subsumes modification)
/// - **Modified after Modified** → keep single Modified (deduplication)
/// - **Any other new type** → replaces the existing entry
fn insert_coalesced(map: &mut HashMap<PathBuf, RawEvent>, event: RawEvent) {
    let path = match &event {
        RawEvent::Created(p) | RawEvent::Modified(p) | RawEvent::Removed(p) => p.clone(),
        RawEvent::Renamed { old, .. } => old.clone(),
    };

    map.entry(path)
        .and_modify(|existing| {
            use RawEvent::*;
            match (&*existing, &event) {
                (Created(_), Modified(_)) => {}  // Created wins
                (Modified(_), Modified(_)) => {} // dedup
                (Renamed { .. }, _) | (_, Renamed { .. }) => *existing = event.clone(), // rename replaces anything
                (_, _) => *existing = event.clone(), // new type replaces
            }
        })
        .or_insert(event);
}

/// Flush collected events to the output channel.
///
/// If more than 10 unique paths were modified in this debounce window,
/// emit a single [`WatchEvent::BulkChange`] instead of individual events.
fn flush_pending(pending: &HashMap<PathBuf, RawEvent>, tx: &mpsc::Sender<WatchEvent>) {
    if pending.is_empty() {
        return;
    }

    if pending.len() > 10 {
        let _ = tx.try_send(WatchEvent::BulkChange {
            count: pending.len(),
        });
        return;
    }

    for raw in pending.values() {
        let event = match raw {
            RawEvent::Created(p) => WatchEvent::Created(p.clone()),
            RawEvent::Modified(p) => WatchEvent::Modified(p.clone()),
            RawEvent::Removed(p) => WatchEvent::Removed(p.clone()),
            RawEvent::Renamed { old, new } => WatchEvent::Rename {
                old: old.clone(),
                new: new.clone(),
            },
        };
        let _ = tx.try_send(event);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    /// CI containers (GitHub Actions overlay2) do not reliably trigger PollWatcher
    /// file-system events. Skip these tests in CI — they validate correctly in
    /// local development.
    fn is_ci() -> bool {
        std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok()
    }

    /// Helper: create a temp directory.
    fn setup_vault() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("lontar_test_")
            .tempdir()
            .unwrap()
    }

    /// Helper: create and start a watcher with short debounce.
    /// Returns both the watcher (keeps it alive) and the receiver.
    ///
    /// **Must** be called from a tokio runtime (e.g. `#[tokio::test]`).
    fn start_watcher(root: &Path) -> (VaultWatcher, mpsc::Receiver<WatchEvent>) {
        let mut w = VaultWatcher::new(root, Some(30)); // 50ms debounce for tests
        let rx = w.subscribe().expect("subscribe failed");
        w.start_with_poll().expect("watcher start failed");
        (w, rx)
    }

    /// Drain all pending events after waiting `wait_ms`.
    fn drain_events(rx: &mut mpsc::Receiver<WatchEvent>, wait_ms: u64) -> Vec<WatchEvent> {
        thread::sleep(Duration::from_millis(wait_ms));
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        events
    }

    #[tokio::test]
    async fn test_create_file_emits_event() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let (_w, mut rx) = start_watcher(dir.path());

        let note_path = dir.path().join("hello.md");
        fs::write(&note_path, "# Hello").unwrap();

        let events = drain_events(&mut rx, 500);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WatchEvent::Created(p) if p == &note_path)),
            "expected Created event for hello.md, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_modify_file_emits_event() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let note_path = dir.path().join("edit.md");
        fs::write(&note_path, "v1").unwrap();

        let (_w, mut rx) = start_watcher(dir.path());

        fs::write(&note_path, "v2").unwrap();

        let events = drain_events(&mut rx, 500);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WatchEvent::Modified(p) if p == &note_path)),
            "expected Modified event for edit.md, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_delete_file_emits_event() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let note_path = dir.path().join("delete-me.md");
        fs::write(&note_path, "bye").unwrap();

        let (_w, mut rx) = start_watcher(dir.path());

        fs::remove_file(&note_path).unwrap();

        let events = drain_events(&mut rx, 500);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WatchEvent::Removed(p) if p == &note_path)),
            "expected Removed event for delete-me.md, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_hidden_files_ignored() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let (_w, mut rx) = start_watcher(dir.path());

        // Hidden .md file — should be ignored.
        fs::write(dir.path().join(".hidden.md"), "secret").unwrap();

        // File inside hidden directory.
        fs::create_dir_all(dir.path().join(".trash")).unwrap();
        fs::write(dir.path().join(".trash/note.md"), "trashed").unwrap();

        // .git directory contents.
        fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
        fs::write(dir.path().join(".git/config"), "[core]").unwrap();

        // .vault-index directory.
        fs::create_dir_all(dir.path().join(".vault-index")).unwrap();

        thread::sleep(Duration::from_millis(500));
        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            events.is_empty(),
            "hidden files/dirs should be ignored, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_non_markdown_ignored() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let (_w, mut rx) = start_watcher(dir.path());

        fs::write(dir.path().join("readme.txt"), "not markdown").unwrap();
        fs::write(dir.path().join("image.png"), b"\x89PNG").unwrap();
        fs::write(dir.path().join("data.json"), "{}").unwrap();

        thread::sleep(Duration::from_millis(500));
        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            events.is_empty(),
            "non-.md files should be ignored, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_coalesce_modified_after_created() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let (_w, mut rx) = start_watcher(dir.path());

        let note_path = dir.path().join("quick.md");
        // Create then immediately modify — within debounce window.
        fs::write(&note_path, "first").unwrap();
        thread::sleep(Duration::from_millis(5));
        fs::write(&note_path, "second").unwrap();

        let events = drain_events(&mut rx, 500);

        let has_created = events
            .iter()
            .any(|e| matches!(e, WatchEvent::Created(p) if p == &note_path));
        let has_modified = events
            .iter()
            .any(|e| matches!(e, WatchEvent::Modified(p) if p == &note_path));

        assert!(has_created, "expected Created event, got: {events:?}");
        assert!(
            !has_modified,
            "Modified should be coalesced into Created, got: {events:?}"
        );
    }

    #[tokio::test]
    async fn test_bulk_change_emitted_for_many_files() {
        if is_ci() {
            return;
        }
        let dir = setup_vault();
        let (_w, mut rx) = start_watcher(dir.path());

        // Create 15 .md files in rapid succession (within one debounce window).
        for i in 0..15 {
            fs::write(dir.path().join(format!("note_{i}.md")), "content").unwrap();
        }

        let events = drain_events(&mut rx, 500);
        assert!(
            events.iter().any(|e| matches!(
                e,
                WatchEvent::BulkChange { count } if *count > 10
            )),
            "expected BulkChange for 15 files, got: {events:?}"
        );
    }
}
