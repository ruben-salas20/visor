//! What each operating system does its own way. Linux is the one in use;
//! the Windows side is filled in by the port (docs/plan-multiplataforma.md).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
