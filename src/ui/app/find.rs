//! Area D, Ctrl+F. PDFs and the note editor have a search of their own; the
//! rest gets the frame's find bar, `marco/src/areas/find/`. Two engines run
//! the search: `page/find.js` for visor's own pages, which can say which match
//! is current, and the engine's own search for websites, which can only count
//! (`page_view::Finder`; WebKit's find controller on Linux).
//! Answers come back as events and are dropped when a newer search has started.

use super::App;
use crate::i18n::{Lang, lang};
use crate::ui::action::{Action, UserEvent};
use crate::ui::page_calls;
use crate::ui::page_view::{FindSignals, Finder};
use crate::ui::protocol::find::{Msg, State};
use crate::vault;
use serde_json::Value;

/// WebKit and `find.js` stop counting here: past it, the counter says "more than".
const MAX_MATCHES: u32 = 1000;

/// The find bar's state. `searched` is the view being searched, from the
/// search's start until it ends.
#[derive(Default)]
pub(super) struct Bar {
    open: bool,
    side: u64,
    text: String,
    match_case: bool,
    whole_word: bool,
    /// The match shown, 1-based; 0 when there is none.
    current: u32,
    total: u32,
    more: bool,
    /// Goes up each time the bar takes the keyboard.
    edit: u64,
    /// Numbers the requests to the page. An answer counts only if no newer request was made.
    request: u64,
    searched: Option<Search>,
}

struct Search {
    view: u64,
    engine: FindEngine,
    /// The page answered before, so `window.__visorFind` is in it and calls carry no script.
    loaded: bool,
    /// Only with `FindEngine::WebKit`.
    finder: Option<Finder>,
    /// The finder's signals; disconnected when the search ends.
    signals: Option<FindSignals>,
}

/// Which search runs in the page of a view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FindEngine {
    /// `page/find.js`, for the vault and loose files: visor's own pages only.
    Page,
    /// The engine's search, for websites, which get no script from visor.
    WebKit,
}

impl App {
    /// Ctrl+F: the page's own search if it has one, else the find bar.
    pub(super) fn find(&mut self) {
        let Some(id) = self.tabs.focused_view() else { return };
        if self.is_pdf_view(id) {
            self.eval(page_calls::pdf_find());
            return;
        }
        // The note editor answers later, as an event.
        let Some(view) = self.view() else { return };
        let proxy = self.proxy.clone();
        let asked = view.evaluate_script_with_callback(page_calls::note_find(), move |answer| {
            if native_wanted(&answer) {
                proxy.send(UserEvent::Action(Action::FindNative));
            }
        });
        if asked.is_err() {
            self.open_find();
        }
    }

    /// Opens the find bar over the focused side's page, or takes the keyboard
    /// into its field if it is already open on that page.
    pub(super) fn open_find(&mut self) {
        let Some(id) = self.tabs.focused_view() else { return };
        let side = self.tabs.focused_side().id();
        self.finder.open = true;
        self.finder.side = side;
        self.finder.edit += 1;
        self.host.focus_frame();
        if self.finder.searched.as_ref().is_some_and(|s| s.view == id) {
            return;
        }
        self.detach_find();
        let engine = if self.readable_page(id) { FindEngine::Page } else { FindEngine::WebKit };
        self.attach_find(id, engine);
        if !self.finder.text.is_empty() {
            self.run_find(None);
        }
    }

    /// Ends the search on a view that is about to be left or unloaded, and hides the bar.
    pub(super) fn forget_find(&mut self, view: u64) {
        if self.finder.searched.as_ref().is_some_and(|s| s.view == view) {
            self.detach_find();
            self.finder.open = false;
        }
    }

    /// What the frame shows: the bar's state and its counter.
    pub(super) fn find_state(&self) -> State {
        let f = &self.finder;
        let label = if f.text.is_empty() { String::new() } else { counter(f.current, f.total, f.more, lang()) };
        State {
            open: f.open,
            side: f.side,
            text: f.text.clone(),
            match_case: f.match_case,
            whole_word: f.whole_word,
            label,
            found: f.total > 0,
            edit: f.edit,
        }
    }

