//! The model of the window: its sides, their tabs, which side has the focus
//! and which tabs are live. It knows no GTK and no webview; a live tab only
//! carries the id of its view.
//!
//! Every operation states an intent (open, close, split…) and returns the
//! [`Effect`]s the window must apply, in order, to the widgets and views. The
//! fields are private, so the invariants hold between operations: there is at
//! least one side, no side is empty, `active` is in range, a view id is held
//! by one tab only, and the visible tabs of the sides are always live.

use crate::session::Place;
use crate::vault::is_web;
use crate::vault::config::{PaneState, Workspace};
use serde_json::{Value, json};

/// Pages kept loaded across all tabs. Each one is a WebKit process of
/// 60–90 MB; the active tab of each side always counts. See docs/adr/0011.
pub const MAX_LIVE: usize = 3;

/// What the window has to do after an operation. Side numbers are indexes at
/// the moment the effect is applied, so applying them in order is enough.
#[derive(Debug, PartialEq)]
pub enum Effect {
    /// A new, empty right side with this id.
    AddSide { id: u64 },
    /// Drops side `side`; the one left moves to the left.
    RemoveSide { side: usize },
    // Phase 1, area B: a file dropped on the left half.
    #[allow(dead_code)]
    SwapSides,
    /// Builds the view of a tab in `side`, hidden, and loads `target`.
    Create { side: usize, view: u64, target: String, fragment: String },
    /// Drops the view and forgets its page.
    Unload { view: u64 },
    /// Moves the view to another side's container without reloading it.
    MoveView { view: u64, from: usize, to: usize },
    /// Loads `target` in a view that is already live.
    Navigate { view: u64, target: String, fragment: String },
    /// The view may be about to hide: ask where the reader is, if it is on screen.
    SavePosition { view: u64 },
    /// Shows the active view of `side` and hides the rest, and updates its bar.
    Show { side: usize },
    /// Gives the keyboard to the view of the focused side.
    Focus,
}

/// A document open in a side. Only a few tabs have a view, because each one
/// costs a WebKit process; the rest load `target` again when shown.
pub struct Tab {
    id: u64,
    /// Vault-relative path, absolute path of a loose page, web address, or
    /// empty for the start page.
    target: String,
    /// The page title, for websites once they load.
    title: Option<String>,
    /// The live view. Its id tags the view's messages.
    view: Option<u64>,
    /// Whether the view has started showing its page. Until then it stays hidden.
    ready: bool,
    /// When the tab was last shown, to unload the oldest first.
    used: u64,
    /// Reading position, as `{y}` or `{line, y}`, taken when the tab was last hidden.
    position: Option<String>,
    /// Position to put back once the reloaded page finishes loading.
    restore: Option<String>,
    /// Section to scroll to on the first load, from a link with `#`.
    fragment: String,
}

impl Tab {
    fn new(id: u64, target: String) -> Tab {
        Tab { id, target, title: None, view: None, ready: false, used: 0, position: None, restore: None, fragment: String::new() }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn view(&self) -> Option<u64> {
        self.view
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn label(&self) -> String {
        if let Some(title) = self.title.as_ref().filter(|t| !t.trim().is_empty()) {
            return title.clone();
        }
        if self.target.is_empty() {
            return crate::i18n::tr("Nueva pestaña", "New tab").into();
        }
        if let Some(rest) = self.target.split_once("://").map(|(_, r)| r) {
            return rest.split('/').next().unwrap_or(rest).to_string();
        }
        self.target.rsplit('/').next().unwrap_or(&self.target).to_string()
    }

    /// Stops being live, without a trace of the view.
    fn unload(&mut self, effects: &mut Vec<Effect>) {
        self.ready = false;
        if let Some(view) = self.view.take() {
            effects.push(Effect::Unload { view });
        }
    }
}

/// One side of the window.
pub struct Side {
    id: u64,
    tabs: Vec<Tab>,
    active: usize,
    /// Its tab strip no longer matches the model.
    dirty: bool,
}

impl Side {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active(&self) -> usize {
        self.active
    }

    /// The visible tab. A side is never empty.
    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }
}

pub struct Tabs {
    sides: Vec<Side>,
    /// Index of the side keys and agents act on.
    focused: usize,
    /// Counts tab switches, to find the tab used longest ago.
    clock: u64,
    /// Source of ids for sides, tabs and views.
    next_id: u64,
}

/// What `navigated` found out about the tab.
pub struct Navigated {
    pub side: usize,
    pub active: bool,
    /// The view has just started showing its page, so it can be shown.
    pub revealed: bool,
}

/// What `loaded` found out about the tab.
pub struct Loaded {
    #[allow(dead_code)]
    pub side: usize,
    /// Reading position to put back in the reloaded page.
    pub restore: Option<String>,
    /// A website: its title is read from the view.
    pub web: bool,
}

impl Default for Tabs {
    fn default() -> Self {
        Self::new()
    }
}

impl Tabs {
    /// One side with one empty tab, not live yet: `reset` is the first call.
    pub fn new() -> Tabs {
        Tabs::new_from(0)
    }

    /// As `new`, but ids are handed out above `first_id`. Each window starts
    /// its ids in its own range, so view ids stay unique across windows.
    pub fn new_from(first_id: u64) -> Tabs {
        let mut tabs = Tabs { sides: vec![], focused: 0, clock: 0, next_id: first_id };
        let mut effects = Vec::new();
        let side = tabs.add_side(&mut effects);
        tabs.push_empty(side);
        tabs
    }

    // Queries.

    pub fn sides(&self) -> &[Side] {
        &self.sides
    }

    pub fn side(&self, p: usize) -> &Side {
        &self.sides[p]
    }

    pub fn focused(&self) -> usize {
        self.focused
    }

    pub fn focused_side(&self) -> &Side {
        &self.sides[self.focused]
    }

    /// The view of the focused side's active tab. It is always live.
    pub fn focused_view(&self) -> Option<u64> {
        self.focused_side().tab().view
    }

    /// The side holding `view` and whether it is that side's active tab.
    pub fn locate(&self, view: u64) -> Option<(usize, bool)> {
        self.sides.iter().enumerate().find_map(|(p, side)| {
            side.tabs.iter().position(|t| t.view == Some(view)).map(|i| (p, side.active == i))
        })
    }

    /// Every tab's target, left side first.
    pub fn targets(&self) -> impl Iterator<Item = &str> {
        self.sides.iter().flat_map(|s| &s.tabs).map(|t| t.target.as_str())
    }

    /// The live tabs as (view, target).
    pub fn live(&self) -> impl Iterator<Item = (u64, &str)> {
        self.sides.iter().flat_map(|s| &s.tabs).filter_map(|t| Some((t.view?, t.target.as_str())))
    }

