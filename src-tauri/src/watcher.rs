use std::path::PathBuf;
use tokio::sync::mpsc;

/// File-system watcher that emits events over a channel.
/// Placeholder — implementation pending.
pub struct VaultWatcher {
    root: PathBuf,
    _tx: mpsc::Sender<WatchEvent>,
    _rx: Option<mpsc::Receiver<WatchEvent>>,
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
}

impl VaultWatcher {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            root: root.into(),
            _tx: tx,
            _rx: Some(rx),
        }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Start watching. Non-blocking.
    pub fn start(&mut self) -> Result<(), String> {
        // TODO: notify-based file watcher
        Ok(())
    }

    /// Stop watching.
    pub fn stop(&self) {
        // TODO: stop watcher
    }
}
