//! The window's views: the frame over the whole window and each side's page
//! views in the hole the frame announces for it. GTK on Linux, WebView2 child
//! windows on Windows; both give a `Host` with the same methods.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;
