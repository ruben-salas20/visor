//! Area E: the settings, the list of shortcuts, updates and the status bar.
//! Frame side: `marco/src/areas/settings/`; the settings and the shortcuts load when opened.

use crate::ui::protocol::sides::Kind;
use serde::{Deserialize, Serialize};

/// What the frame asks for: the two panels and the choices in them.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    /// The frame opened a panel (its button, or a click on the status bar).
    Open { panel: Panel },
    /// The frame closed a panel (Escape, a click outside, its button): the keys go back to the page.
    Closed { panel: Panel },
    /// `None` is the system's theme.
    SetTheme { choice: Option<String> },
    /// `None` is the system's language.
    SetLanguage { choice: Option<String> },
    /// -1 zooms out, 0 resets, 1 zooms in.
    Zoom { step: i8 },
    SetSearchEngine { id: String },
    SetAutoUpdate { on: bool },
    SetShowOther { on: bool },
    CheckUpdates,
    Restart,
}

/// The two panels the frame draws over the pages.
#[derive(Deserialize, Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Settings,
    Shortcuts,
}

/// The status bar, plus the settings and shortcuts panels: whether each one shows.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Status {
    pub vault: Option<String>,
    /// What the focused side shows.
    pub kind: Kind,
    /// The pages' zoom, 1.0 for 100 %.
    pub zoom: f64,
    pub version: &'static str,
    /// The last result of an update check or a restart, if any.
    pub update: Option<String>,
    /// An update check is running: its button waits.
    pub update_busy: bool,
    /// A new version is on disk: the settings offer a restart.
    pub update_restart: bool,
    /// The language chosen differs from the one in use: it applies on restart.
    pub language_pending: bool,
    pub prefs: Prefs,
    pub settings_open: bool,
    pub shortcuts_open: bool,
}

/// The choices of the settings panel, as they are saved.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Prefs {
    /// `None` is the system's theme.
    pub theme: Option<String>,
    /// `None` is the system's language.
    pub language: Option<String>,
    /// The id of the search engine in use.
    pub search_engine: &'static str,
    /// The engines, in the order the panel lists them.
    pub engines: Vec<Choice>,
    pub auto_update: bool,
    pub show_other: bool,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_panel_names_the_panel() {
        let msg: Msg = serde_json::from_str(r#"{"type":"closed","panel":"shortcuts"}"#).unwrap();
        assert_eq!(msg, Msg::Closed { panel: Panel::Shortcuts });
    }

    #[test]
    fn the_system_choice_is_null() {
        let msg: Msg = serde_json::from_str(r#"{"type":"set_theme","choice":null}"#).unwrap();
        assert_eq!(msg, Msg::SetTheme { choice: None });
    }
}