    /// A message from the frame's find bar.
    pub(super) fn on_find(&mut self, msg: Msg) {
        match msg {
            Msg::Search { text, match_case, whole_word } if self.finder.open => {
                self.finder.text = text;
                self.finder.match_case = match_case;
                self.finder.whole_word = whole_word;
                self.run_find(None);
            }
            Msg::Step { forward } if self.finder.open => self.run_find(Some(forward)),
            Msg::Close => self.close_find(true),
            _ => {}
        }
    }

    /// The answer of the page's search to request `request` on `view`.
    pub(super) fn find_answer(&mut self, view: u64, request: u64, raw: &str) {
        if request != self.finder.request {
            return;
        }
        let Some(s) = self.finder.searched.as_mut().filter(|s| s.view == view) else { return };
        if answered(raw) {
            s.loaded = true;
        }
        let (current, total, more) = parse_answer(raw);
        self.finder.current = current;
        self.finder.total = total;
        self.finder.more = more;
    }

    /// WebKit counted the matches of the search on `view`; `total` is capped at the limit.
    pub(super) fn find_found(&mut self, view: u64, total: u32, more: bool) {
        if self.finder.searched.as_ref().filter(|s| s.view == view).is_none() {
            return;
        }
        self.finder.total = total;
        self.finder.more = more;
        if total == 0 {
            self.finder.current = 0;
        }
    }

    /// Hides the bar and ends its search. `focus` gives the keyboard back to the page.
    fn close_find(&mut self, focus: bool) {
        let side = self.finder.side;
        if let Some(s) = self.finder.searched.as_ref().filter(|s| s.finder.is_none()) {
            self.eval_in(s.view, &page_calls::find_clear(s.loaded));
        }
        self.detach_find();
        self.finder.open = false;
        if focus && let Some(p) = self.side_index(side) {
            self.focus_pane(p, true);
        }
    }

    /// Makes `view` the one searched, with `engine`. Nothing runs in the page yet.
    fn attach_find(&mut self, view: u64, engine: FindEngine) {
        let (finder, signals) = match engine {
            FindEngine::Page => (None, None),
            FindEngine::WebKit => match self.view_of(view).and_then(|v| v.finder(&self.proxy, view, MAX_MATCHES)) {
                Some((finder, signals)) => (Some(finder), Some(signals)),
                None => return,
            },
        };
        self.finder.searched = Some(Search { view, engine, loaded: false, finder, signals });
    }

    /// Ends the search for good, without touching the page: its answers become
    /// stale, WebKit's search stops and its signals are disconnected.
    fn detach_find(&mut self) {
        self.finder.request = self.finder.request.wrapping_add(1);
        if let Some(Search { signals: Some(signals), .. }) = self.finder.searched.take() {
            signals.end();
        }
        self.finder.current = 0;
        self.finder.total = 0;
        self.finder.more = false;
    }

    /// Searches the text with the checkbox options (`None`), or moves to the
    /// next (`Some(true)`) or previous match (`Some(false)`).
    fn run_find(&mut self, step_to: Option<bool>) {
        let Some(s) = self.finder.searched.as_ref() else { return };
        let (view, engine, loaded, controller) = (s.view, s.engine, s.loaded, s.finder.clone());
        self.finder.request = self.finder.request.wrapping_add(1);
        let request = self.finder.request;
        let text = self.finder.text.clone();
        let (match_case, whole_word) = (self.finder.match_case, self.finder.whole_word);
        if text.is_empty() {
            match (engine, &controller) {
                (FindEngine::WebKit, Some(c)) => c.clear(),
                _ => self.eval_in(view, &page_calls::find_clear(loaded)),
            }
            self.finder.current = 0;
            self.finder.total = 0;
            self.finder.more = false;
            return;
        }
        match engine {
            FindEngine::WebKit => {
                let Some(c) = controller else { return };
                match step_to {
                    None => {
                        self.finder.current = 1;
                        self.finder.total = 0;
                        self.finder.more = false;
                        c.search(&text, match_case, whole_word);
                    }
                    Some(forward) if self.finder.total > 0 => {
                        self.finder.current = step(self.finder.current, self.finder.total, forward);
                        if forward {
                            c.next();
                        } else {
                            c.previous();
                        }
                    }
                    Some(_) => {}
                }
            }
            FindEngine::Page => {
                let script = match step_to {
                    None => page_calls::find_search(&text, match_case, whole_word, loaded),
                    Some(forward) => page_calls::find_step(forward, loaded),
                };
                let Some(web) = self.view_of(view) else { return };
                let proxy = self.proxy.clone();
                let _ = web.evaluate_script_with_callback(&script, move |raw| {
                    proxy.send(UserEvent::FindAnswer { view, request, raw });
                });
            }
        }
    }

