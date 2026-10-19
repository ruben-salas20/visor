//! Area C, the address row of each side: back, forward, reload, the path of
//! the page and the annotation buttons. Frame side: `marco/src/areas/bar/`.
//! Still to come: typing an address (`go`, Ctrl+L) and its error state.

use super::App;
use crate::session::Place;
use crate::ui::action::Action;
use crate::ui::address::{self, Engine, Plan};
use crate::ui::protocol::bar::{Address, Msg, State};
use crate::vault;
use std::path::PathBuf;
use webkit2gtk::WebViewExt;
use wry::WebViewExtUnix;

impl App {
    pub(super) fn on_bar(&mut self, msg: Msg) {
        let (side, action) = match msg {
            Msg::Back { side } => (side, Action::Back),
            Msg::Forward { side } => (side, Action::Forward),
            Msg::Reload { side } => (side, Action::Reload),
            Msg::PickElement { side } => (side, Action::PickElement),
            Msg::ToggleAnnotations { side } => (side, Action::ToggleAnnotations),
        };
        let Some(p) = self.side_index(side) else { return };
        self.focus_pane(p, false);
        self.act(action);
    }

    pub(super) fn bar_state(&self) -> State {
        let sides = self
            .tabs
            .sides()
            .iter()
            .map(|side| {
                let target = side.tab().target();
                let view = side.tab().view().and_then(|id| self.view_of(id)).map(|v| v.webview());
                Address {
                    side: side.id(),
                    target: target.to_string(),
                    back: view.as_ref().is_some_and(WebViewExt::can_go_back),
                    forward: view.as_ref().is_some_and(WebViewExt::can_go_forward),
                    annotations: !target.is_empty() && !vault::is_web(target) && !vault::is_loose(target),
                }
            })
            .collect();
        State { sides }
    }

    /// Ctrl+L and Ctrl+T: the cursor goes to the focused side's address.
    pub(super) fn focus_address(&mut self) {
        // Phase 1.
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

    /// What was typed in the address leads nowhere.
    fn address_error(&mut self) {
        // Phase 1 marks the field.
    }
}
