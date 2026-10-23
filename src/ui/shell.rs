//! The window library and the event loop, which differ by system: tao over GTK
//! on Linux, winit on Windows (docs/adr/0040). Each half gives the same names:
//!
//! - `Loop`, `Target`, `Window`, `WindowId`: the loop, what windows are made
//!   with inside it, a window of visor and its id.
//! - `event_loop()` and `run()`: the loop, and handing it to a `Handler`.
//! - `create_window()`, `connect_shortcuts()`, `after()`, `web_context()`,
//!   `prepare()`: a window, the app-wide shortcuts, a delayed call, the one web
//!   context and what the system needs before the first window.
//!
//! Nothing here knows about tabs or vaults: that is `visor.rs` and `app.rs`.

use crate::ui::action::Routed;

/// What the loop reports of a window.
pub enum Signal {
    Close,
    Focused,
    Resized,
}

/// What the loop calls. Each method returns true when the process must end.
pub trait Handler {
    /// Once, before any window event, to open the first windows.
    fn start(&mut self, target: &Target);
    fn window(&mut self, id: WindowId, signal: Signal) -> bool;
    fn user(&mut self, target: &Target, routed: Routed) -> bool;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;
