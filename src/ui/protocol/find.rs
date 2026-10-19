//! Area D, the find bar (Ctrl+F) over the page of a side. Frame side:
//! `marco/src/areas/find/`, which draws the bar and sends what was typed.

use serde::{Deserialize, Serialize};

/// What the frame asks for.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    /// The text or a checkbox changed: search again from the first match.
    Search { text: String, match_case: bool, whole_word: bool },
    /// Shows the next match (`forward`) or the previous one.
    Step { forward: bool },
    /// Escape or the close button: hides the bar and gives the keys back to the page.
    Close,
}

/// The bar's whole state. `open` false hides it.
#[derive(Serialize, Debug, PartialEq, Clone, Default)]
pub struct State {
    pub open: bool,
    /// The side whose page the bar lies over.
    pub side: u64,
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
    /// The counter, already in the visor's language: "3 de 12", "Sin resultados".
    pub label: String,
    /// Whether the search has matches, which turns the step buttons on.
    pub found: bool,
    /// Goes up each time the bar takes the keyboard; the frame then focuses its field.
    pub edit: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_asks_for_a_search_a_step_or_a_close() {
        let search = r#"{"type":"search","text":"jacobiano","match_case":false,"whole_word":true}"#;
        assert_eq!(serde_json::from_str::<Msg>(search).unwrap(), Msg::Search { text: "jacobiano".into(), match_case: false, whole_word: true });
        assert_eq!(serde_json::from_str::<Msg>(r#"{"type":"step","forward":false}"#).unwrap(), Msg::Step { forward: false });
        assert_eq!(serde_json::from_str::<Msg>(r#"{"type":"close"}"#).unwrap(), Msg::Close);
        assert!(serde_json::from_str::<Msg>(r#"{"type":"step"}"#).is_err());
    }

    #[test]
    fn the_state_says_its_side_and_counter() {
        let state = State { open: true, side: 2, label: "3 de 12".into(), found: true, edit: 5, ..Default::default() };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["side"], 2);
        assert_eq!(json["label"], "3 de 12");
        assert_eq!(json["found"], true);
    }
}
