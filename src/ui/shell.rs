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

use crate::ui::action::{Action, Routed};

/// A key as the app-wide shortcuts see it, whatever the system calls it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Key {
    /// A letter, lowercase, or `,` or `\`.
    Char(char),
    /// A digit, from the top row or the keypad.
    Digit(u8),
    /// `+` or `=`, from the main keys or the keypad.
    Plus,
    Minus,
    Tab,
    Left,
    Right,
    F5,
}

#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// The action of an app-wide shortcut, if `key` with `mods` is one. Caught
/// before the focused webview sees the key, so they work whichever side has focus.
///
/// `in_drawing` is set while a drawing is in front: Ctrl+K then goes to the
/// page, which uses it to add a link.
fn shortcut(key: Key, mods: Mods, in_drawing: bool) -> Option<Action> {
    let Mods { ctrl, alt, shift } = mods;
    let action = match key {
        Key::Char('a') if ctrl && shift => Action::PickElement,
        Key::Char('l') if ctrl && shift => Action::ToggleAnnotations,
        Key::Plus if ctrl => Action::ZoomKey(1),
        Key::Minus if ctrl => Action::ZoomKey(-1),
        // Ctrl+0 is the zoom, not a tab.
        Key::Digit(0) if ctrl => Action::ZoomKey(0),
        Key::Digit(n) if ctrl => Action::SelectNth(usize::from(n)),
        Key::Char(',') if ctrl => Action::Settings,
        Key::Char('k') if ctrl && !in_drawing => Action::Shortcuts,
        Key::Char('f') if ctrl && !shift && !in_drawing => Action::Find,
        Key::Tab if ctrl => Action::CycleTab(if shift { -1 } else { 1 }),
        Key::Left if ctrl && alt => Action::FocusSide(0),
        Key::Right if ctrl && alt => Action::FocusSide(1),
        Key::Char('\\') if ctrl => Action::ToggleSplit,
        Key::F5 => Action::Reload,
        Key::Char('r') if ctrl => Action::Reload,
        Key::Char('o') if ctrl => Action::PickVault,
        Key::Char('q') if ctrl => Action::Quit,
        Key::Char('b') if ctrl => Action::ToggleSidebar,
        Key::Char('l') if ctrl => Action::FocusAddress,
        Key::Char('n') if ctrl => Action::NewNote,
        Key::Char('e') if ctrl => Action::ToggleEdit,
        Key::Char('t') if ctrl => Action::NewTab,
        Key::Char('w') if ctrl => Action::CloseTab(None),
        Key::Left if alt => Action::Back,
        Key::Right if alt => Action::Forward,
        _ => return None,
    };
    Some(action)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods { ctrl: true, alt: false, shift: false };
    const CTRL_SHIFT: Mods = Mods { ctrl: true, alt: false, shift: true };
    const CTRL_ALT: Mods = Mods { ctrl: true, alt: true, shift: false };
    const ALT: Mods = Mods { ctrl: false, alt: true, shift: false };
    const NONE: Mods = Mods { ctrl: false, alt: false, shift: false };

    #[test]
    fn ctrl_0_is_the_zoom_and_the_other_digits_pick_a_tab() {
        assert!(matches!(shortcut(Key::Digit(0), CTRL, false), Some(Action::ZoomKey(0))));
        assert!(matches!(shortcut(Key::Digit(3), CTRL, false), Some(Action::SelectNth(3))));
        assert!(shortcut(Key::Digit(3), NONE, false).is_none(), "a digit alone is typing");
    }

    #[test]
    fn shift_tells_some_shortcuts_apart() {
        assert!(matches!(shortcut(Key::Char('l'), CTRL_SHIFT, false), Some(Action::ToggleAnnotations)));
        assert!(matches!(shortcut(Key::Char('l'), CTRL, false), Some(Action::FocusAddress)));
        assert!(matches!(shortcut(Key::Tab, CTRL_SHIFT, false), Some(Action::CycleTab(-1))));
        assert!(matches!(shortcut(Key::Tab, CTRL, false), Some(Action::CycleTab(1))));
        assert!(shortcut(Key::Char('f'), CTRL_SHIFT, false).is_none());
        assert!(shortcut(Key::Char('a'), CTRL, false).is_none(), "Ctrl+A selects text");
    }

    #[test]
    fn a_drawing_keeps_ctrl_k_and_ctrl_f() {
        assert!(shortcut(Key::Char('k'), CTRL, true).is_none());
        assert!(shortcut(Key::Char('f'), CTRL, true).is_none());
        assert!(matches!(shortcut(Key::Char('k'), CTRL, false), Some(Action::Shortcuts)));
    }

    #[test]
    fn the_arrows_go_back_or_to_a_side() {
        assert!(matches!(shortcut(Key::Left, ALT, false), Some(Action::Back)));
        assert!(matches!(shortcut(Key::Right, CTRL_ALT, false), Some(Action::FocusSide(1))));
        assert!(shortcut(Key::Left, NONE, false).is_none());
        assert!(matches!(shortcut(Key::F5, NONE, false), Some(Action::Reload)));
    }
}
