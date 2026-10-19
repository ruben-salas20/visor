//! Area C, the address row under each side's tabs. Frame side: `marco/src/areas/bar/`.

use serde::{Deserialize, Serialize};

/// What the address row asks for, on its side.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    Back { side: u64 },
    Forward { side: u64 },
    Reload { side: u64 },
    PickElement { side: u64 },
    ToggleAnnotations { side: u64 },
}

/// One address row per side, in the order of the sides.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct State {
    pub sides: Vec<Address>,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Address {
    pub side: u64,
    /// The active tab's target: vault path, absolute path, web address or empty.
    pub target: String,
    pub back: bool,
    pub forward: bool,
    /// Whether the annotation buttons apply: only vault pages have annotations.
    pub annotations: bool,
}