    /// Whether the view shows a vault page or a loose file, which the page's own search may run in.
    fn readable_page(&self, view: u64) -> bool {
        let url = self.documents.get(&view).map(|d| d.borrow().clone()).unwrap_or_default();
        let vault = self.vault();
        vault::agent_may_read(&url, vault.as_deref())
    }

    fn eval_in(&self, view: u64, code: &str) {
        if let Some(web) = self.view_of(view) {
            let _ = web.evaluate_script(code);
        }
    }
}

/// Whether the answer of `note_find` leaves Ctrl+F to the find bar. Anything but
/// "true" does: the editor did not show a search, or the page did not answer.
fn native_wanted(answer: &str) -> bool {
    answer != "true"
}

/// Whether the page's answer is a search result, so the page runs the search.
fn answered(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw).is_ok_and(|v| v.get("total").is_some())
}

/// Reads the page's answer, `{"current": n, "total": n, "more": bool}`; `more`
/// says the search stopped at its limit. Anything else counts as no matches.
fn parse_answer(raw: &str) -> (u32, u32, bool) {
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return (0, 0, false) };
    let field = |name: &str| value[name].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
    (field("current"), field("total"), value["more"].as_bool().unwrap_or(false))
}

/// The match shown after one step from `current` (1-based, 0 for none),
/// wrapping around at both ends as the search does.
fn step(current: u32, total: u32, forward: bool) -> u32 {
    if total == 0 {
        0
    } else if forward {
        if current >= total { 1 } else { current + 1 }
    } else if current <= 1 {
        total
    } else {
        current - 1
    }
}

/// The counter next to the field: "3 de 12", "3 de más de 1000" (`more`: the
/// search stopped at `total`), or "Sin resultados".
fn counter(current: u32, total: u32, more: bool, lang: Lang) -> String {
    match (total, more, lang) {
        (0, _, Lang::Es) => "Sin resultados".into(),
        (0, _, Lang::En) => "No results".into(),
        (total, false, Lang::Es) => format!("{current} de {total}"),
        (total, false, Lang::En) => format!("{current} of {total}"),
        (total, true, Lang::Es) => format!("{current} de más de {total}"),
        (total, true, Lang::En) => format!("{current} of more than {total}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_of_the_page_gives_its_counts() {
        assert_eq!(parse_answer(r#"{"current":3,"total":12,"more":false}"#), (3, 12, false));
        assert_eq!(parse_answer(r#"{"current":1,"total":1000,"more":true}"#), (1, 1000, true));
        assert_eq!(parse_answer("null"), (0, 0, false));
        assert_eq!(parse_answer("no es JSON"), (0, 0, false));
        assert!(answered(r#"{"current":0,"total":0,"more":false}"#));
        assert!(!answered("true"));
        assert!(!answered("null"));
    }

    #[test]
    fn a_step_wraps_at_both_ends() {
        assert_eq!(step(3, 12, true), 4);
        assert_eq!(step(12, 12, true), 1);
        assert_eq!(step(1, 12, false), 12);
        assert_eq!(step(5, 12, false), 4);
        assert_eq!(step(0, 0, true), 0);
    }

    #[test]
    fn the_counter_says_the_count_in_the_language() {
        assert_eq!(counter(3, 12, false, Lang::Es), "3 de 12");
        assert_eq!(counter(1, 1000, true, Lang::Es), "1 de más de 1000");
        assert_eq!(counter(0, 0, false, Lang::Es), "Sin resultados");
        assert_eq!(counter(2, 5, false, Lang::En), "2 of 5");
    }

    #[test]
    fn a_note_editor_without_a_search_leaves_ctrl_f_to_the_bar() {
        assert!(!native_wanted("true"));
        assert!(native_wanted("false"));
        assert!(native_wanted(""));
    }
}
