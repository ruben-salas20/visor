//! The find bar of a side: Ctrl+F in a page without a search of its own (see
//! `Action::Find`). It searches with one of two engines, chosen when it opens:
//! the page itself (`page/find.js`) for visor's own pages, which can say which
//! match is current, or WebKit's find controller for websites, which cannot be
//! asked for anything but the count. Each side makes its bar on the first
//! Ctrl+F, then only shows and hides it.

use crate::i18n::{Lang, lang, tr};
use crate::ui::icons::{draw_back, draw_close, draw_forward, icon_button};
use crate::ui::page_calls;
use gtk::gdk::ModifierType;
use gtk::gdk::keys::constants as key;
use gtk::glib::{Propagation, SignalHandlerId};
use gtk::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use webkit2gtk::{FindController, FindControllerExt, FindOptions, WebView, WebViewExt};

/// WebKit stops counting here, so a page with more matches shows this many.
const MAX_MATCHES: u32 = 1000;

/// Whether the answer of `note_find` asks for the native bar. Anything but
/// "true" does: the editor did not show a search, or the page did not answer.
pub fn native_wanted(answer: &str) -> bool {
    answer != "true"
}

/// Which search runs in the page of a view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// `page/find.js`, for the vault and loose files: they run only visor's script.
    Page,
    /// WebKit's find controller, for websites, which get no script from visor.
    WebKit,
}

/// Reads the page's answer, `{"current": n, "total": n}`. Anything else, such
/// as an error or a page that no longer runs the search, counts as no matches.
pub fn parse_answer(raw: &str) -> (u32, u32) {
    let Ok(value) = serde_json::from_str::<Value>(raw) else { return (0, 0) };
    let field = |name: &str| value[name].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
    (field("current"), field("total"))
}

