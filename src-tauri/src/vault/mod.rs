use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::atomic_write::atomic_write_str;

pub mod lock;

// ---------------------------------------------------------------------------
// VaultError
// ---------------------------------------------------------------------------

/// Errors produced by vault operations.
#[derive(Debug)]
pub enum VaultError {
    /// Root does not exist or is not a directory.
    InvalidRoot(PathBuf),
    /// Root is not readable.
    NotReadable(PathBuf),
    /// Root is not writable.
    NotWritable(PathBuf),
    /// Wrapped I/O error.
    Io(io::Error),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRoot(p) => write!(f, "invalid vault root: {}", p.display()),
            Self::NotReadable(p) => write!(f, "vault root not readable: {}", p.display()),
            Self::NotWritable(p) => write!(f, "vault root not writable: {}", p.display()),
            Self::Io(e) => write!(f, "vault I/O error: {e}"),
        }
    }
}

impl std::error::Error for VaultError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for VaultError {
    fn from(e: io::Error) -> Self {
        VaultError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// TreeEntry
// ---------------------------------------------------------------------------

/// A node in the vault directory tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeEntry {
    /// A markdown file leaf.
    File { name: String },
    /// A directory containing sorted children (dirs first, then files).
    Dir {
        name: String,
        children: Vec<TreeEntry>,
    },
}

impl TreeEntry {
    /// Recursively build a tree from `dir`.
    ///
    /// * Skips hidden entries (name starts with `.`), which covers
    ///   `.appname/`, `.vault-index/`, `.trash/`, `.git/`, etc.
    /// * Only `.md` files appear as leaves.
    /// * Children are sorted: directories alphabetically, then files alphabetically.
    fn build_from_dir(dir: &Path) -> io::Result<Self> {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<TreeEntry> = Vec::new();

        let entries = match fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) => return Err(e),
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip hidden entries (covers .appname, .vault-index, .trash, .git, etc.)
            if file_name_str.starts_with('.') {
                continue;
            }

            let path = entry.path();

            if path.is_dir() {
                dirs.push(path);
            } else if path.is_file()
                && path.extension().is_some_and(|ext| ext == "md")
            {
                files.push(TreeEntry::File {
                    name: file_name_str.into_owned(),
                });
            }
        }

        // Sort dirs alphabetically by name.
        dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        // Sort files alphabetically.
        files.sort_by(|a, b| {
            let (TreeEntry::File { name: na }, TreeEntry::File { name: nb }) = (a, b) else {
                unreachable!("files vec only contains File variants")
            };
            na.cmp(nb)
        });

        // Recurse into subdirectories.
        let mut children: Vec<TreeEntry> = dirs
            .into_iter()
            .map(|d| TreeEntry::build_from_dir(&d))
            .collect::<Result<Vec<_>, _>>()?;

        children.extend(files);

        Ok(TreeEntry::Dir { name, children })
    }
}

// ---------------------------------------------------------------------------
// VaultManager
// ---------------------------------------------------------------------------

/// Manages a note vault — a directory tree of markdown files.
///
/// # Concurrency
///
/// Callers **must** acquire a [`lock::VaultLock`] before performing mutations.
/// `VaultManager` does not acquire the lock internally.
pub struct VaultManager {
    root: PathBuf,
}

