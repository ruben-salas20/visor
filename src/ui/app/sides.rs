//! Area B, the sides and their tab rows. Frame side: `marco/src/areas/sides/`.
//! The model is `ui/tabs.rs`; this only turns the frame's clicks into its
//! operations and the model into the frame's snapshot. The divider is the
//! frame's; the drop zones over the pages and page fullscreen are the host's.

use super::App;
use crate::session::Place;
use crate::ui::action::Action;
use crate::ui::page_calls;
use crate::ui::protocol::sides::{Msg, Side, State, Tab, kind_of, split_ratio};

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
            Msg::Split { ratio } => {
                let Some(ratio) = split_ratio(ratio) else { return };
                self.split_ratio = Some(ratio);
                // Also the share a new window starts with.
                self.cfg_mut(|c| c.split_ratio = Some(ratio));
                self.save_config_soon();
                self.layout_changed = true;
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
        let ratio = self.split_ratio.or_else(|| self.cfg(|c| c.split_ratio)).and_then(split_ratio).unwrap_or(0.5);
        State { sides, focused: self.tabs.focused(), ratio }
    }

    /// A page asked for fullscreen, or left it. WebKit makes the window
    /// fullscreen; the pages of that side cover it, and the frame below.
    pub(super) fn fullscreen(&mut self, view: u64, on: bool) {
        let holder = on.then(|| self.tabs.locate(view)).flatten().map(|(p, _)| self.tabs.side(p).id());
        self.host.set_fullscreen(holder);
    }

    /// A file started being dragged from the panel: the pages take it.
    pub(super) fn drag_start(&mut self, path: String) {
        let split = self.tabs.sides().len() == 2;
        self.host.show_drop(true, split);
        self.dragging = Some(path);
    }

    pub(super) fn drag_end(&mut self) {
        self.host.show_drop(false, false);
        for p in 0..self.tabs.sides().len() {
            self.frost(p, None);
        }
        self.dragging = None;
    }

    pub(super) fn drop_hover(&self, side: u64, zone: Option<u8>) {
        if let Some(p) = self.side_index(side) {
            self.frost(p, zone);
        }
    }

    /// Marks on the page of side `p` where the file would land, or removes the mark.
    fn frost(&self, p: usize, zone: Option<u8>) {
        let Some(view) = self.tabs.side(p).tab().view().and_then(|id| self.view_of(id)) else { return };
        let theme = self.theme();
        let accent = theme.as_ref().map_or("#3584e4", |t| t.accent.as_str());
        let _ = view.evaluate_script(&page_calls::frost(zone, accent));
    }

    pub(super) fn dropped(&mut self, side: u64, half: Option<usize>) {
        let target = self.dragging.take();
        self.drag_end();
        let (Some(target), Some(p)) = (target, self.side_index(side)) else { return };
        let effects = self.tabs.drop_file(target, p, half);
        self.apply_and_publish(effects);
    }
}
