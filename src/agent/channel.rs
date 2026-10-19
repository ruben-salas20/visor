//! The channel between the CLI and the app: a Unix socket on Linux, a named
//! pipe on Windows (W5 in docs/plan-multiplataforma.md). Only one instance
//! claims it; the others pass their orders through it.

#[cfg(target_os = "linux")]
pub use super::socket::{Claim, ClaimError, claim};
#[cfg(target_os = "linux")]
pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(windows)]
pub use super::pipe::{Claim, ClaimError, Listener, Stream, claim};

/// Connects to the app listening at `path`.
pub fn connect(path: &std::path::Path) -> std::io::Result<Stream> {
    Stream::connect(path)
}
