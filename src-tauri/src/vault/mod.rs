use std::path::{Path, PathBuf};

/// Manages the note vault — a directory tree of markdown files.
pub struct VaultManager {
    root: PathBuf,
}

impl VaultManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Return the vault root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Verify the vault root exists and is a directory.
    pub fn is_valid(&self) -> bool {
        self.root.is_dir()
    }

    /// Collect all markdown files recursively.
    pub fn list_notes(&self) -> Vec<PathBuf> {
        let mut notes = Vec::new();
        if !self.is_valid() {
            return notes;
        }
        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                notes.push(path.to_path_buf());
            }
        }
        notes
    }
}

pub mod lock;
