//! Area D, the find bar (Ctrl+F) for pages without a search of their own.
//! Frame side: `marco/src/areas/find/`, loaded when it opens.

use serde::Deserialize;

/// Nothing yet: the find bar comes in phase 1.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {}
