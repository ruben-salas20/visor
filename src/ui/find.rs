//! The find bar of a side: Ctrl+F in a page without a search of its own (see
//! `Action::Find`). It drives WebKit's find controller of the view it was
//! opened on. Each side makes its bar on the first Ctrl+F, then only shows and
//! hides it.

use crate::i18n::{Lang, lang, tr};
use crate::ui::icons::{draw_back, draw_close, draw_forward, icon_button};
use gtk::gdk::ModifierType;
use gtk::gdk::keys::constants as key;
use gtk::glib::{Propagation, SignalHandlerId};
use gtk::prelude::*;
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
    /// The view being searched, from `open` until `detach`.
    searched: RefCell<Option<Searched>>,
}

struct Searched {
    view: u64,
    web: WebView,
    controller: FindController,
    /// Connected to `controller`; disconnected in `detach` so the cycle of
    /// clones through the handlers ends with the search.
    handlers: Vec<SignalHandlerId>,
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
            "Coincide solo al principio de una palabra",
            "Matches only at the start of a word",
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

    /// Shows the bar and searches `view`, whose WebKit view is `web`. Does
    /// nothing to the search if the bar already shows this view.
    pub fn open(&self, view: u64, web: &WebView) {
        let changed = self.attach(view, web);
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
        if let Some(s) = self.0.searched.borrow().as_ref() {
            s.controller.search_finish();
            if focus {
                s.web.grab_focus();
            }
        }
        self.0.current.set(0);
        self.0.total.set(0);
        self.0.frame.hide();
    }

    /// Drops the search if it is on `view`, which is about to be unloaded or moved.
    pub fn forget(&self, view: u64) {
        if self.0.searched.borrow().as_ref().is_some_and(|s| s.view == view) {
            self.detach();
            self.0.frame.hide();
        }
    }

    /// Makes `view` the one searched. False if the view was already it, or
    /// WebKit has no find controller for it.
    fn attach(&self, view: u64, web: &WebView) -> bool {
        if self.0.searched.borrow().as_ref().is_some_and(|s| s.view == view) {
            return false;
        }
        self.detach();
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
        *self.0.searched.borrow_mut() = Some(Searched { view, web: web.clone(), controller, handlers: vec![found, missed] });
        true
    }

    /// Ends the search on the current view and forgets it.
    fn detach(&self) {
        let taken = self.0.searched.borrow_mut().take();
        if let Some(s) = taken {
            s.controller.search_finish();
            for id in s.handlers {
                s.controller.disconnect(id);
            }
        }
        self.0.current.set(0);
        self.0.total.set(0);
    }

    /// Searches the entry's text with the checkbox options (`None`), or moves
    /// to the next (`Some(true)`) or previous match (`Some(false)`).
    fn run(&self, step_to: Option<bool>) {
        let inner = &self.0;
        let text = inner.entry.text();
        let searched = inner.searched.borrow();
        if let Some(s) = searched.as_ref() {
            if text.is_empty() {
                s.controller.search_finish();
                inner.current.set(0);
                inner.total.set(0);
            } else {
                match step_to {
                    None => {
                        inner.current.set(1);
                        inner.total.set(0);
                        let flags = options(inner.match_case.is_active(), inner.whole_word.is_active());
                        s.controller.search(&text, flags.bits(), MAX_MATCHES);
                    }
                    Some(forward) if inner.total.get() > 0 => {
                        inner.current.set(step(inner.current.get(), inner.total.get(), forward));
                        if forward {
                            s.controller.search_next();
                        } else {
                            s.controller.search_previous();
                        }
                    }
                    Some(_) => {}
                }
            }
        }
        drop(searched);
        self.refresh();
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
}
