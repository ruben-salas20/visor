//! Area B, the sides and their tab rows. Frame side: `marco/src/areas/sides/`.
//! The model is `ui/tabs.rs`; this only turns the frame's clicks into its
//! operations and the model into the frame's snapshot. Still to come: the
//! divider, the drop zones and page fullscreen.

use super::App;
use crate::session::Place;
use crate::ui::action::Action;
use crate::ui::protocol::sides::{Msg, Side, State, Tab, kind_of};

impl App {
    pub(super) fn on_sides(&mut self, msg: Msg) {
        match msg {
            Msg::Select { tab } => self.act(Action::SelectTab(tab)),
            Msg::Close { tab } => self.act(Action::CloseTab(Some(tab))),
            Msg::New { side } => {
                if let Some(p) = self.side_index(side) {
                    self.focus_pane(p, false);
                    self.open_in(String::new(), Place::NewTab);
                }
            }
            Msg::Focus { side } => {
                if let Some(p) = self.side_index(side) {
                    self.focus_pane(p, true);
                }
            }
        }
    }

    /// The index of the side with this id.
    pub(super) fn side_index(&self, id: u64) -> Option<usize> {
        self.tabs.sides().iter().position(|s| s.id() == id)
    }

    pub(super) fn sides_state(&self) -> State {
        let sides = self
            .tabs
            .sides()
            .iter()
            .map(|side| Side {
                id: side.id(),
                active: side.active(),
                tabs: side.tabs().iter().map(|t| Tab { id: t.id(), label: t.label(), target: t.target().to_string(), kind: kind_of(t.target()) }).collect(),
            })
            .collect();
        State { sides, focused: self.tabs.focused() }
    }

    /// A page asked for fullscreen, or left it.
    pub(super) fn fullscreen(&mut self, view: u64, on: bool) {
        // Phase 1: the side's pages take the whole window, and the frame hides.
        let _ = (view, on);
    }
}