/// The match shown after one step from `current` (1-based, 0 for none),
/// wrapping around at both ends as the search does.
pub fn step(current: u32, total: u32, forward: bool) -> u32 {
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

/// The counter next to the entry: "3 de 12", or "Sin resultados".
pub fn counter(current: u32, total: u32, lang: Lang) -> String {
    match (total, lang) {
        (0, Lang::Es) => "Sin resultados".into(),
        (0, Lang::En) => "No results".into(),
        (total, Lang::Es) => format!("{current} de {total}"),
        (total, Lang::En) => format!("{current} of {total}"),
    }
}

/// The search options for the two checkboxes. WebKitGTK 2.0 has no flag for
/// the end of a word, so "whole word" only checks the start.
pub fn options(match_case: bool, whole_word: bool) -> FindOptions {
    let mut options = FindOptions::WRAP_AROUND;
    if !match_case {
        options |= FindOptions::CASE_INSENSITIVE;
    }
    if whole_word {
        options |= FindOptions::AT_WORD_STARTS;
    }
    options
}

/// The bar of one side. Clones share the same bar: its signal handlers hold
/// a clone, so the bar lives as long as the window does.
#[derive(Clone)]
pub struct FindBar(Rc<Inner>);

struct Inner {
    frame: gtk::Box,
    entry: gtk::Entry,
    count: gtk::Label,
    match_case: gtk::CheckButton,
    whole_word: gtk::CheckButton,
    /// The match shown, 1-based; 0 when there is none.
    current: Cell<u32>,
    total: Cell<u32>,
    /// Numbers the page's requests. An answer is used only if no newer request
    /// was made, and `close`/`detach` make every pending answer stale.
    request: Cell<u64>,
    /// The view being searched, from `open` until `detach`.
    searched: RefCell<Option<Searched>>,
}

struct Searched {
    view: u64,
    web: WebView,
    engine: Engine,
    /// Only with `Engine::WebKit`.
    controller: Option<FindController>,
    /// Connected to `controller`; disconnected in `detach` so the cycle of
    /// clones through the handlers ends with the search.
    handlers: Vec<SignalHandlerId>,
}

/// Runs `script` in the page of `web` and hands its answer to `on_answer`.
/// Only ever used for the visor pages that the engine was chosen for.
/// `run_javascript` is the one the `v2_38` feature has: `evaluate_javascript`
/// needs `v2_40`, which the crate does not enable.
#[allow(deprecated)]
fn evaluate(web: &WebView, script: &str, on_answer: impl FnOnce(Result<String, ()>) + 'static) {
    web.run_javascript(script, None::<&gtk::gio::Cancellable>, move |result| {
        let text = result.ok().and_then(|r| r.js_value()).map(|value| value.to_string());
        on_answer(text.ok_or(()));
    });
}

impl FindBar {
    /// Makes the bar, hidden, over the top right of `overlay`.
    pub fn new(overlay: &gtk::Overlay) -> FindBar {
        let frame = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        frame.style_context().add_class("visor-bar");
        frame.set_halign(gtk::Align::End);
        frame.set_valign(gtk::Align::Start);
        frame.set_margin_top(8);
        frame.set_margin_end(8);
        let entry = gtk::Entry::new();
        entry.set_width_chars(24);
        entry.set_placeholder_text(Some(tr("Buscar en la página", "Find in the page")));
        let previous = icon_button(tr("Anterior (Mayús+Enter)", "Previous (Shift+Enter)"), draw_back);
        let next = icon_button(tr("Siguiente (Enter)", "Next (Enter)"), draw_forward);
        let close = icon_button(tr("Cerrar (Esc)", "Close (Esc)"), draw_close);
        for button in [&previous, &next, &close] {
            button.set_can_focus(false);
        }
        let count = gtk::Label::new(None);
        let match_case = gtk::CheckButton::with_label(tr("Mayúsculas", "Match case"));
        let whole_word = gtk::CheckButton::with_label(tr("Palabra completa", "Whole word"));
        whole_word.set_tooltip_text(Some(tr(
            "En las webs, solo comprueba el principio de la palabra",
            "On websites, only the start of the word is checked",
        )));
        frame.pack_start(&entry, true, true, 0);
        frame.pack_start(&count, false, false, 0);
        frame.pack_start(&previous, false, false, 0);
        frame.pack_start(&next, false, false, 0);
        frame.pack_start(&match_case, false, false, 0);
        frame.pack_start(&whole_word, false, false, 0);
        frame.pack_start(&close, false, false, 0);
        overlay.add_overlay(&frame);
        // Children shown once; from here on only the frame is shown and hidden.
        for child in frame.children() {
            child.show_all();
        }
        let bar = FindBar(Rc::new(Inner {
            frame,
            entry,
            count,
            match_case,
            whole_word,
            current: Cell::new(0),
            total: Cell::new(0),
            request: Cell::new(0),
            searched: RefCell::new(None),
        }));
        let b = bar.clone();
        bar.0.entry.connect_changed(move |_| b.run(None));
        let b = bar.clone();
        bar.0.match_case.connect_toggled(move |_| b.run(None));
        let b = bar.clone();
        bar.0.whole_word.connect_toggled(move |_| b.run(None));
        let b = bar.clone();
        previous.connect_clicked(move |_| b.run(Some(false)));
        let b = bar.clone();
        next.connect_clicked(move |_| b.run(Some(true)));
        let b = bar.clone();
        close.connect_clicked(move |_| b.close(true));
        let b = bar.clone();
        bar.0.entry.connect_key_press_event(move |_, event| {
            let shift = event.state().contains(ModifierType::SHIFT_MASK);
            match event.keyval() {
                key::Return | key::KP_Enter => b.run(Some(!shift)),
                key::Escape => b.close(true),
                _ => return Propagation::Proceed,
            }
            Propagation::Stop
        });
        bar
    }

    /// Shows the bar and searches `view`, whose WebKit view is `web`, with
    /// `engine`. Does nothing to the search if the bar already shows this view.
    pub fn open(&self, view: u64, web: &WebView, engine: Engine) {
        let changed = self.attach(view, web, engine);
        let was_shown = self.0.frame.is_visible();
        self.0.frame.show();
        self.focus_text();
        if changed || !was_shown {
            self.run(None);
        }
    }

    /// Puts the keyboard in the entry, with its text selected.
    pub fn focus_text(&self) {
        self.0.entry.grab_focus();
        self.0.entry.select_region(0, -1);
    }

    pub fn is_open(&self) -> bool {
        self.0.frame.is_visible()
    }

    /// Hides the bar and ends the search, keeping its view. `focus` gives the
    /// keyboard back to the page.
    pub fn close(&self, focus: bool) {
        self.cancel();
        if let Some(s) = self.0.searched.borrow().as_ref() {
            match &s.controller {
                Some(controller) => controller.search_finish(),
                None => evaluate(&s.web, &page_calls::find_clear(), |_| {}),
            }
            if focus {
                s.web.grab_focus();
            }
        }
        self.0.current.set(0);
        self.0.total.set(0);
        self.0.frame.hide();
    }

    /// Drops the search if it is on `view`, which is about to be unloaded, moved,
    /// or left for another page.
    pub fn forget(&self, view: u64) {
        if self.0.searched.borrow().as_ref().is_some_and(|s| s.view == view) {
            self.detach();
            self.0.frame.hide();
        }
    }

    /// Makes `view` the one searched. False if the view was already it, or
    /// WebKit has no find controller for it.
    fn attach(&self, view: u64, web: &WebView, engine: Engine) -> bool {
        if self.0.searched.borrow().as_ref().is_some_and(|s| s.view == view) {
            return false;
        }
        self.detach();
        if engine == Engine::Page {
            *self.0.searched.borrow_mut() = Some(Searched { view, web: web.clone(), engine, controller: None, handlers: Vec::new() });
            return true;
        }
        let Some(controller) = web.find_controller() else { return false };
        let b = self.clone();
        let found = controller.connect_found_text(move |_, n| {
            b.0.total.set(n);
            b.refresh();
        });
        let b = self.clone();
        let missed = controller.connect_failed_to_find_text(move |_| {
            b.0.total.set(0);
            b.0.current.set(0);
            b.refresh();
        });
        *self.0.searched.borrow_mut() = Some(Searched {
            view,
            web: web.clone(),
            engine,
            controller: Some(controller),
            handlers: vec![found, missed],
        });
        true
    }

    /// Ends the search on the current view and forgets it. Nothing runs in the
    /// page here: the view may already show another page.
    fn detach(&self) {
        self.cancel();
        let taken = self.0.searched.borrow_mut().take();
        if let Some(Searched { controller: Some(controller), handlers, .. }) = taken {
            controller.search_finish();
            for id in handlers {
                controller.disconnect(id);
            }
        }
        self.0.current.set(0);
        self.0.total.set(0);
    }

    /// Makes every answer still on its way stale.
    fn cancel(&self) {
        self.0.request.set(self.0.request.get().wrapping_add(1));
    }

    /// Searches the entry's text with the checkbox options (`None`), or moves
    /// to the next (`Some(true)`) or previous match (`Some(false)`).
    fn run(&self, step_to: Option<bool>) {
        let inner = &self.0;
        let text = inner.entry.text();
        let searched = inner.searched.borrow();
        if let Some(s) = searched.as_ref() {
            match s.engine {
                Engine::WebKit => self.run_webkit(s, &text, step_to),
                Engine::Page => self.run_page(s, &text, step_to),
            }
        }
        drop(searched);
        self.refresh();
    }

    fn run_webkit(&self, s: &Searched, text: &str, step_to: Option<bool>) {
        let inner = &self.0;
        let Some(controller) = s.controller.as_ref() else { return };
        if text.is_empty() {
            controller.search_finish();
            inner.current.set(0);
            inner.total.set(0);
            return;
        }
        match step_to {
            None => {
                inner.current.set(1);
                inner.total.set(0);
                let flags = options(inner.match_case.is_active(), inner.whole_word.is_active());
                controller.search(text, flags.bits(), MAX_MATCHES);
            }
            Some(forward) if inner.total.get() > 0 => {
                inner.current.set(step(inner.current.get(), inner.total.get(), forward));
                if forward {
                    controller.search_next();
                } else {
                    controller.search_previous();
                }
            }
            Some(_) => {}
        }
    }

    fn run_page(&self, s: &Searched, text: &str, step_to: Option<bool>) {
        let inner = &self.0;
        self.cancel();
        if text.is_empty() {
            evaluate(&s.web, &page_calls::find_clear(), |_| {});
            inner.current.set(0);
            inner.total.set(0);
            return;
        }
        let request = inner.request.get();
        let script = match step_to {
            None => page_calls::find_search(text, inner.match_case.is_active(), inner.whole_word.is_active()),
            Some(forward) => page_calls::find_step(forward),
        };
        let b = self.clone();
        evaluate(&s.web, &script, move |answer| {
            if b.0.request.get() != request {
                return;
            }
            let (current, total) = answer.map(|raw| parse_answer(&raw)).unwrap_or((0, 0));
            b.0.current.set(current);
            b.0.total.set(total);
            b.refresh();
        });
    }

    fn refresh(&self) {
        let inner = &self.0;
        let text = if inner.entry.text().is_empty() {
            String::new()
        } else {
            counter(inner.current.get(), inner.total.get(), lang())
        };
        inner.count.set_text(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_names_the_match_and_the_total() {
        assert_eq!(counter(3, 12, Lang::Es), "3 de 12");
        assert_eq!(counter(3, 12, Lang::En), "3 of 12");
        assert_eq!(counter(0, 0, Lang::Es), "Sin resultados");
        assert_eq!(counter(0, 0, Lang::En), "No results");
    }

    #[test]
    fn steps_wrap_at_both_ends() {
        assert_eq!(step(1, 3, true), 2);
        assert_eq!(step(3, 3, true), 1);
        assert_eq!(step(1, 3, false), 3);
        assert_eq!(step(2, 3, false), 1);
        assert_eq!(step(0, 0, true), 0);
        assert_eq!(step(0, 0, false), 0);
    }

    #[test]
    fn options_follow_the_checkboxes() {
        let plain = options(false, false);
        assert!(plain.contains(FindOptions::CASE_INSENSITIVE));
        assert!(plain.contains(FindOptions::WRAP_AROUND));
        assert!(!plain.contains(FindOptions::AT_WORD_STARTS));
        let exact = options(true, true);
        assert!(!exact.contains(FindOptions::CASE_INSENSITIVE));
        assert!(exact.contains(FindOptions::AT_WORD_STARTS));
        assert!(exact.contains(FindOptions::WRAP_AROUND));
    }

    #[test]
    fn only_a_true_answer_keeps_the_note_search() {
        assert!(!native_wanted("true"));
        assert!(native_wanted("false"));
        assert!(native_wanted(""));
    }

    #[test]
    fn the_page_answer_gives_current_and_total() {
        assert_eq!(parse_answer(r#"{"current":2,"total":5}"#), (2, 5));
        assert_eq!(parse_answer(r#"{"current":0,"total":0}"#), (0, 0));
    }

    #[test]
    fn a_damaged_answer_counts_as_no_matches() {
        assert_eq!(parse_answer(""), (0, 0));
        assert_eq!(parse_answer("null"), (0, 0));
        assert_eq!(parse_answer(r#"{"total":"3"}"#), (0, 0));
        assert_eq!(parse_answer(r#"{"current":1,"total":-4}"#), (1, 0));
    }
}