    /// Tabs, split and focus, as saved per vault.
    pub fn workspace(&self) -> Workspace {
        Workspace {
            panes: self
                .sides
                .iter()
                .map(|s| PaneState { tabs: s.tabs.iter().map(|t| t.target.clone()).collect(), active: s.active })
                .collect(),
            focused: self.focused,
        }
    }

    /// The sides as `visor status` reports them.
    pub fn layout(&self) -> Value {
        let sides: Vec<Value> = self
            .sides
            .iter()
            .map(|s| {
                let tabs: Vec<&str> = s.tabs.iter().map(|t| t.target.as_str()).collect();
                json!({ "tabs": tabs, "active": s.active })
            })
            .collect();
        json!({ "list": sides, "focused": self.focused })
    }

    /// Whether the strip of side `p` has to be drawn again, and marks it clean.
    /// The frame compares whole snapshots instead; phase 1 may draw less with it.
    #[allow(dead_code)]
    pub fn take_dirty(&mut self, p: usize) -> bool {
        std::mem::take(&mut self.sides[p].dirty)
    }

    // Intents.

    /// Starts over with the saved `workspace`, dropping every tab. Tabs whose
    /// file `exists` no longer reports are left out.
    pub fn reset(&mut self, workspace: Option<Workspace>, exists: impl Fn(&str) -> bool) -> Vec<Effect> {
        let mut fx = Vec::new();
        while self.sides.len() > 1 {
            self.remove_side(1, &mut fx);
        }
        for mut tab in std::mem::take(&mut self.sides[0].tabs) {
            tab.unload(&mut fx);
        }
        let workspace = restored(workspace.unwrap_or_default(), exists);
        for (p, state) in workspace.panes.iter().enumerate().take(2) {
            if p == 1 {
                self.add_side(&mut fx);
            }
            let tabs: Vec<Tab> = state.tabs.iter().map(|t| Tab::new(self.new_id(), t.clone())).collect();
            let side = &mut self.sides[p];
            side.active = state.active.min(tabs.len().saturating_sub(1));
            side.tabs = tabs;
        }
        if self.sides[0].tabs.is_empty() {
            self.push_empty(0);
        }
        self.focused = workspace.focused.min(self.sides.len() - 1);
        for p in 0..self.sides.len() {
            self.show(p, &mut fx);
        }
        fx.push(Effect::Focus);
        fx
    }

    /// Opens `target` where `place` says and focuses that side. The tab it
    /// lands on is live afterwards; the first `Create` or `Navigate` is its load.
    pub fn open(&mut self, target: String, place: Place, fragment: &str) -> Vec<Effect> {
        let mut fx = Vec::new();
        let p = match place {
            Place::Here | Place::NewTab => self.focused,
            Place::Beside => {
                if self.sides.len() == 1 {
                    self.add_side(&mut fx);
                }
                1 - self.focused
            }
        };
        let side = &self.sides[p];
        let i = if place == Place::Here && !side.tabs.is_empty() {
            side.active
        } else {
            let at = if side.tabs.is_empty() { 0 } else { side.active + 1 };
            let id = self.new_id();
            self.sides[p].tabs.insert(at, Tab::new(id, String::new()));
            at
        };
        let side = &mut self.sides[p];
        side.active = i;
        let tab = &mut side.tabs[i];
        tab.target.clone_from(&target);
        tab.title = None;
        tab.position = None;
        tab.restore = None;
        match tab.view {
            Some(view) => fx.push(Effect::Navigate { view, target, fragment: fragment.to_string() }),
            None => {
                tab.fragment = fragment.to_string();
                self.ensure_live(p, i, &mut fx);
            }
        }
        self.show(p, &mut fx);
        self.set_focus(p);
        fx.push(Effect::Focus);
        fx
    }

    /// Adds a tab at the end of the side holding `view`, without loading it.
    /// False when no side holds the view.
    pub fn open_background(&mut self, view: u64, target: String, fragment: String) -> bool {
        let Some((p, _)) = self.locate(view) else { return false };
        let mut tab = Tab::new(self.new_id(), target);
        tab.fragment = fragment;
        self.sides[p].tabs.push(tab);
        self.sides[p].dirty = true;
        true
    }

    /// Shows the tab with this id and focuses its side.
    pub fn select(&mut self, id: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        let Some((p, i)) = self.find_tab(id) else { return fx };
        self.sides[p].active = i;
        self.show(p, &mut fx);
        self.set_focus(p);
        fx.push(Effect::Focus);
        fx
    }

    /// Moves `step` tabs on the focused side, wrapping around.
    pub fn cycle(&mut self, step: i8) -> Vec<Effect> {
        let mut fx = Vec::new();
        let p = self.focused;
        let side = &mut self.sides[p];
        side.active = (side.active as isize + step as isize).rem_euclid(side.tabs.len() as isize) as usize;
        self.show(p, &mut fx);
        fx.push(Effect::Focus);
        fx
    }