impl VaultManager {
    /// Create a new `VaultManager` pointing at `root`.
    ///
    /// Does **not** validate the root. Use [`validate_vault`](Self::validate_vault)
    /// or [`open_vault`](Self::open_vault) for validated construction.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
        }
    }

    /// Open a vault, validating that the root exists and is read/writable.
    ///
    /// **Caller must acquire a [`lock::VaultLock`] before calling this.**
    pub fn open_vault(path: impl Into<PathBuf>) -> Result<Self, VaultError> {
        let vm = Self {
            root: path.into(),
        };
        vm.validate_vault()?;
        Ok(vm)
    }

    /// Vault root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    // ----- Validation -------------------------------------------------------

    /// Validate vault root: exists, is a directory, readable, and writable.
    pub fn validate_vault(&self) -> Result<(), VaultError> {
        let root = &self.root;

        if !root.exists() || !root.is_dir() {
            return Err(VaultError::InvalidRoot(root.clone()));
        }

        // Readability: can we list the directory?
        if fs::read_dir(root).is_err() {
            return Err(VaultError::NotReadable(root.clone()));
        }

        // Writability: can we create+remove a temp file?
        let probe = root.join(".lontar-probe-writable");
        match fs::write(&probe, b"") {
            Ok(()) => {
                let _ = fs::remove_file(&probe);
            }
            Err(_) => return Err(VaultError::NotWritable(root.clone())),
        }

        Ok(())
    }

    // ----- Path utilities ---------------------------------------------------

    /// Join `relative` with the vault root (private helper).
    fn vault_path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Resolve a relative path against the vault root.
    pub fn resolve_path(&self, relative: &str) -> PathBuf {
        self.vault_path(relative)
    }

    /// Strip the vault root prefix from an absolute path.
    ///
    /// Returns `Some(relative_string)` when `absolute` is inside the vault,
    /// or `None` otherwise.
    pub fn relative_path(&self, absolute: &Path) -> Option<String> {
        absolute
            .strip_prefix(&self.root)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// Check whether `relative` resolves to an existing filesystem entry.
    pub fn path_exists(&self, relative: &str) -> bool {
        self.vault_path(relative).exists()
    }

    /// Check whether `relative` has a `.md` extension (case-insensitive).
    pub fn is_markdown(&self, relative: &str) -> bool {
        Path::new(relative)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    }

    // ----- Directory tree ---------------------------------------------------

    /// Build the full directory tree.
    ///
    /// Skips hidden entries (`.appname/`, `.vault-index/`, `.trash/`, `.git/`,
    /// etc.) and non-markdown files. Children are sorted: directories
    /// alphabetically first, then `.md` files alphabetically.
    pub fn build_tree(&self) -> io::Result<TreeEntry> {
        TreeEntry::build_from_dir(&self.root)
    }

    // ----- File CRUD --------------------------------------------------------

    /// Read the content of a note at `path`.
    pub fn read_note(&self, path: &str) -> io::Result<String> {
        fs::read_to_string(self.vault_path(path))
    }

    /// Create a new note at `path` with `content`.
    ///
    /// Parent directories are created automatically. Uses atomic write so
    /// the file is either fully written or not at all.
    pub fn create_note(&self, path: &str, content: &str) -> io::Result<()> {
        let full = self.vault_path(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write_str(&full, content)
    }

    /// Overwrite an existing note at `path` with `content`. Uses atomic write.
    pub fn update_note(&self, path: &str, content: &str) -> io::Result<()> {
        atomic_write_str(&self.vault_path(path), content)
    }

    /// Delete a note by moving it to `.trash/{timestamp}/{original_path}`.
    ///
    /// The timestamp is milliseconds since the Unix epoch.
    pub fn delete_note(&self, path: &str) -> io::Result<()> {
        let full = self.vault_path(path);
        if !full.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("note not found: {path}"),
            ));
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        let trash_path = self
            .root
            .join(".trash")
            .join(timestamp.to_string())
            .join(path);

        if let Some(parent) = trash_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::rename(&full, &trash_path)
    }

    /// Rename a note or folder from `old` to `new`.
    ///
    /// Intermediate directories in the destination are created automatically.
    pub fn rename_note(&self, old: &str, new: &str) -> io::Result<()> {
        let full_old = self.vault_path(old);
        let full_new = self.vault_path(new);

        if !full_old.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("source not found: {old}"),
            ));
        }

        if let Some(parent) = full_new.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::rename(full_old, full_new)
    }

    /// Create a folder (and any intermediate directories) at `path`.
    pub fn create_folder(&self, path: &str) -> io::Result<()> {
        fs::create_dir_all(self.vault_path(path))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_vault() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // ---- build_tree --------------------------------------------------------

    #[test]
    fn test_build_tree_nested_dirs() {
        let dir = setup_vault();
        let root = dir.path();

        // Structure:
        //   A/
        //     inner.md
        //     Sub/
        //       deep.md
        //   Z.md
        //   .hidden/        ← skipped
        //     secret.md
        //   .vault-index/   ← skipped
        //   readme.txt      ← not .md, skipped
        fs::create_dir_all(root.join("A/Sub")).unwrap();
        fs::write(root.join("A/inner.md"), "inner").unwrap();
        fs::write(root.join("A/Sub/deep.md"), "deep").unwrap();
        fs::write(root.join("Z.md"), "z").unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden/secret.md"), "secret").unwrap();
        fs::create_dir_all(root.join(".vault-index")).unwrap();
        fs::write(root.join("readme.txt"), "not markdown").unwrap();

        let vm = VaultManager::new(root);
        let tree = vm.build_tree().unwrap();

        // Root Dir: children = [Dir("A"), File("Z.md")]
        let TreeEntry::Dir { ref children, .. } = tree else {
            panic!("expected root Dir");
        };
        assert_eq!(children.len(), 2);

        // First child: Dir "A"
        let TreeEntry::Dir { name, ref children } = children[0] else {
            panic!("expected Dir A");
        };
        assert_eq!(name, "A");

        // A's children: [Dir("Sub"), File("inner.md")]
        assert_eq!(children.len(), 2);

        let TreeEntry::Dir { name, ref children } = children[0] else {
            panic!("expected Dir Sub");
        };
        assert_eq!(name, "Sub");

        // Sub's children: [File("deep.md")]
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], TreeEntry::File { name: "deep.md".into() });

        // Second child of root: File("Z.md")
        assert_eq!(
            children[1],
            TreeEntry::File { name: "inner.md".into() }
        );

        // Root second child: File("Z.md")
        assert_eq!(
            tree_children(&tree)[1],
            TreeEntry::File { name: "Z.md".into() }
        );
    }

    // ---- create + read -----------------------------------------------------

    #[test]
    fn test_create_and_read_note() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_note("hello.md", "# Hello\nWorld").unwrap();
        assert!(vm.path_exists("hello.md"));

        let content = vm.read_note("hello.md").unwrap();
        assert_eq!(content, "# Hello\nWorld");
    }

    #[test]
    fn test_create_note_auto_creates_parents() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_note("deep/folder/note.md", "nested content")
            .unwrap();
        assert!(dir.path().join("deep/folder/note.md").exists());
        assert_eq!(
            vm.read_note("deep/folder/note.md").unwrap(),
            "nested content"
        );
    }

    // ---- update (atomic) ---------------------------------------------------

    #[test]
    fn test_update_note_atomic() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_note("edit.md", "version 1").unwrap();
        vm.update_note("edit.md", "version 2").unwrap();

        let content = vm.read_note("edit.md").unwrap();
        assert_eq!(content, "version 2");
    }

    // ---- delete → .trash ---------------------------------------------------

    #[test]
    fn test_delete_note_moves_to_trash() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_note("gone.md", "goodbye").unwrap();
        assert!(vm.path_exists("gone.md"));

        vm.delete_note("gone.md").unwrap();
        assert!(!vm.path_exists("gone.md"));

        // .trash/ should exist with one timestamped subfolder.
        let trash_dir = dir.path().join(".trash");
        assert!(trash_dir.is_dir());

        let ts_dirs: Vec<_> = fs::read_dir(&trash_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        assert_eq!(ts_dirs.len(), 1);

        // Trashed file should live at .trash/{ts}/gone.md
        let trashed = ts_dirs[0].path().join("gone.md");
        assert!(trashed.exists());
        assert_eq!(fs::read_to_string(&trashed).unwrap(), "goodbye");
    }

    #[test]
    fn test_delete_nested_note_trash_structure() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_note("folder/sub/note.md", "nested").unwrap();
        vm.delete_note("folder/sub/note.md").unwrap();

        let trash_dir = dir.path().join(".trash");
        let ts_dir = fs::read_dir(&trash_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();

        // Original dir structure preserved inside trash
        assert!(ts_dir.join("folder/sub/note.md").exists());
    }

    // ---- create_folder -----------------------------------------------------

    #[test]
    fn test_create_folder() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_folder("new/nested/dir").unwrap();
        let full = vm.resolve_path("new/nested/dir");
        assert!(full.is_dir());
        assert!(vm.path_exists("new/nested/dir"));
    }

    // ---- validation --------------------------------------------------------

    #[test]
    fn test_validate_vault_ok() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());
        assert!(vm.validate_vault().is_ok());
    }

    #[test]
    fn test_validate_vault_invalid_root() {
        let vm = VaultManager::new("/nonexistent/vault/path");
        match vm.validate_vault() {
            Err(VaultError::InvalidRoot(p)) => {
                assert_eq!(p.to_str().unwrap(), "/nonexistent/vault/path");
            }
            other => panic!("expected InvalidRoot, got: {other:?}"),
        }
    }

    // ---- path utilities ----------------------------------------------------

    #[test]
    fn test_relative_path_inside_vault() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        let abs = vm.resolve_path("notes/daily.md");
        let rel = vm.relative_path(&abs).unwrap();
        assert_eq!(rel, "notes/daily.md");
    }

    #[test]
    fn test_relative_path_outside_vault() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        let outside = PathBuf::from("/tmp/other.md");
        assert!(vm.relative_path(&outside).is_none());
    }

    #[test]
    fn test_is_markdown() {
        let vm = VaultManager::new("/tmp");
        assert!(vm.is_markdown("note.md"));
        assert!(vm.is_markdown("path/to/NOTE.MD"));
        assert!(!vm.is_markdown("readme.txt"));
        assert!(!vm.is_markdown("noextension"));
        assert!(!vm.is_markdown("md"));
    }

    #[test]
    fn test_open_vault_validated() {
        let dir = setup_vault();
        let vm = VaultManager::open_vault(dir.path()).unwrap();
        assert!(vm.root().is_dir());
    }

    #[test]
    fn test_open_vault_invalid() {
        let result = VaultManager::open_vault("/no/such/dir");
        assert!(result.is_err());
    }

    // ---- rename ------------------------------------------------------------

    #[test]
    fn test_rename_note() {
        let dir = setup_vault();
        let vm = VaultManager::new(dir.path());

        vm.create_note("old.md", "content").unwrap();
        vm.rename_note("old.md", "new.md").unwrap();

        assert!(!vm.path_exists("old.md"));
        assert!(vm.path_exists("new.md"));
        assert_eq!(vm.read_note("new.md").unwrap(), "content");
    }

    // ---- helpers -----------------------------------------------------------

    /// Extract children from a root Dir tree entry for assertion convenience.
    fn tree_children(entry: &TreeEntry) -> &[TreeEntry] {
        match entry {
            TreeEntry::Dir { children, .. } => children,
            _ => panic!("expected Dir entry"),
        }
    }
}
