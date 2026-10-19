//! Area D, Ctrl+F. PDFs and the note editor have a search of their own; the
//! rest gets the frame's find bar, `marco/src/areas/find/`, in phase 1.

use super::App;
use crate::ui::action::{Action, UserEvent};
use crate::ui::page_calls;

impl App {
    /// Ctrl+F: the page's own search if it has one, else the find bar.
    pub(super) fn find(&self) {
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

    /// Opens the find bar over the focused side's page.
    pub(super) fn open_find(&self) {
        // Phase 1.
    }
}

/// Whether the note editor's answer to `note_find` leaves Ctrl+F to the find bar.
fn native_wanted(answer: &str) -> bool {
    answer != "true"
}
