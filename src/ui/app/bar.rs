//! Area C, the address row of each side: back, forward, reload, the path of
//! the page, the address field and the annotation buttons. Frame side:
//! `marco/src/areas/bar/`, which draws the field and sends what was typed.

use super::App;
use crate::session::Place;
use crate::ui::action::Action;
use crate::ui::address::{self, Engine, Plan};
use crate::ui::protocol::bar::{Address, Msg, State};
use crate::vault;
use std::path::{Path, PathBuf};

/// Counts the two things the frame must react to, per side. Each count only goes up.
#[derive(Default, Clone, Copy)]
pub(super) struct Marks {
    edit: u64,
    refused: u64,
}

impl App {
    pub(super) fn on_bar(&mut self, msg: Msg) {
        let (side, action) = match msg {
            Msg::Back { side } => (side, Action::Back),
            Msg::Forward { side } => (side, Action::Forward),
            Msg::Reload { side } => (side, Action::Reload),
            Msg::PickElement { side } => (side, Action::PickElement),
            Msg::ToggleAnnotations { side } => (side, Action::ToggleAnnotations),
            Msg::Go { side, text } => (side, Action::Go(text)),
            Msg::Leave { side } => {
                if let Some(p) = self.side_index(side) {
                    self.focus_pane(p, true);
                }
                return;
            }
        };
        let Some(p) = self.side_index(side) else { return };
        self.focus_pane(p, false);
        let go = matches!(action, Action::Go(_));
        let refused = self.address.get(&side).map_or(0, |m| m.refused);
        self.act(action);
        // Like a browser, the keys go back to the page once it is on its way;
        // a refused address keeps them in the field, which stays open.
        if go && self.address.get(&side).map_or(0, |m| m.refused) == refused {
            self.focus_pane(p, true);
        }
    }

    pub(super) fn bar_state(&self) -> State {
        let sides = self
            .tabs
            .sides()
            .iter()
            .map(|side| {
                let target = side.tab().target();
                let view = side.tab().view().and_then(|id| self.view_of(id));
                let marks = self.address.get(&side.id()).copied().unwrap_or_default();
                let (annotations, pick) = annotation_buttons(target);
                Address {
                    side: side.id(),
                    target: target.to_string(),
                    back: view.is_some_and(|v| v.can_go_back().unwrap_or(false)),
                    forward: view.is_some_and(|v| v.can_go_forward().unwrap_or(false)),
                    annotations,
                    pick,
                    edit: marks.edit,
                    refused: marks.refused,
                }
            })
            .collect();
        State { sides }
    }

    /// Ctrl+L and Ctrl+T: the keys go to the frame, and the focused side's address field opens.
    pub(super) fn focus_address(&mut self) {
        let side = self.tabs.focused_side().id();
        self.address.entry(side).or_default().edit += 1;
        self.host.focus_frame();
    }

    /// Resolves what was typed in the address field: a URL, a vault path, an
    /// absolute path, as the field shows for loose pages, a bare domain, or
    /// text to search for. The rules are in `address::plan`.
    pub(super) fn go(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let in_vault = self.vault().and_then(|v| Some(v.rel(&v.resolve(text)?)));
        match address::plan(text, in_vault.as_deref()) {
            Plan::Loose => {
                if self.open(PathBuf::from(text), text, Place::Here, false).is_err() {
                    self.address_error();
                }
            }
            Plan::Web => {
                self.open_in(text.to_string(), Place::Here);
            }
            Plan::Vault(rel) => {
                self.open_in(rel.to_string(), Place::Here);
            }
            Plan::Domain => {
                self.open_in(format!("https://{text}"), Place::Here);
            }
            Plan::Search(query) => {
                let engine = self.cfg(|c| Engine::from_id(c.search_engine.as_deref()));
                self.open_in(address::search_url(engine, query), Place::Here);
            }
            Plan::Error => self.address_error(),
        }
    }

    /// What was typed in the address leads nowhere: the frame marks the focused side's field.
    fn address_error(&mut self) {
        let side = self.tabs.focused_side().id();
        self.address.entry(side).or_default().refused += 1;
    }
}

/// Which annotation buttons a target shows: the list on vault pages, and
/// picking an element too, except on a PDF, which has text only.
fn annotation_buttons(target: &str) -> (bool, bool) {
    let annotations = !target.is_empty() && !vault::is_web(target) && !vault::is_loose(target);
    (annotations, annotations && !vault::is_pdf(Path::new(target)))
}

#[cfg(test)]
mod annotation_tests {
    use super::annotation_buttons;

    #[test]
    fn a_pdf_has_the_list_but_no_picking() {
        assert_eq!(annotation_buttons("tema/apuntes.pdf"), (true, false));
        assert_eq!(annotation_buttons("tema/ondas.md"), (true, true));
        assert_eq!(annotation_buttons("https://example.org/a.pdf"), (false, false));
        assert_eq!(annotation_buttons(""), (false, false));
    }
}