    /// Shows the tab at place `n` (1 to 8) of the focused side, or the last one
    /// for 9. Nothing happens when the side has no tab at that place.
    pub fn select_nth(&mut self, n: usize) -> Vec<Effect> {
        let side = &self.sides[self.focused];
        let at = match n {
            1..=8 => n - 1,
            9 => match side.tabs.len().checked_sub(1) {
                Some(last) => last,
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let Some(tab) = side.tabs.get(at) else { return Vec::new() };
        let id = tab.id();
        self.select(id)
    }

    /// Closes the tab with this id, or the focused side's active one. The
    /// last tab of a side closes the side; the last of the window gives way
    /// to an empty tab.
    pub fn close(&mut self, id: Option<u64>) -> Vec<Effect> {
        let mut fx = Vec::new();
        let found = match id {
            Some(id) => self.find_tab(id),
            None => Some((self.focused, self.sides[self.focused].active)),
        };
        let Some((p, i)) = found else { return fx };
        self.sides[p].tabs.remove(i).unload(&mut fx);
        if self.sides[p].tabs.is_empty() {
            if self.sides.len() > 1 {
                self.remove_side(p, &mut fx);
                self.show(0, &mut fx);
                fx.push(Effect::Focus);
                return fx;
            }
            self.push_empty(p);
        }
        let side = &mut self.sides[p];
        if i < side.active || side.active >= side.tabs.len() {
            side.active = side.active.saturating_sub(1);
        }
        self.show(p, &mut fx);
        if p == self.focused {
            fx.push(Effect::Focus);
        }
        fx
    }

    /// Whether the window shows nothing but its empty tab.
    pub fn is_blank(&self) -> bool {
        self.sides.len() == 1 && self.sides[0].tabs.len() == 1 && self.sides[0].tabs[0].target.is_empty()
    }

    /// Closes every tab whose target is `target`, on any side, and says how
    /// many. Sides are settled once at the end, not after each tab: a side left
    /// empty goes, the right one first so the side numbers in the effects stay
    /// right, and an empty window gets an empty tab. The sides left show their
    /// active tab again, which may need a view.
    pub fn close_showing(&mut self, target: &str) -> (Vec<Effect>, usize) {
        let mut fx = Vec::new();
        let ids: Vec<u64> =
            self.sides.iter().flat_map(|s| &s.tabs).filter(|t| t.target == target).map(|t| t.id).collect();
        let focused_id = self.sides[self.focused].id;
        let mut touched = Vec::new();
        let mut closed = 0;
        for id in ids {
            let Some((p, i)) = self.find_tab(id) else { continue };
            let side = &mut self.sides[p];
            side.tabs.remove(i).unload(&mut fx);
            if i < side.active || side.active >= side.tabs.len() {
                side.active = side.active.saturating_sub(1);
            }
            if !touched.contains(&side.id) {
                touched.push(side.id);
            }
            closed += 1;
        }
        if closed == 0 {
            return (fx, 0);
        }
        let mut collapsed = false;
        for p in (0..self.sides.len()).rev() {
            if self.sides[p].tabs.is_empty() && self.sides.len() > 1 {
                self.remove_side(p, &mut fx);
                collapsed = true;
            }
        }
        // Only the last side can be left empty here.
        if self.sides[0].tabs.is_empty() {
            self.push_empty(0);
        }
        for p in 0..self.sides.len() {
            if touched.contains(&self.sides[p].id) {
                self.show(p, &mut fx);
            }
        }
        if collapsed || touched.contains(&focused_id) {
            fx.push(Effect::Focus);
        }
        (fx, closed)
    }

    /// Splits the window, with the focused document on both sides, or joins
    /// the two sides keeping every tab on the left. Joined views are not reloaded.
    pub fn toggle_split(&mut self) -> Vec<Effect> {
        if self.sides.len() == 1 {
            let target = self.focused_side().tab().target.clone();
            return self.open(target, Place::Beside, "");
        }
        let mut fx = Vec::new();
        let right_active = self.sides[1].active;
        let was_right = self.focused == 1;
        let moved = std::mem::take(&mut self.sides[1].tabs);
        let offset = self.sides[0].tabs.len();
        for view in moved.iter().filter_map(|t| t.view) {
            fx.push(Effect::MoveView { view, from: 1, to: 0 });
        }
        self.sides[0].tabs.extend(moved);
        self.remove_side(1, &mut fx);
        if was_right {
            self.sides[0].active = offset + right_active;
        }
        self.show(0, &mut fx);
        fx.push(Effect::Focus);
        fx
    }

    /// Swaps the two sides and focuses the left one.
    pub fn swap_sides(&mut self) -> Vec<Effect> {
        self.sides.swap(0, 1);
        self.focused = 0;
        for side in &mut self.sides {
            side.dirty = true;
        }
        vec![Effect::SwapSides]
    }

    /// Opens a file dropped on side `p`. Split, it goes to a new tab of that
    /// side. Not split, the window splits and the file takes the half it was
    /// dropped on (`half`, 0 left or 1 right), the other half keeping what it had.
    pub fn drop_file(&mut self, target: String, p: usize, half: Option<usize>) -> Vec<Effect> {
        if p >= self.sides.len() {
            return Vec::new();
        }
        if self.sides.len() == 2 {
            self.set_focus(p);
            return self.open(target, Place::NewTab, "");
        }
        self.set_focus(0);
        let mut fx = self.open(target, Place::Beside, "");
        // The new side is added on the right.
        if half == Some(0) {
            fx.extend(self.swap_sides());
        }
        fx
    }

    /// Focuses side `p`. False when it already had the focus.
    pub fn focus(&mut self, p: usize) -> bool {
        let changed = p != self.focused;
        self.set_focus(p);
        changed
    }

    /// Points the tabs showing `old`, or a file inside the folder `old`, at
    /// where it moved, or at the empty page if it was deleted. The flag says
    /// that a tab without a view changed, which only the saved workspace shows.
    pub fn retarget(&mut self, old: &str, new: Option<&str>) -> (Vec<Effect>, bool) {
        let mut fx = Vec::new();
        let mut hidden = false;
        for side in &mut self.sides {
            for tab in &mut side.tabs {
                // `a/b` is not the folder of `a/bc`.
                let inside = tab.target == old || tab.target.strip_prefix(old).is_some_and(|rest| rest.starts_with('/'));
                if !inside {
                    continue;
                }
                let next = new.map_or(String::new(), |n| format!("{n}{}", &tab.target[old.len()..]));
                tab.target.clone_from(&next);
                tab.position = None;
                tab.restore = None;
                side.dirty = true;
                match tab.view {
                    Some(view) => fx.push(Effect::Navigate { view, target: next, fragment: String::new() }),
                    None => hidden = true,
                }
            }
        }
        (fx, hidden)
    }

    // What the views report.

    /// The view showed another page, at `target` (empty if it is neither a
    /// vault page nor a website).
    pub fn navigated(&mut self, view: u64, target: String) -> Option<Navigated> {
        let (p, i) = self.find_view(view)?;
        let side = &mut self.sides[p];
        let tab = &mut side.tabs[i];
        let revealed = !tab.ready;
        tab.ready = true;
        tab.target = target;
        tab.title = None;
        side.dirty = true;
        Some(Navigated { side: p, active: side.active == i, revealed })
    }

    /// The view finished loading.
    pub fn loaded(&mut self, view: u64) -> Option<Loaded> {
        let (p, i) = self.find_view(view)?;
        let tab = &mut self.sides[p].tabs[i];
        Some(Loaded { side: p, restore: tab.restore.take(), web: is_web(&tab.target) })
    }

    pub fn set_title(&mut self, view: u64, title: Option<String>) {
        if let Some((p, i)) = self.find_view(view) {
            let tab = &mut self.sides[p].tabs[i];
            if tab.title != title {
                tab.title = title;
                self.sides[p].dirty = true;
            }
        }
    }

    pub fn set_position(&mut self, view: u64, position: Option<String>) {
        if let Some((p, i)) = self.find_view(view) {
            self.sides[p].tabs[i].position = position;
        }
    }

    /// The window could not build the view it was asked to `Create`.
    pub fn abort_view(&mut self, view: u64) {
        if let Some((p, i)) = self.find_view(view) {
            let tab = &mut self.sides[p].tabs[i];
            tab.view = None;
            tab.ready = false;
        }
    }

    // Internals.

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn find_tab(&self, id: u64) -> Option<(usize, usize)> {
        self.sides.iter().enumerate().find_map(|(p, s)| s.tabs.iter().position(|t| t.id == id).map(|i| (p, i)))
    }

    fn find_view(&self, view: u64) -> Option<(usize, usize)> {
        self.sides.iter().enumerate().find_map(|(p, s)| s.tabs.iter().position(|t| t.view == Some(view)).map(|i| (p, i)))
    }

    /// A new side on the right, which the caller fills at once.
    fn add_side(&mut self, fx: &mut Vec<Effect>) -> usize {
        let id = self.new_id();
        self.sides.push(Side { id, tabs: vec![], active: 0, dirty: true });
        fx.push(Effect::AddSide { id });
        self.sides.len() - 1
    }

    fn push_empty(&mut self, p: usize) {
        let id = self.new_id();
        let side = &mut self.sides[p];
        side.tabs.push(Tab::new(id, String::new()));
        side.active = side.tabs.len() - 1;
        side.dirty = true;
    }

    /// Drops side `p` with the views it still has; the focus goes to the side left.
    fn remove_side(&mut self, p: usize, fx: &mut Vec<Effect>) {
        for tab in &mut self.sides[p].tabs {
            tab.unload(fx);
        }
        self.sides.remove(p);
        fx.push(Effect::RemoveSide { side: p });
        self.focused = 0;
        for side in &mut self.sides {
            side.dirty = true;
        }
    }

    fn set_focus(&mut self, p: usize) {
        if p != self.focused {
            self.sides[self.focused].dirty = true;
            self.sides[p].dirty = true;
            self.focused = p;
        }
    }

    /// Makes the active tab of side `p` live and shown. The views that stay
    /// behind may be hidden by it, so they are asked for their position first.
    fn show(&mut self, p: usize, fx: &mut Vec<Effect>) {
        let active = self.sides[p].active;
        self.ensure_live(p, active, fx);
        let side = &mut self.sides[p];
        for (i, tab) in side.tabs.iter().enumerate() {
            if let Some(view) = tab.view
                && i != active
            {
                fx.push(Effect::SavePosition { view });
            }
        }
        side.dirty = true;
        fx.push(Effect::Show { side: p });
    }

    /// Gives the tab a view if it has none, and unloads the tab hidden
    /// longest ago while there are more than MAX_LIVE.
    fn ensure_live(&mut self, p: usize, i: usize, fx: &mut Vec<Effect>) {
        self.clock += 1;
        let (clock, view) = (self.clock, if self.sides[p].tabs[i].view.is_none() { self.new_id() } else { 0 });
        let tab = &mut self.sides[p].tabs[i];
        tab.used = clock;
        if tab.view.is_none() {
            tab.view = Some(view);
            fx.push(Effect::Create { side: p, view, target: tab.target.clone(), fragment: std::mem::take(&mut tab.fragment) });
        }
        while self.sides.iter().flat_map(|s| &s.tabs).filter(|t| t.view.is_some()).count() > MAX_LIVE {
            let oldest = self
                .sides
                .iter()
                .enumerate()
                .flat_map(|(p, s)| {
                    s.tabs.iter().enumerate().filter(move |(i, t)| t.view.is_some() && s.active != *i).map(move |(i, t)| (p, i, t.used))
                })
                .min_by_key(|(_, _, used)| *used);
            let Some((p, i, _)) = oldest else { break };
            let tab = &mut self.sides[p].tabs[i];
            tab.restore = tab.position.take();
            tab.unload(fx);
        }
    }
}

/// The saved tabs without those that no longer exist, at most two sides,
/// none empty. The active tab stays the same one when it survives.
fn restored(mut workspace: Workspace, exists: impl Fn(&str) -> bool) -> Workspace {
    let keep = |t: &String| t.is_empty() || is_web(t) || exists(t);
    for pane in &mut workspace.panes {
        let active = pane.tabs.get(pane.active).cloned();
        pane.tabs.retain(keep);
        pane.active = active.and_then(|a| pane.tabs.iter().position(|t| *t == a)).unwrap_or(0);
    }
    workspace.panes.retain(|p| !p.tabs.is_empty());
    workspace
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(tabs: &[&str], active: usize) -> PaneState {
        PaneState { tabs: tabs.iter().map(|t| t.to_string()).collect(), active }
    }

    fn build(panes: Vec<PaneState>, focused: usize) -> Tabs {
        let mut tabs = Tabs::new();
        tabs.reset(Some(Workspace { panes, focused }), |_| true);
        tabs
    }

    fn targets(tabs: &Tabs, p: usize) -> Vec<&str> {
        tabs.side(p).tabs().iter().map(Tab::target).collect()
    }

    fn live(tabs: &Tabs) -> Vec<&str> {
        tabs.live().map(|(_, t)| t).collect()
    }

    fn open(tabs: &mut Tabs, target: &str) -> Vec<Effect> {
        tabs.open(target.into(), Place::NewTab, "")
    }

    fn unloaded(fx: &[Effect]) -> usize {
        fx.iter().filter(|e| matches!(e, Effect::Unload { .. })).count()
    }

    /// Every invariant the fields guard.
    fn check(tabs: &Tabs) {
        assert!(!tabs.sides().is_empty() && tabs.sides().len() <= 2);
        assert!(tabs.focused() < tabs.sides().len());
        let mut views = Vec::new();
        for s in tabs.sides() {
            assert!(!s.tabs().is_empty());
            assert!(s.active() < s.tabs().len());
            assert!(s.tab().view().is_some(), "a visible tab must be live");
            views.extend(s.tabs().iter().filter_map(Tab::view));
        }
        assert!(views.len() <= MAX_LIVE);
        let n = views.len();
        views.sort();
        views.dedup();
        assert_eq!(views.len(), n, "a view belongs to one tab");
    }

    #[test]
    fn starts_with_one_empty_tab() {
        let tabs = Tabs::new();
        assert_eq!(targets(&tabs, 0), [""]);
        assert_eq!(tabs.side(0).tab().label(), "Nueva pestaña");
    }

    #[test]
    fn new_hands_out_ids_from_one() {
        let tabs = Tabs::new();
        assert_eq!(tabs.side(0).id(), 1);
        assert_eq!(tabs.side(0).tab().id(), 2);
    }

    #[test]
    fn new_from_hands_out_ids_above_the_first() {
        let base = 5 << 40;
        let mut tabs = Tabs::new_from(base);
        assert_eq!(tabs.side(0).id(), base + 1);
        assert_eq!(tabs.side(0).tab().id(), base + 2);
        let view = open(&mut tabs, "a.md").into_iter().find_map(|e| match e {
            Effect::Create { view, .. } => Some(view),
            _ => None,
        });
        assert!(view.is_some_and(|v| v > base));
        check(&tabs);
    }

    #[test]
    fn labels() {
        let tabs = build(vec![side(&["notas/a.md", "https://example.com/x/y", "/tmp/p.html"], 0)], 0);
        let labels: Vec<String> = tabs.side(0).tabs().iter().map(Tab::label).collect();
        assert_eq!(labels, ["a.md", "example.com", "p.html"]);
    }

    #[test]
    fn unloads_the_hidden_tab_used_longest_ago() {
        let mut tabs = build(vec![side(&[""], 0)], 0);
        for t in ["a", "b", "c"] {
            open(&mut tabs, t);
        }
        // The empty tab was used longest ago.
        assert_eq!(live(&tabs), ["a", "b", "c"]);
        let a = tabs.side(0).tabs()[1].id();
        assert_eq!(unloaded(&tabs.select(a)), 0);
        // Using "a" again spares it: "b" is now the one used longest ago.
        let fx = open(&mut tabs, "d");
        assert_eq!(unloaded(&fx), 1);
        assert_eq!(targets(&tabs, 0), ["", "a", "d", "b", "c"]);
        assert_eq!(live(&tabs), ["a", "d", "c"]);
        check(&tabs);
    }

    #[test]
    fn a_visible_tab_is_never_unloaded() {
        let mut tabs = build(vec![side(&["x"], 0)], 0);
        tabs.toggle_split();
        // Right side now shows "x" too; fill the left with more tabs.
        tabs.focus(0);
        for t in ["a", "b", "c", "d", "e"] {
            open(&mut tabs, t);
            check(&tabs);
        }
        assert!(live(&tabs).contains(&"x"), "the right side's page stays");
        assert!(live(&tabs).contains(&"e"));
        assert_eq!(live(&tabs).len(), 3);
    }

    #[test]
    fn with_two_sides_both_count_toward_the_limit() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 0), side(&["x", "y"], 0)], 0);
        // Showing each side's active tab gives two live tabs.
        assert_eq!(live(&tabs), ["a", "x"]);
        let b = tabs.side(0).tabs()[1].id();
        tabs.select(b);
        assert_eq!(live(&tabs), ["a", "b", "x"]);
        let c = tabs.side(0).tabs()[2].id();
        let fx = tabs.select(c);
        // Three would be a fourth: "a", used longest ago and hidden, goes.
        assert_eq!(unloaded(&fx), 1);
        assert_eq!(live(&tabs), ["b", "c", "x"]);
        check(&tabs);
    }

    #[test]
    fn unloading_keeps_the_reading_position_to_restore() {
        let mut tabs = build(vec![side(&[""], 0)], 0);
        open(&mut tabs, "a");
        let view = tabs.side(0).tabs()[1].view().unwrap();
        tabs.set_position(view, Some("{\"y\":10}".into()));
        for t in ["b", "c", "d"] {
            open(&mut tabs, t);
        }
        assert!(tabs.side(0).tabs()[1].view().is_none());
        let a = tabs.side(0).tabs()[1].id();
        let fx = tabs.select(a);
        let created = fx.iter().find_map(|e| if let Effect::Create { view, .. } = e { Some(*view) } else { None }).unwrap();
        tabs.navigated(created, "a".into());
        assert_eq!(tabs.loaded(created).unwrap().restore.as_deref(), Some("{\"y\":10}"));
        assert!(tabs.loaded(created).unwrap().restore.is_none());
    }

    #[test]
    fn closing_the_last_tab_of_a_side_closes_the_side() {
        let mut tabs = build(vec![side(&["a"], 0), side(&["b"], 0)], 1);
        let b = tabs.side(1).tab().id();
        let fx = tabs.close(Some(b));
        assert_eq!(tabs.sides().len(), 1);
        assert_eq!(tabs.focused(), 0);
        assert!(fx.contains(&Effect::RemoveSide { side: 1 }));
        assert_eq!(fx.last(), Some(&Effect::Focus));
        check(&tabs);
        // The left one too: the right moves over.
        let mut tabs = build(vec![side(&["a"], 0), side(&["b"], 0)], 0);
        tabs.close(None);
        assert_eq!(targets(&tabs, 0), ["b"]);
        check(&tabs);
    }

    #[test]
    fn closing_the_last_tab_of_the_window_leaves_an_empty_one() {
        let mut tabs = build(vec![side(&["a"], 0)], 0);
        let fx = tabs.close(None);
        assert_eq!(targets(&tabs, 0), [""]);
        assert!(!fx.iter().any(|e| matches!(e, Effect::RemoveSide { .. })));
        assert_eq!(unloaded(&fx), 1);
        check(&tabs);
    }

    #[test]
    fn closing_picks_the_neighbour() {
        let mut tabs = build(vec![side(&["a", "b", "c", "d"], 1)], 0);
        tabs.close(None);
        assert_eq!((targets(&tabs, 0), tabs.side(0).tab().target()), (vec!["a", "c", "d"], "c"));
        // The last one gives way to the previous.
        let d = tabs.side(0).tabs()[2].id();
        tabs.select(d);
        tabs.close(None);
        assert_eq!(tabs.side(0).tab().target(), "c");
        // A tab before the active one keeps the same page active.
        let a = tabs.side(0).tabs()[0].id();
        tabs.close(Some(a));
        assert_eq!(tabs.side(0).tab().target(), "c");
        check(&tabs);
    }

    #[test]
    fn closing_a_tab_on_the_other_side_keeps_the_focus() {
        let mut tabs = build(vec![side(&["a", "b"], 0), side(&["x", "y"], 0)], 0);
        let y = tabs.side(1).tabs()[1].id();
        let fx = tabs.close(Some(y));
        assert_eq!(tabs.focused(), 0);
        assert!(!fx.contains(&Effect::Focus));
        assert_eq!(targets(&tabs, 1), ["x"]);
    }

    #[test]
    fn closing_a_page_closes_its_tabs_on_both_sides() {
        let mut tabs = build(vec![side(&["a", "x", "a"], 2), side(&["b", "a"], 0)], 0);
        let (fx, n) = tabs.close_showing("a");
        assert_eq!(n, 3);
        assert_eq!(targets(&tabs, 0), ["x"]);
        assert_eq!(targets(&tabs, 1), ["b"]);
        // The visible tabs of both sides are live, the one that was left behind is not.
        assert_eq!(live(&tabs), ["x", "b"]);
        assert_eq!(fx.last(), Some(&Effect::Focus));
        check(&tabs);
    }

    #[test]
    fn closing_a_page_keeps_the_active_tab_when_it_survives() {
        let mut tabs = build(vec![side(&["a", "x", "y"], 2), side(&["b", "a"], 1)], 1);
        let (fx, n) = tabs.close_showing("a");
        assert_eq!(n, 2);
        assert_eq!(tabs.side(0).tab().target(), "y");
        assert_eq!(tabs.side(1).tab().target(), "b");
        assert_eq!(tabs.focused(), 1);
        assert_eq!(tabs.sides().len(), 2);
        assert!(fx.contains(&Effect::Focus));
        check(&tabs);
    }

    #[test]
    fn closing_a_page_that_empties_a_side_removes_it() {
        let mut tabs = build(vec![side(&["a", "b"], 0), side(&["a"], 0)], 1);
        let (fx, n) = tabs.close_showing("a");
        assert_eq!(n, 2);
        assert_eq!(tabs.sides().len(), 1);
        assert_eq!(targets(&tabs, 0), ["b"]);
        assert_eq!(tabs.focused(), 0);
        assert!(fx.contains(&Effect::RemoveSide { side: 1 }));
        assert_eq!(fx.last(), Some(&Effect::Focus));
        check(&tabs);
    }

    #[test]
    fn closing_a_page_that_empties_both_sides_leaves_one_empty_tab() {
        let mut tabs = build(vec![side(&["a"], 0), side(&["a", "a"], 1)], 1);
        let (fx, n) = tabs.close_showing("a");
        assert_eq!(n, 3);
        assert_eq!(tabs.sides().len(), 1);
        assert_eq!(targets(&tabs, 0), [""]);
        assert_eq!(fx.iter().filter(|e| matches!(e, Effect::RemoveSide { .. })).count(), 1);
        check(&tabs);
    }

    #[test]
    fn closing_the_last_tab_of_the_window_by_page_leaves_an_empty_one() {
        let mut tabs = build(vec![side(&["a"], 0)], 0);
        let (_, n) = tabs.close_showing("a");
        assert_eq!(n, 1);
        assert_eq!(targets(&tabs, 0), [""]);
        check(&tabs);
    }

    #[test]
    fn closing_a_page_nobody_shows_changes_nothing() {
        let mut tabs = build(vec![side(&["a", "b"], 1)], 0);
        let (fx, n) = tabs.close_showing("c");
        assert!(fx.is_empty());
        assert_eq!(n, 0);
        assert_eq!(targets(&tabs, 0), ["a", "b"]);
    }

    #[test]
    fn blank_means_only_the_empty_tab_is_left() {
        assert!(build(vec![side(&[""], 0)], 0).is_blank());
        assert!(!build(vec![side(&["a"], 0)], 0).is_blank());
        assert!(!build(vec![side(&[""], 0), side(&["a"], 0)], 0).is_blank());
    }

    #[test]
    fn here_replaces_the_tab() {
        let mut tabs = build(vec![side(&["a", "b"], 0)], 0);
        let fx = tabs.open("c".into(), Place::Here, "#s");
        assert_eq!(targets(&tabs, 0), ["c", "b"]);
        // "a" was live: it is told to load, not rebuilt.
        assert!(matches!(&fx[0], Effect::Navigate { target, fragment, .. } if target == "c" && fragment == "#s"));
    }

    #[test]
    fn here_on_a_tab_without_view_builds_it_with_the_fragment() {
        // The empty tab of a window just started has no view yet.
        let mut tabs = Tabs::new();
        let fx = tabs.open("z".into(), Place::Here, "#f");
        assert!(matches!(&fx[0], Effect::Create { side: 0, target, fragment, .. } if target == "z" && fragment == "#f"));
        check(&tabs);
    }

    #[test]
    fn a_new_tab_goes_after_the_active_one_and_is_shown() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 1)], 0);
        let fx = tabs.open("n".into(), Place::NewTab, "");
        assert_eq!(targets(&tabs, 0), ["a", "b", "n", "c"]);
        assert_eq!(tabs.side(0).active(), 2);
        assert!(matches!(&fx[0], Effect::Create { side: 0, target, .. } if target == "n"));
        assert!(fx.contains(&Effect::Show { side: 0 }));
        assert_eq!(fx.last(), Some(&Effect::Focus));
        check(&tabs);
    }

    #[test]
    fn beside_splits_when_needed_and_otherwise_uses_the_other_side() {
        let mut tabs = build(vec![side(&["a"], 0)], 0);
        let fx = tabs.open("b".into(), Place::Beside, "");
        assert!(matches!(fx[0], Effect::AddSide { .. }));
        assert_eq!((targets(&tabs, 0), targets(&tabs, 1)), (vec!["a"], vec!["b"]));
        assert_eq!(tabs.focused(), 1);
        // Already split: beside the right one is the left one.
        let fx = tabs.open("c".into(), Place::Beside, "");
        assert!(!fx.iter().any(|e| matches!(e, Effect::AddSide { .. })));
        assert_eq!(targets(&tabs, 0), ["a", "c"]);
        assert_eq!(tabs.focused(), 0);
        check(&tabs);
    }

    #[test]
    fn splitting_shows_the_focused_document_on_both_sides() {
        let mut tabs = build(vec![side(&["a", "b"], 1)], 0);
        let fx = tabs.toggle_split();
        assert!(matches!(fx[0], Effect::AddSide { .. }));
        assert_eq!(targets(&tabs, 1), ["b"]);
        assert_eq!(tabs.focused(), 1);
        check(&tabs);
    }

    #[test]
    fn joining_moves_the_right_tabs_left_without_reloading() {
        let mut tabs = build(vec![side(&["a", "b"], 0), side(&["x", "y"], 1)], 1);
        let right_view = tabs.side(1).tab().view().unwrap();
        let fx = tabs.toggle_split();
        assert_eq!(targets(&tabs, 0), ["a", "b", "x", "y"]);
        assert_eq!(tabs.sides().len(), 1);
        // The tab that was shown on the right is the one shown now.
        assert_eq!(tabs.side(0).tab().target(), "y");
        assert_eq!(tabs.side(0).tab().view(), Some(right_view));
        assert!(fx.contains(&Effect::MoveView { view: right_view, from: 1, to: 0 }));
        assert!(!fx.iter().any(|e| matches!(e, Effect::Create { .. } | Effect::Navigate { .. })));
        assert_eq!(fx.iter().filter(|e| matches!(e, Effect::RemoveSide { .. })).count(), 1);
        check(&tabs);
        // Focus on the left: its tab stays shown.
        let mut tabs = build(vec![side(&["a", "b"], 1), side(&["x"], 0)], 0);
        tabs.toggle_split();
        assert_eq!(tabs.side(0).tab().target(), "b");
        check(&tabs);
    }

    #[test]
    fn joining_keeps_the_limit() {
        let mut tabs = build(vec![side(&["a", "b"], 0), side(&["x", "y"], 0)], 0);
        let b = tabs.side(0).tabs()[1].id();
        tabs.select(b);
        assert_eq!(live(&tabs).len(), 3);
        tabs.toggle_split();
        check(&tabs);
    }

    #[test]
    fn cycle_wraps_around() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 2)], 0);
        tabs.cycle(1);
        assert_eq!(tabs.side(0).tab().target(), "a");
        tabs.cycle(-1);
        assert_eq!(tabs.side(0).tab().target(), "c");
        check(&tabs);
    }

    #[test]
    fn select_nth_shows_that_tab() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 0)], 0);
        tabs.select_nth(2);
        assert_eq!(tabs.side(0).tab().target(), "b");
        check(&tabs);
    }

    #[test]
    fn select_nth_out_of_range_changes_nothing() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 0)], 0);
        let fx = tabs.select_nth(5);
        assert!(fx.is_empty());
        assert_eq!(tabs.side(0).tab().target(), "a");
        check(&tabs);
    }

    #[test]
    fn select_nine_goes_to_the_last_tab() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 0)], 0);
        tabs.select_nth(9);
        assert_eq!(tabs.side(0).tab().target(), "c");
        check(&tabs);
    }

    #[test]
    fn select_nine_with_one_tab_is_that_tab() {
        let mut tabs = build(vec![side(&["a"], 0)], 0);
        tabs.select_nth(9);
        assert_eq!(tabs.side(0).tab().target(), "a");
        check(&tabs);
    }

    #[test]
    fn select_nth_acts_on_the_focused_side_only() {
        let mut tabs = build(vec![side(&["a", "b", "c"], 0), side(&["x", "y", "z"], 0)], 1);
        tabs.select_nth(2);
        assert_eq!(tabs.side(1).tab().target(), "y");
        assert_eq!(tabs.side(0).tab().target(), "a");
        assert_eq!(tabs.focused(), 1);
        check(&tabs);
        tabs.select_nth(9);
        assert_eq!(tabs.side(1).tab().target(), "z");
        assert_eq!(tabs.side(0).tab().target(), "a");
        check(&tabs);
    }

    #[test]
    fn restore_drops_tabs_that_no_longer_exist() {
        let mut tabs = Tabs::new();
        let ws = Workspace { panes: vec![side(&["a.md", "gone.md", "b.md"], 2), side(&["gone.md", "also.md"], 0)], focused: 1 };
        tabs.reset(Some(ws), |t| t == "a.md" || t == "b.md");
        assert_eq!(tabs.sides().len(), 1);
        assert_eq!(targets(&tabs, 0), ["a.md", "b.md"]);
        // Still on "b.md", which moved to index 1.
        assert_eq!(tabs.side(0).tab().target(), "b.md");
        // The focus was on a side that is gone.
        assert_eq!(tabs.focused(), 0);
        check(&tabs);
    }

    #[test]
    fn restore_keeps_empty_tabs_and_websites() {
        let mut tabs = Tabs::new();
        tabs.reset(Some(Workspace { panes: vec![side(&["", "https://example.com"], 1)], focused: 0 }), |_| false);
        assert_eq!(targets(&tabs, 0), ["", "https://example.com"]);
    }

    #[test]
    fn restore_clamps_a_wrong_active_tab() {
        let mut tabs = Tabs::new();
        tabs.reset(Some(Workspace { panes: vec![side(&["a", "b"], 9), side(&["x"], 4)], focused: 7 }), |_| true);
        assert_eq!(tabs.side(0).active(), 0);
        assert_eq!(tabs.side(1).active(), 0);
        assert_eq!(tabs.focused(), 1);
        check(&tabs);
    }

    #[test]
    fn restore_without_workspace_or_with_everything_gone_gives_an_empty_tab() {
        let mut tabs = build(vec![side(&["a"], 0), side(&["b"], 0)], 1);
        let fx = tabs.reset(None, |_| true);
        assert_eq!((tabs.sides().len(), targets(&tabs, 0)), (1, vec![""]));
        // Both sides' views go, and the right side.
        assert_eq!(unloaded(&fx), 2);
        assert!(fx.contains(&Effect::RemoveSide { side: 1 }));
        tabs.reset(Some(Workspace { panes: vec![side(&["gone"], 0)], focused: 0 }), |_| false);
        assert_eq!(targets(&tabs, 0), [""]);
        check(&tabs);
    }

    #[test]
    fn restore_shows_every_side_in_order() {
        let mut tabs = Tabs::new();
        let fx = tabs.reset(Some(Workspace { panes: vec![side(&["a"], 0), side(&["b"], 0)], focused: 1 }), |_| true);
        let kinds: Vec<&str> = fx
            .iter()
            .map(|e| match e {
                Effect::AddSide { .. } => "add",
                Effect::Create { side: 0, .. } => "create0",
                Effect::Create { .. } => "create1",
                Effect::Show { side: 0 } => "show0",
                Effect::Show { .. } => "show1",
                Effect::Focus => "focus",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["add", "create0", "show0", "create1", "show1", "focus"]);
        assert_eq!(tabs.focused(), 1);
    }

    #[test]
    fn retarget_a_file() {
        let mut tabs = build(vec![side(&["a.md", "b.md"], 0), side(&["a.md"], 0)], 0);
        let (fx, hidden) = tabs.retarget("a.md", Some("x/a.md"));
        assert_eq!((targets(&tabs, 0), targets(&tabs, 1)), (vec!["x/a.md", "b.md"], vec!["x/a.md"]));
        // Both are live; "b.md" is not live, and not touched.
        assert_eq!(fx.len(), 2);
        assert!(fx.iter().all(|e| matches!(e, Effect::Navigate { target, .. } if target == "x/a.md")));
        assert!(!hidden);
    }

    #[test]
    fn retarget_a_folder_not_its_namesakes() {
        let mut tabs = build(vec![side(&["a/b/n.md", "a/b", "a/bc/n.md", "a/b.md", "a/x.md"], 4)], 0);
        let (fx, hidden) = tabs.retarget("a/b", Some("z/b"));
        assert_eq!(targets(&tabs, 0), ["z/b/n.md", "z/b", "a/bc/n.md", "a/b.md", "a/x.md"]);
        // Only "a/x.md" is live, and it is not inside.
        assert!(fx.is_empty());
        assert!(hidden);
    }

    #[test]
    fn retarget_a_deleted_file_empties_the_tab() {
        let mut tabs = build(vec![side(&["d/a.md"], 0)], 0);
        let (fx, _) = tabs.retarget("d", None);
        assert_eq!(targets(&tabs, 0), [""]);
        assert!(matches!(&fx[0], Effect::Navigate { target, .. } if target.is_empty()));
    }

    #[test]
    fn retarget_marks_the_strip_only_when_something_changed() {
        let mut tabs = build(vec![side(&["a.md", "b.md"], 0)], 0);
        tabs.take_dirty(0);
        tabs.retarget("b.md", Some("c.md"));
        assert!(tabs.take_dirty(0));
        tabs.retarget("zzz", None);
        assert!(!tabs.take_dirty(0));
    }

    #[test]
    fn status_layout() {
        let tabs = build(vec![side(&["a", "b"], 1), side(&["x"], 0)], 1);
        assert_eq!(
            tabs.layout(),
            json!({ "list": [{ "tabs": ["a", "b"], "active": 1 }, { "tabs": ["x"], "active": 0 }], "focused": 1 })
        );
        let ws = tabs.workspace();
        assert_eq!((ws.focused, ws.panes[0].tabs.clone(), ws.panes[0].active), (1, vec!["a".to_string(), "b".into()], 1));
    }

    #[test]
    fn a_view_shows_its_page_once() {
        let mut tabs = Tabs::new();
        let view = tabs.side(0).tab().view();
        assert!(view.is_none());
        let fx = tabs.open("a".into(), Place::Here, "");
        let Effect::Create { view, .. } = fx[0] else { panic!("no create") };
        assert!(!tabs.side(0).tab().is_ready());
        let first = tabs.navigated(view, "a".into()).unwrap();
        assert!(first.revealed && first.active);
        assert!(!tabs.navigated(view, "a".into()).unwrap().revealed);
        assert!(tabs.navigated(9999, "a".into()).is_none());
    }

    #[test]
    fn an_unloaded_tab_is_not_ready_when_it_comes_back() {
        let mut tabs = Tabs::new();
        let fx = tabs.open("a".into(), Place::Here, "");
        let Effect::Create { view, .. } = fx[0] else { panic!("no create") };
        tabs.navigated(view, "a".into());
        for t in ["b", "c", "d"] {
            open(&mut tabs, t);
        }
        assert!(!tabs.side(0).tabs()[0].is_ready());
        check(&tabs);
    }

    #[test]
    fn a_failed_view_leaves_the_tab_unloaded() {
        let mut tabs = Tabs::new();
        let fx = tabs.open("a".into(), Place::Here, "");
        let Effect::Create { view, .. } = fx[0] else { panic!("no create") };
        tabs.abort_view(view);
        assert!(tabs.side(0).tab().view().is_none());
        assert!(tabs.locate(view).is_none());
    }

    #[test]
    fn the_strip_is_drawn_again_only_when_it_changed() {
        let mut tabs = build(vec![side(&["a", "b"], 0), side(&["x"], 0)], 0);
        assert!(tabs.take_dirty(0) && tabs.take_dirty(1));
        assert!(!tabs.take_dirty(0) && !tabs.take_dirty(1));
        // Switching tabs on the focused side touches that side only.
        tabs.cycle(1);
        assert!(tabs.take_dirty(0));
        assert!(!tabs.take_dirty(1));
        // Moving the focus touches both: the mark of the focused side moves.
        assert!(tabs.focus(1));
        assert!(!tabs.focus(1));
        assert!(tabs.take_dirty(0) && tabs.take_dirty(1));
        // A title is a change only when it differs.
        tabs.reset(Some(Workspace { panes: vec![side(&["https://e.com"], 0)], focused: 0 }), |_| true);
        let view = tabs.side(0).tab().view().unwrap();
        tabs.take_dirty(0);
        tabs.set_title(view, Some("E".into()));
        assert!(tabs.take_dirty(0));
        tabs.set_title(view, Some("E".into()));
        assert!(!tabs.take_dirty(0));
    }

    #[test]
    fn a_background_tab_costs_nothing_until_shown() {
        let mut tabs = build(vec![side(&["a"], 0), side(&["x"], 0)], 0);
        let x_view = tabs.side(1).tab().view().unwrap();
        assert!(tabs.open_background(x_view, "n".into(), "#f".into()));
        assert_eq!(targets(&tabs, 1), ["x", "n"]);
        assert!(tabs.side(1).tabs()[1].view().is_none());
        let n = tabs.side(1).tabs()[1].id();
        let fx = tabs.select(n);
        assert!(fx.iter().any(|e| matches!(e, Effect::Create { side: 1, fragment, .. } if fragment == "#f")));
        assert!(!tabs.open_background(9999, "n".into(), String::new()));
        check(&tabs);
    }

    #[test]
    fn swapping_sides_puts_the_focus_on_the_left() {
        let mut tabs = build(vec![side(&["a"], 0), side(&["b"], 0)], 1);
        let fx = tabs.swap_sides();
        assert_eq!(fx, [Effect::SwapSides]);
        assert_eq!((targets(&tabs, 0), tabs.focused()), (vec!["b"], 0));
    }

    #[test]
    fn a_dropped_file_takes_the_half_it_fell_on() {
        let mut tabs = build(vec![side(&["a"], 0)], 0);
        tabs.drop_file("n".into(), 0, Some(1));
        assert_eq!((targets(&tabs, 0), targets(&tabs, 1), tabs.focused()), (vec!["a"], vec!["n"], 1));
        check(&tabs);

        let mut tabs = build(vec![side(&["a"], 0)], 0);
        let fx = tabs.drop_file("n".into(), 0, Some(0));
        assert_eq!(fx.last(), Some(&Effect::SwapSides));
        assert_eq!((targets(&tabs, 0), targets(&tabs, 1), tabs.focused()), (vec!["n"], vec!["a"], 0));
        check(&tabs);
    }

    #[test]
    fn a_file_dropped_on_a_split_window_opens_in_a_new_tab_of_that_side() {
        let mut tabs = build(vec![side(&["a"], 0), side(&["b"], 0)], 0);
        let fx = tabs.drop_file("n".into(), 1, None);
        assert!(!fx.iter().any(|e| matches!(e, Effect::AddSide { .. } | Effect::SwapSides)));
        assert_eq!((targets(&tabs, 0), targets(&tabs, 1), tabs.focused()), (vec!["a"], vec!["b", "n"], 1));
        assert!(tabs.drop_file("m".into(), 2, None).is_empty(), "a side that is gone");
        check(&tabs);
    }

    #[test]
    fn locate_tells_the_side_and_whether_it_is_shown() {
        let tabs = build(vec![side(&["a", "b"], 0), side(&["x"], 0)], 0);
        let a = tabs.side(0).tab().view().unwrap();
        let x = tabs.side(1).tab().view().unwrap();
        assert_eq!((tabs.locate(a), tabs.locate(x)), (Some((0, true)), Some((1, true))));
        assert_eq!(tabs.live().count(), 2);
        assert_eq!(tabs.targets().collect::<Vec<_>>(), ["a", "b", "x"]);
    }
}
