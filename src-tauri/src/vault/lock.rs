//! File-based lock for exclusive vault access.
//!
//! Creates a `.vault-lock` JSON file in the vault root. If the lock exists
//! and the owning process is dead, the lock is stolen. Otherwise acquisition
//! fails with [`LockError::AlreadyLocked`].
//!
//! Lock acquisition uses `OpenOptions::create_new(true)` for an atomic
//! check-and-create, eliminating TOCTOU races between stale-lock removal
//! and new-lock creation.

#[cfg(unix)]
extern crate libc;

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Error returned when vault lock acquisition fails.
#[derive(Debug)]
pub enum LockError {
    /// Another live process holds the lock.
    AlreadyLocked { pid: u32, host: String },
    /// An I/O error occurred.
    Io(io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyLocked { pid, host } => {
                write!(f, "vault locked by pid {pid} on {host}")
            }
            Self::Io(e) => write!(f, "lock I/O error: {e}"),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::AlreadyLocked { .. } => None,
        }
    }
}

impl From<io::Error> for LockError {
    fn from(e: io::Error) -> Self {
        LockError::Io(e)
    }
}

impl From<serde_json::Error> for LockError {
    fn from(e: serde_json::Error) -> Self {
        LockError::Io(io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// JSON payload stored in the lock file.
#[derive(Serialize, Deserialize)]
struct LockInfo {
    pid: u32,
    host: String,
    timestamp: u64,
}

/// Exclusive lock on a vault directory.
///
/// Acquired via [`VaultLock::acquire`]. Automatically released on drop.
#[derive(Debug)]
pub struct VaultLock {
    path: PathBuf,
    held: bool,
}

impl VaultLock {
    /// Path to the lock file for a given vault root.
    fn lock_path(vault_path: &Path) -> PathBuf {
        vault_path.join(".vault-lock")
    }

    /// Try to acquire the vault lock.
    ///
    /// If a stale lock (dead PID) is found, it is stolen automatically.
    /// Uses `OpenOptions::create_new(true)` to atomically create the lock file,
    /// preventing TOCTOU races where two processes could both overwrite each
    /// other's lock.
    ///
    /// # Errors
    ///
    /// Returns [`LockError::AlreadyLocked`] if another live process holds the lock,
    /// or [`LockError::Io`] on filesystem errors.
    pub fn acquire(vault_path: &Path) -> Result<Self, LockError> {
        let lock_path = Self::lock_path(vault_path);

        let info = LockInfo {
            pid: std::process::id(),
            host: hostname(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        let json = serde_json::to_string_pretty(&info)?;

        for attempt in 0..3u32 {
            if lock_path.exists() {
                let existing = std::fs::read_to_string(&lock_path)?;

                if let Ok(info) = serde_json::from_str::<LockInfo>(&existing) {
                    if pid_is_alive(info.pid) {
                        return Err(LockError::AlreadyLocked {
                            pid: info.pid,
                            host: info.host,
                        });
                    }
                    // Stale lock — remove and retry.
                    std::fs::remove_file(&lock_path)?;
                } else {
                    // Corrupt lock file — remove it.
                    std::fs::remove_file(&lock_path)?;
                }
            }

            // Atomic create — fails if another process raced us.
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(json.as_bytes())?;
                    file.sync_all()?;
                    return Ok(Self {
                        path: lock_path,
                        held: true,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 2 => {
                    // Raced — retry the whole check-and-create sequence.
                    continue;
                }
                Err(e) => return Err(LockError::Io(e)),
            }
        }

        Err(LockError::AlreadyLocked {
            pid: 0,
            host: "unknown".into(),
        })
    }

    /// Release the lock by deleting the lock file.
    ///
    /// Safe to call multiple times — subsequent calls are no-ops.
    pub fn release(&mut self) -> io::Result<()> {
        if self.held {
            self.held = false;
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// Whether the lock is currently held.
    pub fn is_held(&self) -> bool {
        self.held
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        if self.held {
            if let Err(e) = self.release() {
                eprintln!("vault lock: failed to release on drop: {e}");
            }
        }
    }
}

/// Check whether a process with the given PID is alive.
#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    // kill(pid, 0) returns 0 if process exists, error otherwise.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    use std::os::windows::io::RawHandle;
    unsafe {
        let handle = windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        );
        if handle == 0 {
            return false;
        }
        windows_sys::Win32::Foundation::CloseHandle(handle);
        true
    }
}

/// Get a hostname string. Falls back to `"unknown"`.
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire() {
        let dir = tempfile::tempdir().unwrap();
        let lock = VaultLock::acquire(dir.path()).unwrap();
        assert!(lock.is_held());

        let lock_path = dir.path().join(".vault-lock");
        assert!(lock_path.exists());

        let info: LockInfo =
            serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(info.pid, std::process::id());
    }

    #[test]
    fn test_double_acquire_errors() {
        let dir = tempfile::tempdir().unwrap();

        let _lock1 = VaultLock::acquire(dir.path()).unwrap();

        let err = VaultLock::acquire(dir.path()).unwrap_err();
        match err {
            LockError::AlreadyLocked { pid, .. } => {
                assert_eq!(pid, std::process::id());
            }
            other => panic!("expected AlreadyLocked, got: {other}"),
        }
    }

    #[test]
    fn test_release() {
        let dir = tempfile::tempdir().unwrap();

        let mut lock = VaultLock::acquire(dir.path()).unwrap();
        assert!(lock.is_held());

        lock.release().unwrap();
        assert!(!lock.is_held());
        assert!(!dir.path().join(".vault-lock").exists());

        // Double-release is a no-op.
        lock.release().unwrap();
    }
}
