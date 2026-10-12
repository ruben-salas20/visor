//! What reaches the window's event loop: events from pages, the panel, the
//! agent server and the file watchers, and the actions the controls trigger.

use crate::session::AgentRequest;
use std::path::PathBuf;

/// What the person did in the panel. Paths are relative to the vault and
/// unchecked: Rust validates each one before acting.
pub enum SidebarMsg {
    PickVault,
    SwitchVault { path: PathBuf },
    RemoveVault { path: PathBuf },
    Open { path: String, tab: bool, beside: bool },
    CreateNote { dir: String, name: String },
    CreateFolder { dir: String, name: String },
    Trash { path: String },
    Move { path: String, dir: String },
    /// A file started being dragged; the pages can take it while it lasts.
    DragStart { path: String },
    DragEnd,
    /// The folders now open in the tree, to restore them next time.
    OpenFolders(Vec<String>),
}

pub enum UserEvent {
    Sidebar(SidebarMsg),
    /// A message from a page view, with the URL of the page that sent it.
    Page { view: u64, url: String, body: String },
    Navigated { view: u64, url: String },
    PageLoaded(u64),
    /// A page view got the keyboard focus.
    Focused(u64),
    /// An order from the agent socket server.
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
    /// A file dragged from the sidebar was dropped on a side: on one half of
    /// it (0 left, 1 right) when the window is not split, or on all of it.
    Dropped { pane: u64, half: Option<usize> },
    /// The zone of a side under a dragged file changed: 0 left half, 1 right
    /// half, 2 the whole side, or none when the pointer left it.
    DropHover { pane: u64, zone: Option<u8> },
    Action(Action),
    /// The settings write that was put off is due.
    SaveConfig,
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
    PickVault,
    ToggleSidebar,
    ToggleBar,
    /// Steps the page zoom in (1) or out (-1), or resets it (0).
    Zoom(i8),
    Reload,
    Back,
    Forward,
    FocusAddress,
    LeaveAddress,
    /// The address field lost focus; a hidden bar hides again.
    AddressDone,
    NewNote,
    ToggleEdit,
    PickElement,
    ToggleAnnotations,
    Go(String),
    NewTab,
    /// Closes the tab with this id, or the focused one.
    CloseTab(Option<u64>),
    /// Moves to the next (1) or previous (-1) tab of the focused side.
    CycleTab(i8),
    SelectTab(u64),
    ToggleSplit,
    /// Makes the pane with this id the one keys and agents act on.
    FocusPane(u64),
    /// Moves the keyboard focus to the left (0) or right (1) side.
    FocusSide(usize),
    /// The theme chosen in the settings: «claro», «oscuro», or `None` for the system's.
    SetTheme(Option<String>),
    SetAutoUpdate(bool),
    SetShowOtherFiles(bool),
    /// Checks for a new version now and installs it if there is one.
    CheckUpdates,
    /// Restarts visor on the installed version, if no note has unsaved text.
    Restart,
    /// Shows the sidebar if hidden and opens its settings popover.
    Settings,
}
