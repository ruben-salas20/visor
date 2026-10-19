//! Area E: the settings panel, the shortcuts panel, updates and restart, and
//! the status bar. Frame side: `marco/src/areas/settings/`.

use super::App;
use crate::i18n::tr;
use crate::ui::action::{Action, RestartCheck, UserEvent};
use crate::ui::address::Engine;
use crate::ui::page_calls;
use crate::ui::protocol::settings::{Choice, Msg, Panel, Prefs, Status};
use crate::ui::protocol::sides::kind_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use wry::WebView;

const RESTART_WAIT: Duration = Duration::from_secs(3);

impl App {
    pub(super) fn status(&self) -> Status {
        let (theme, language, engine, auto_update, show_other) = self.cfg(|c| {
            (
                c.theme.clone(),
                c.language.clone(),
                c.search_engine.clone(),
                c.auto_update.unwrap_or(true),
                c.show_other_files.unwrap_or(true),
            )
        });
        Status {
            vault: self.vault().map(|v| super::files::name_of(v.path())),
            kind: kind_of(self.tabs.focused_side().tab().target()),
            zoom: self.cfg(|c| c.zoom.unwrap_or(1.0)),
            version: env!("CARGO_PKG_VERSION"),
            update: self.update_message.clone(),
            update_busy: self.update_busy,
            update_restart: self.update_restart,
            language_pending: self.language_pending,
            prefs: Prefs {
                theme,
                language,
                search_engine: Engine::from_id(engine.as_deref()).id(),
                engines: Engine::ALL.into_iter().map(|e| Choice { id: e.id(), label: e.label() }).collect(),
                auto_update,
                show_other,
            },
            settings_open: self.settings_open,
            shortcuts_open: self.shortcuts_open,
        }
    }

    /// Ctrl+K and the keyboard button of the status bar.
    pub(super) fn toggle_shortcuts(&mut self) {
        self.shortcuts_open = !self.shortcuts_open;
        if !self.shortcuts_open {
            self.return_to_page();
        }
    }

    /// Ctrl+,.
    pub(super) fn open_settings(&mut self) {
        self.settings_open = true;
    }

    /// The language chosen differs from the one in use: it applies on restart.
    pub(super) fn language_pending(&mut self, pending: bool) {
        self.language_pending = pending;
    }

    /// An update check, or a restart, ended with `message`. `installed` offers the restart.
    pub(super) fn update_done(&mut self, message: &str, installed: bool) {
        self.update_busy = false;
        self.update_restart = installed;
        self.update_message = Some(message.to_string());
    }

    /// Checks for a new version on a thread, so the window keeps answering; the
    /// result comes back as `UpdateDone`.
    pub(super) fn check_updates(&mut self) {
        self.update_busy = true;
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

    /// What the settings and shortcuts panels ask for.
    pub(super) fn on_settings(&mut self, msg: Msg) {
        match msg {
            Msg::Open { panel: Panel::Settings } => self.settings_open = true,
            Msg::Open { panel: Panel::Shortcuts } => self.shortcuts_open = true,
            Msg::Closed { panel } => {
                match panel {
                    Panel::Settings => self.settings_open = false,
                    Panel::Shortcuts => self.shortcuts_open = false,
                }
                self.return_to_page();
            }
            Msg::SetTheme { choice } => self.act(Action::SetTheme(choice)),
            Msg::SetLanguage { choice } => self.act(Action::SetLanguage(choice)),
            Msg::Zoom { step } => self.act(Action::Zoom(step)),
            Msg::SetSearchEngine { id } => self.act(Action::SetSearchEngine(Engine::from_id(Some(&id)))),
            Msg::SetAutoUpdate { on } => self.act(Action::SetAutoUpdate(on)),
            Msg::SetShowOther { on } => self.act(Action::SetShowOtherFiles(on)),
            Msg::CheckUpdates => self.act(Action::CheckUpdates),
            Msg::Restart => self.act(Action::Restart),
        }
    }

    /// After a panel closes, the keys go back to the page on the focused side.
    fn return_to_page(&mut self) {
        let side = self.tabs.focused();
        self.focus_pane(side, true);
    }
}
