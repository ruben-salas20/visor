//! Area E: the settings, the list of shortcuts and the status bar. Frame
//! side: `marco/src/areas/settings/`; the settings and the shortcuts load when opened.

use crate::ui::protocol::sides::Kind;
use serde::{Deserialize, Serialize};

/// Nothing yet: settings and shortcuts come in phase 1.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {}

/// The status bar.
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
}
