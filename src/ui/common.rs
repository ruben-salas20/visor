//! What every window shares: the settings, the theme, the one web context and
//! the app-wide CSS. A window reaches it through `CommonRef`. How to borrow it
//! safely is described in `ui/visor.rs`.

use crate::ui::theme::Theme;
use crate::vault::config::Config;
use std::cell::RefCell;
use std::rc::Rc;
use wry::WebContext;

pub type CommonRef = Rc<RefCell<Common>>;

pub struct Common {
    pub config: Config,
    /// Changes in `config` not yet written.
    config_dirty: bool,
    /// Whether a write is already scheduled, so a burst of changes makes one write.
    save_scheduled: bool,
    pub theme: Option<Rc<Theme>>,
    /// The monospace font for the theme. It is found in a thread, after the window opens.
    pub mono: Option<String>,
    pub background: (u8, u8, u8, u8),
    pub context: WebContext,
    pub styles: gtk::CssProvider,
}

impl Common {
    pub fn new(config: Config, theme: Option<Theme>, context: WebContext, styles: gtk::CssProvider) -> Self {
        let background = theme.as_ref().map_or((255, 255, 255, 255), Theme::rgba);
        Common {
            config,
            config_dirty: false,
            save_scheduled: false,
            theme: theme.map(Rc::new),
            mono: None,
            background,
            context,
            styles,
        }
    }

    /// Marks the settings as changed. True if the caller must schedule the
    /// write; it is scheduled once, until `config_write_due` runs.
    pub fn mark_config_dirty(&mut self) -> bool {
        self.config_dirty = true;
        !std::mem::replace(&mut self.save_scheduled, true)
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
