//! What every window shares: the settings, the theme and the one web context. A window reaches it through `CommonRef`. How to borrow it
//! safely is described in `ui/visor.rs`.

use crate::i18n::tr;
use crate::session::SharedRef;
use crate::ui::action::{Proxy, UserEvent};
use crate::ui::theme::Theme;
use crate::vault::Vault;
use crate::vault::config::Config;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use wry::WebContext;

/// How long after a change the settings are written: a burst of changes makes one write.
const CONFIG_SAVE_DELAY: Duration = Duration::from_millis(500);

pub type CommonRef = Rc<RefCell<Common>>;

pub struct Common {
    pub config: Config,
    /// Changes in `config` not yet written.
    config_dirty: bool,
    /// Whether a write is already scheduled, so a burst of changes makes one write.
    save_scheduled: bool,
    /// Sends the scheduled write to the process, which outlives every window.
    saver: Proxy,
    pub theme: Option<Rc<Theme>>,
    /// The monospace font for the theme. It is found in a thread, after the window opens.
    pub mono: Option<String>,
    pub background: (u8, u8, u8, u8),
    pub context: WebContext,
    /// The port of every origin of the local server.
    port: u16,
    /// Where the frame is served: its origin and secret prefix.
    pub frame_base: String,
    /// Every vault opened since the start, each with its own origin, by the
    /// path it was opened with. Two windows on one vault share it.
    vaults: HashMap<PathBuf, Arc<Vault>>,
}

impl Common {
    /// `saver` sends to the process (window 0).
    pub fn new(config: Config, theme: Option<Theme>, context: WebContext, port: u16, frame_base: String, saver: Proxy) -> Self {
        let background = theme.as_ref().map_or((255, 255, 255, 255), Theme::rgba);
        Common {
            config,
            config_dirty: false,
            save_scheduled: false,
            saver,
            theme: theme.map(Rc::new),
            mono: None,
            background,
            context,
            port,
            frame_base,
            vaults: HashMap::new(),
        }
    }

    /// The vault at `root`, served on its origin. The first time, it gets the
    /// address the settings keep for it, or a new one.
    pub fn vault(&mut self, root: &Path, shared: &SharedRef) -> Result<Arc<Vault>, String> {
        if let Some(vault) = self.vaults.get(root) {
            return Ok(vault.clone());
        }
        let known = self.config.origins.len();
        let host = self.config.origin_for(root).ok_or(tr("Hay demasiadas bóvedas para darles una dirección a cada una.", "There are too many vaults to give each one an address."))?;
        if self.config.origins.len() != known {
            self.config_dirty = true;
        }
        let vault = crate::http::serve_vault(root, host, self.port, shared.clone())
            .ok_or(tr("No se pudo servir la bóveda.", "Could not serve the vault."))?;
        self.vaults.insert(root.to_path_buf(), vault.clone());
        Ok(vault)
    }

    /// Marks the settings as changed and schedules one write shortly after,
    /// instead of one per change: the write blocks the windows. A crash loses
    /// at most this delay of state.
    pub fn mark_config_dirty(&mut self) {
        self.config_dirty = true;
        // Scheduled here, where it is marked, so nothing marks it without a write coming.
        if !std::mem::replace(&mut self.save_scheduled, true) {
            let saver = self.saver.clone();
            gtk::glib::timeout_add_local_once(CONFIG_SAVE_DELAY, move || saver.send(UserEvent::SaveConfig));
        }
    }

    /// Called when the scheduled write runs. Changes made later schedule another.
    pub fn config_write_due(&mut self) {
        self.save_scheduled = false;
    }

    /// Writes the settings now if a change is waiting.
    pub fn flush_config(&mut self) {
        if std::mem::take(&mut self.config_dirty) {
            self.config.save();
        }
    }

    /// Loads the theme again from the settings and the monospace font.
    pub fn reload_theme(&mut self) {
        let theme = Theme::load(self.mono.clone(), self.config.theme.as_deref()).map(Rc::new);
        if let Some(theme) = &theme {
            self.background = theme.rgba();
        }
        self.theme = theme;
    }
}

/// The settings every window shows, to tell what an event in one window
/// changed and show it in the others.
#[derive(Clone, PartialEq)]
pub struct Settings {
    theme: Option<String>,
    zoom: Option<f64>,
    bar: bool,
    show_other: Option<bool>,
    auto_update: Option<bool>,
    search_engine: Option<String>,
    language: Option<String>,
    vaults: Vec<PathBuf>,
}

/// What changed between two [`Settings`].
#[derive(Default, Debug, PartialEq)]
pub struct SettingsChange {
    pub theme: bool,
    pub zoom: bool,
    pub bar: bool,
    pub show_other: bool,
    pub vaults: bool,
    /// Any other setting, shown only in the settings menu.
    pub menu: bool,
}

impl Settings {
    pub fn of(config: &Config) -> Self {
        Settings {
            theme: config.theme.clone(),
            zoom: config.zoom,
            bar: config.bar_shown,
            show_other: config.show_other_files,
            auto_update: config.auto_update,
            search_engine: config.search_engine.clone(),
            language: config.language.clone(),
            vaults: config.vaults.clone(),
        }
    }

    /// `None` when nothing changed.
    pub fn change(&self, after: &Settings) -> Option<SettingsChange> {
        (self != after).then(|| SettingsChange {
            theme: self.theme != after.theme,
            zoom: self.zoom != after.zoom,
            bar: self.bar != after.bar,
            show_other: self.show_other != after.show_other,
            vaults: self.vaults != after.vaults,
            menu: self.auto_update != after.auto_update || self.search_engine != after.search_engine || self.language != after.language,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_says_what_the_other_windows_must_show() {
        let mut config = Config::default();
        let before = Settings::of(&config);
        assert_eq!(before.change(&Settings::of(&config)), None);
        config.zoom = Some(1.25);
        config.vaults.push(PathBuf::from("/b"));
        let change = before.change(&Settings::of(&config)).unwrap();
        assert_eq!(change, SettingsChange { zoom: true, vaults: true, ..Default::default() });
        config.search_engine = Some("ddg".into());
        assert!(before.change(&Settings::of(&config)).unwrap().menu);
    }
}
