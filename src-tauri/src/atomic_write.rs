use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Write `data` to `path` atomically.
///
/// Strategy: write to `.{filename}.tmp` → fsync → rename.
/// On POSIX, `rename` is atomic so the original file is either fully replaced
/// or left untouched. On failure the `.tmp` file is cleaned up.
///
/// # Errors
///
/// Returns an [`io::Error`] if the parent directory doesn't exist, the temp
/// file can't be created/written/fsynced, or the rename fails.
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
    })?;

    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "path has no file name component",
        )
    })?;

    let tmp_path = parent.join(format!(".{}.tmp", file_name.to_string_lossy()));

    // Write data to temp file, fsync to ensure durability.
    let result = (|| {
        let mut file = File::create(&tmp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = result {
        // Best-effort cleanup of temp file on failure.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Atomic rename (POSIX). On failure, original file is intact.
    // Clean up tmp on rename failure.
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Fsync the parent directory to persist the directory entry (POSIX).
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }

    Ok(())
}

/// Convenience wrapper around [`atomic_write`] for string content.
///
/// Equivalent to `atomic_write(path, content.as_bytes())`.
pub fn atomic_write_str(path: &Path, content: &str) -> io::Result<()> {
    atomic_write(path, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn normal_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.txt");

        atomic_write_str(&path, "hello world").unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "hello world");
    }

    #[test]
    fn overwrite_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        fs::write(&path, "old content").unwrap();

        atomic_write_str(&path, "new content").unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "new content");
    }

    #[test]
    fn write_to_nonexistent_parent_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope/subdir/file.txt");

        let result = atomic_write_str(&path, "won't work");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
