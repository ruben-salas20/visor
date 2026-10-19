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
    /// The text typed in the address field, sent with Enter.
    Go { side: u64, text: String },
    /// Escape in the address field: the keys go back to the page.
    Leave { side: u64 },
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
    /// Goes up each time Rust asks for this address (Ctrl+L): the frame opens the field.
    pub edit: u64,
    /// Goes up each time a typed address led nowhere: the frame marks the field.
    pub refused: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_carries_the_typed_text() {
        let msg: Msg = serde_json::from_str(r#"{"type":"go","side":3,"text":"ondas.md"}"#).unwrap();
        assert_eq!(msg, Msg::Go { side: 3, text: "ondas.md".into() });
    }

    #[test]
    fn the_marks_reach_the_frame() {
        let state = State {
            sides: vec![Address { side: 1, target: String::new(), back: false, forward: false, annotations: false, edit: 2, refused: 1 }],
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["sides"][0]["edit"], 2);
        assert_eq!(json["sides"][0]["refused"], 1);
    }
}
