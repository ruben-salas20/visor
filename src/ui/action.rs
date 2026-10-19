//! What reaches the window's event loop: events from pages, the frame, the
//! agent server and the file watchers, and the actions the controls trigger.

use crate::session::AgentRequest;
use crate::ui::protocol::FrameMsg;
use serde::Deserialize;
use crate::ui::windows::WinId;
use std::path::PathBuf;
use std::sync::Arc;

/// What the person did in the panel. Paths are relative to the vault and
/// unchecked: Rust validates each one before acting. The frame sends these
/// with `"area": "files"` (see `ui/protocol.rs`).
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SidebarMsg {
    PickVault,
    /// Only a vault in the settings' list is opened.
    SwitchVault { path: PathBuf },
    RemoveVault { path: PathBuf },
    Open { path: String, tab: bool, beside: bool },
    CreateNote { dir: String, name: String },
    CreateFolder { dir: String, name: String },
    Trash { path: String },
    /// Opens a file visor cannot show with the program the system gives it.
    OpenElsewhere { path: String },
    Move { path: String, dir: String },
    /// The folders now open in the tree, to restore them next time.
    OpenFolders { folders: Vec<String> },
    /// Opens the field for the name of a new note or folder in `dir`; without
    /// `dir`, in the folder of the file on the focused side.
    AskName { dir: Option<String>, folder: bool },
    CancelName,
    /// A file is being dragged from the tree, maybe to a side (ADR 0014).
    DragStart { path: String },
    DragEnd,
    /// The person dragged the panel's edge.
    Width { width: i32 },
    ShowOther { on: bool },
}

/// An event and the window it is for.
pub struct Routed {
    pub window: WinId,
    pub event: UserEvent,
}

/// Hands an event to the loop. A closed loop drops it, as every caller did
/// with `let _ = send_event(..)`. A function, so this file does not depend on
/// the window library (tao on Linux, winit on Windows).
pub type Send = Arc<dyn Fn(Routed) + std::marker::Send + Sync>;

/// Sends events to the loop on behalf of one window.
#[derive(Clone)]
pub struct Proxy {
    window: WinId,
    inner: Send,
}

impl Proxy {
    pub fn new(window: WinId, inner: Send) -> Self {
        Proxy { window, inner }
    }

    pub fn window(&self) -> WinId {
        self.window
    }

    /// The same loop, on behalf of another window.
    pub fn to(&self, window: WinId) -> Proxy {
        Proxy { window, inner: self.inner.clone() }
    }

    pub fn send(&self, event: UserEvent) {
        (self.inner)(Routed { window: self.window, event });
    }
}

pub enum UserEvent {
    /// An order from the frame, already checked: see `protocol::accept_frame`.
    Frame(FrameMsg),
    /// A message from a page view, with the URL of the page that sent it.
    Page { view: u64, url: String, body: String },
    Navigated { view: u64, url: String },
    PageLoaded(u64),
    /// A page view got the keyboard focus.
    Focused(u64),
    /// An order from the agent socket server, for the window it names.
    Agent(AgentRequest),
    Picked(Option<PathBuf>),
    VaultChanged(Vec<PathBuf>),
    LooseChanged(Vec<PathBuf>),
    ThemeChanged,
    MonoFont(Option<String>),
    Fullscreen { view: u64, on: bool },
    /// A link the page asked to open in a new tab: in the background, for
    /// Ctrl+click and middle click, or shown, for target="_blank".
    OpenTab { view: u64, url: String, background: bool },
    /// A resource of a page view that did not load: a failed request, or one that
    /// answered with an error status. The main document is not reported here.
    ResourceFailed { view: u64, uri: String, reason: LoadFailure },
    /// The reading position a page reported as its tab was hidden.
    Position { view: u64, json: String },
    Action(Action),
    /// The settings write that was put off is due.
    SaveConfig,
    /// For the process: closes every window and remembers them all (Ctrl+Q).
    Quit,
    /// For the process: saves every window and runs the installed binary.
    /// `from` is the window that asked, which shows why if it fails.
    Restart { from: WinId },
    /// A manual update check ended. `installed` offers the restart button.
    UpdateDone { message: String, installed: bool },
    /// The pages answered whether a restart may go ahead.
    RestartChecked(RestartCheck),
}

/// What the pages said before a restart.
pub enum RestartCheck {
    Clear,
    /// A note has text not saved yet.
    Unsaved,
    /// Some page did not answer, so nothing is known about its notes.
    Unknown,
}

/// Why a resource did not load.
pub enum LoadFailure {
    Network,
    Status(u32),
}

pub enum Action {
    Quit,
    PickVault,
    ToggleSidebar,
    ToggleBar,
    /// Steps the page zoom in (1) or out (-1), or resets it (0).
    Zoom(i8),
    /// Ctrl+plus, Ctrl+minus, Ctrl+0: as `Zoom`, but a PDF in front takes it
    /// into its viewer's own zoom.
    ZoomKey(i8),
    Reload,
    Back,
    Forward,
    FocusAddress,
    // Phase 1, area C: typing in the address.
    #[allow(dead_code)]
    LeaveAddress,
    /// The address field lost focus; a hidden bar hides again.
    #[allow(dead_code)]
    AddressDone,
    NewNote,
    ToggleEdit,
    PickElement,
    ToggleAnnotations,
    #[allow(dead_code)]
    Go(String),
    NewTab,
    /// Closes the tab with this id, or the focused one.
    CloseTab(Option<u64>),
    /// Moves to the next (1) or previous (-1) tab of the focused side.
    CycleTab(i8),
    /// Shows the tab at this place (1 to 8) of the focused side, or the last one for 9.
    SelectNth(usize),
    SelectTab(u64),
    ToggleSplit,
    /// Moves the keyboard focus to the left (0) or right (1) side.
    FocusSide(usize),
    // Phase 1, area E: the settings popover sends these.
    /// The theme chosen in the settings: «claro», «oscuro», or `None` for the system's.
    #[allow(dead_code)]
    SetTheme(Option<String>),
    /// The language chosen in the settings: "es", "en", or `None` for the system's.
    #[allow(dead_code)]
    SetLanguage(Option<String>),
    #[allow(dead_code)]
    SetAutoUpdate(bool),
    #[allow(dead_code)]
    SetShowOtherFiles(bool),
    /// The engine for text in the address field that is not an address.
    #[allow(dead_code)]
    SetSearchEngine(crate::ui::address::Engine),
    /// Checks for a new version now and installs it if there is one.
    #[allow(dead_code)]
    CheckUpdates,
    /// Restarts visor on the installed version, if no note has unsaved text.
    #[allow(dead_code)]
    Restart,
    /// Shows the sidebar if hidden and opens its settings popover.
    Settings,
    /// Shows or hides the list of keyboard shortcuts.
    Shortcuts,
    /// Ctrl+F: the page's own search (PDF, note editor) or the native find bar.
    Find,
    /// Ctrl+F found no search of the page's own: opens the native find bar on the focused side.
    FindNative,
}
