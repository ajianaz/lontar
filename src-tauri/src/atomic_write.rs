use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Write `data` to `path` atomically.
///
/// Strategy: write to a unique temp file → fsync → rename.
/// On POSIX, `rename` is atomic so the original file is either fully replaced
/// or left untouched. On failure the temp file is cleaned up.
///
/// Unlike a fixed `.{}.tmp` path, each call generates a unique temp file
/// to prevent race conditions when multiple threads/processes write the
/// same destination concurrently.
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

    // Unique temp file per write attempt — avoids race conditions.
    let prefix = format!(".{}.tmp-", file_name.to_string_lossy());
    let mut tmp_file = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".tmp")
        .tempfile_in(parent)?;

    // Write data + fsync for durability. NamedTempFile auto-cleans on drop
    // if any of these fail (no manual remove_file needed).
    tmp_file.write_all(data)?;
    tmp_file.as_file().sync_all()?;

    // Atomic rename (POSIX). On failure, original file is intact and the
    // returned PersistError carries the NamedTempFile back so it can still
    // auto-clean.
    tmp_file.persist(path).map_err(|e| e.error)?;

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

    #[test]
    fn concurrent_writes_to_same_file() {
        use std::sync::Arc;
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join("race.txt"));
        let num_writers = 8;

        let handles: Vec<_> = (0..num_writers)
            .map(|i| {
                let path = Arc::clone(&path);
                thread::spawn(move || {
                    let content = format!("writer-{}", i);
                    atomic_write_str(&path, &content)
                })
            })
            .collect();

        // All writes should succeed (no race condition on temp file name).
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        // Final file should contain exactly one writer's content.
        let final_content = fs::read_to_string(&*path).unwrap();
        assert!(
            final_content.starts_with("writer-"),
            "expected 'writer-N' but got: {:?}",
            final_content
        );
        let writer_num: u8 = final_content
            .strip_prefix("writer-")
            .unwrap()
            .parse()
            .unwrap();
        assert!(writer_num < num_writers);
    }

    #[test]
    fn temp_file_has_unique_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.txt");

        // Two consecutive writes should produce different temp files.
        // We can't inspect the temp name directly, but we can verify
        // both writes succeed without collision.
        atomic_write_str(&path, "first").unwrap();
        atomic_write_str(&path, "second").unwrap();

        // Final content should be from the last successful write.
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "second");

        // No leftover temp files in the directory.
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["hello.txt"]);
    }
}
