//! Owning the agent socket: only one instance of the app gets it, and it only
//! ever removes its own.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

/// How long to wait for an instance that is still starting up to open its socket.
const STARTUP_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum ClaimError {
    /// Another instance owns the socket.
    Running,
    Failed(String),
}

/// The right to the socket: the lock that keeps other instances out and the
/// identity of the socket file, to tell it from a later one.
pub struct Claim {
    path: PathBuf,
    id: (u64, u64),
    _lock: File,
}

/// Takes the socket at `path`. Instances that start together queue on a lock
/// file, so the second one finds the first one's socket instead of replacing it.
pub fn claim(path: &Path) -> Result<(UnixListener, Claim), ClaimError> {
    if UnixStream::connect(path).is_ok() {
        return Err(ClaimError::Running);
    }
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_name)
        .map_err(|e| ClaimError::Failed(format!("No se pudo crear {}: {e}", Path::new(&lock_name).display())))?;
    let deadline = Instant::now() + STARTUP_WAIT;
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => {
                if UnixStream::connect(path).is_ok() {
                    return Err(ClaimError::Running);
                }
                if Instant::now() >= deadline {
                    return Err(ClaimError::Failed("Otra instancia de visor está arrancando y no responde.".into()));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(TryLockError::Error(e)) => return Err(ClaimError::Failed(format!("No se pudo bloquear el socket: {e}"))),
        }
    }
    // Whoever held the lock is gone, so what is left at `path` is a dead socket,
    // unless an instance from before the lock existed is listening on it.
    if UnixStream::connect(path).is_ok() {
        return Err(ClaimError::Running);
    }
    let _ = fs::remove_file(path);
    let failed = |e: std::io::Error| ClaimError::Failed(format!("No se pudo crear {}: {e}", path.display()));
    let listener = UnixListener::bind(path).map_err(failed)?;
    let meta = fs::metadata(path).map_err(failed)?;
    Ok((listener, Claim { path: path.to_path_buf(), id: (meta.dev(), meta.ino()), _lock: lock }))
}

impl Claim {
    /// Removes the socket, unless it is no longer the one this claim bound.
    pub fn release(self) {
        let ours = fs::metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.id);
        if ours {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-socket-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_one_instance_gets_the_socket() {
        let dir = dir("one");
        let path = dir.join("v.sock");
        let (_listener, claim_a) = claim(&path).unwrap();
        assert!(matches!(claim(&path), Err(ClaimError::Running)));
        claim_a.release();
        assert!(!path.exists());
        assert!(claim(&path).is_ok(), "free again after release");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn instances_started_together_do_not_replace_each_other() {
        let dir = dir("race");
        let path = dir.join("v.sock");
        let tries: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                thread::spawn(move || claim(&path))
            })
            .collect();
        let results: Vec<_> = tries.into_iter().map(|t| t.join().unwrap()).collect();
        let winners = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(winners, 1);
        assert!(results.iter().all(|r| r.is_ok() || matches!(r, Err(ClaimError::Running))));
        assert!(UnixStream::connect(&path).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_dead_socket_is_replaced() {
        let dir = dir("stale");
        let path = dir.join("v.sock");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(claim(&path).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn release_leaves_a_socket_that_is_not_its_own() {
        let dir = dir("other");
        let path = dir.join("v.sock");
        let (_listener, mine) = claim(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let _other = UnixListener::bind(&path).unwrap();
        mine.release();
        assert!(UnixStream::connect(&path).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
}
