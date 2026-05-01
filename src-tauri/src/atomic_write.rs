use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Write `data` to `path` atomically by writing to a temp file then renaming.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir"))?;

    let tmp_name = format!(
        ".{}.tmp",
        path.file_name()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?
            .to_string_lossy()
    );
    let tmp_path = parent.join(&tmp_name);

    let mut file = File::create(&tmp_path)?;
    file.write_all(data)?;
    file.sync_all()?;

    std::fs::rename(&tmp_path, path)?;

    Ok(())
}
