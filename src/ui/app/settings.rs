//! Area E: the settings, updates and restart, the list of shortcuts and the
//! status bar. Frame side: `marco/src/areas/settings/`. Still to come: the
//! settings popover and the shortcuts list, which the frame loads when opened.

use super::App;
use crate::i18n::tr;
use crate::ui::action::{RestartCheck, UserEvent};
use crate::ui::page_calls;
use crate::ui::protocol::settings::Status;
use crate::ui::protocol::sides::kind_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use wry::WebView;

const RESTART_WAIT: Duration = Duration::from_secs(3);

impl App {
    pub(super) fn status(&self) -> Status {
        Status {
            vault: self.vault().map(|v| super::files::name_of(v.path())),
            kind: kind_of(self.tabs.focused_side().tab().target()),
            zoom: self.cfg(|c| c.zoom.unwrap_or(1.0)),
            version: env!("CARGO_PKG_VERSION"),
            update: self.update_message.clone(),
        }
    }

    /// Ctrl+K.
    pub(super) fn toggle_shortcuts(&mut self) {
        // Phase 1.
    }

    /// Ctrl+,.
    pub(super) fn open_settings(&mut self) {
        // Phase 1.
    }

    /// The language changes on the next start; the settings say so.
    pub(super) fn language_pending(&mut self, pending: bool) {
        // Phase 1.
        let _ = pending;
    }

    /// An update check, or a restart, ended with `message`. `installed` offers the restart.
    pub(super) fn update_done(&mut self, message: &str, installed: bool) {
        let _ = installed;
        self.update_message = Some(message.to_string());
    }

    /// Checks for a new version on a thread, so the window keeps answering; the
    /// result comes back as `UpdateDone`.
    pub(super) fn check_updates(&mut self) {
        self.update_message = Some(tr("Buscando…", "Checking…").to_string());
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let outcome = crate::update::run(true);
            let installed = matches!(outcome, crate::update::Outcome::Installed(_));
            proxy.send(UserEvent::UpdateDone { message: outcome.message(), installed });
        });
    }

    /// Restarts once no page has a note with unsaved text. Pages answer on the
    /// main loop, so each answer is counted here and the last one sends `RestartChecked`.
    /// A page that never answers (still loading, stuck) counts as unknown after
    /// `RESTART_WAIT`, so the button never does nothing.
    pub(super) fn ask_restart(&mut self) {
        let views: Vec<&WebView> = self.views.iter().map(|(_, v)| v).collect();
        if views.is_empty() {
            return self.restart();
        }
        let waiting = Arc::new(AtomicUsize::new(views.len()));
        let unsaved = Arc::new(AtomicBool::new(false));
        let unknown = Arc::new(AtomicBool::new(false));
        let decided = Arc::new(AtomicBool::new(false));
        {
            let (decided, proxy) = (decided.clone(), self.proxy.clone());
            gtk::glib::timeout_add_local_once(RESTART_WAIT, move || {
                if !decided.swap(true, Ordering::SeqCst) {
                    proxy.send(UserEvent::RestartChecked(RestartCheck::Unknown));
                }
            });
        }
        let settle = {
            let (waiting, unsaved, unknown, proxy) = (waiting.clone(), unsaved.clone(), unknown.clone(), self.proxy.clone());
            move || {
                if waiting.fetch_sub(1, Ordering::SeqCst) == 1 && !decided.swap(true, Ordering::SeqCst) {
                    let check = if unknown.load(Ordering::SeqCst) {
                        RestartCheck::Unknown
                    } else if unsaved.load(Ordering::SeqCst) {
                        RestartCheck::Unsaved
                    } else {
                        RestartCheck::Clear
                    };
                    proxy.send(UserEvent::RestartChecked(check));
                }
            }
        };
        for view in views {
            let (answered, unsaved_flag) = (settle.clone(), unsaved.clone());
            let asked = view.evaluate_script_with_callback(page_calls::note_unsaved(), move |answer| {
                if answer == "true" {
                    unsaved_flag.store(true, Ordering::SeqCst);
                }
                answered();
            });
            if asked.is_err() {
                unknown.store(true, Ordering::SeqCst);
                settle();
            }
        }
    }

    /// Asks the process to replace itself with the installed binary.
    pub(super) fn restart(&mut self) {
        // The process saves every window first; see `Visor::restart`.
        self.proxy.to(0).send(UserEvent::Restart { from: self.win });
    }
}
