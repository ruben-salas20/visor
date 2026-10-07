use crate::app::{Action, UserEvent};
use crate::bar::Bar;
use crate::theme::Theme;
use gtk::prelude::*;
use tao::event_loop::EventLoopProxy;
use wry::{WebView, WebViewExtUnix};

/// A document open in a pane. Only a few tabs keep a live view, because each
/// one costs a WebKit process; the rest load `target` again when shown.
pub struct Tab {
    pub id: u64,
    /// Vault-relative path, web address, or empty for the start page.
    pub target: String,
    /// The page title, for websites once they load.
    pub title: Option<String>,
    /// The live view and its id, which tags its messages.
    pub view: Option<(u64, WebView)>,
    /// When the tab was last shown, to unload the oldest first.
    pub used: u64,
    /// Reading position, as `{y}` or `{line, y}`, taken when the tab was last hidden.
    pub position: Option<String>,
    /// Position to put back once the reloaded page finishes loading.
    pub restore: Option<String>,
    /// Section to scroll to on the first load, from a link with `#`.
    pub fragment: String,
}

impl Tab {
    pub fn new(id: u64, target: String) -> Tab {
        Tab { id, target, title: None, view: None, used: 0, position: None, restore: None, fragment: String::new() }
    }

    pub fn view_id(&self) -> Option<u64> {
        self.view.as_ref().map(|(id, _)| *id)
    }

    fn label(&self) -> String {
        if let Some(title) = self.title.as_ref().filter(|t| !t.trim().is_empty()) {
            return title.clone();
        }
        if self.target.is_empty() {
            return "Nueva pestaña".into();
        }
        if let Some(rest) = self.target.split_once("://").map(|(_, r)| r) {
            return rest.split('/').next().unwrap_or(rest).to_string();
        }
        self.target.rsplit('/').next().unwrap_or(&self.target).to_string()
    }
}

/// One side of the window: its tab strip, address bar and page views.
pub struct Pane {
    pub id: u64,
    pub root: gtk::Box,
    pub strip: gtk::Box,
    tab_list: gtk::Box,
    pub bar: Bar,
    pub views: gtk::Box,
    pub tabs: Vec<Tab>,
    pub active: usize,
    proxy: EventLoopProxy<UserEvent>,
}

impl Pane {
    pub fn new(id: u64, proxy: &EventLoopProxy<UserEvent>) -> Pane {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        strip.style_context().add_class("visor-bar");
        strip.style_context().add_class("visor-strip");
        let tab_list = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        strip.pack_start(&tab_list, true, true, 0);
        for (icon, tip, action) in [
            ("find-location-symbolic", "Anotar un elemento (Ctrl+Shift+A)", (|| Action::PickElement) as fn() -> Action),
            ("view-list-symbolic", "Anotaciones de la página (Ctrl+Shift+L)", || Action::ToggleAnnotations),
        ]
        .into_iter()
        .rev()
        {
            let b = gtk::Button::from_icon_name(Some(icon), gtk::IconSize::Button);
            b.set_tooltip_text(Some(tip));
            b.set_can_focus(false);
            b.set_valign(gtk::Align::Center);
            let proxy = proxy.clone();
            b.connect_clicked(move |_| {
                let _ = proxy.send_event(UserEvent::Action(Action::FocusPane(id)));
                let _ = proxy.send_event(UserEvent::Action(action()));
            });
            strip.pack_end(&b, false, false, 0);
        }
        let bar = Bar::new(proxy, id);
        let views = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.pack_start(&strip, false, false, 0);
        root.pack_start(&bar.root, false, false, 0);
        root.pack_start(&views, true, true, 0);
        strip.show_all();
        views.show();
        root.show();
        Pane { id, root, strip, tab_list, bar, views, tabs: vec![], active: 0, proxy: proxy.clone() }
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// Rebuilds the tab strip. `focused` marks the pane that keys and agents act on.
    pub fn render(&self, focused: bool) {
        for child in self.tab_list.children() {
            self.tab_list.remove(&child);
        }
        let style = self.strip.style_context();
        if focused { style.add_class("focused") } else { style.remove_class("focused") }
        for (i, tab) in self.tabs.iter().enumerate() {
            let item = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            item.style_context().add_class("visor-tab");
            if i == self.active {
                item.style_context().add_class("active");
            }
            let label = gtk::Label::new(Some(&tab.label()));
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(28);
            let select = gtk::Button::new();
            select.add(&label);
            select.set_can_focus(false);
            select.set_tooltip_text(Some(if tab.target.is_empty() { "Nueva pestaña" } else { &tab.target }));
            let (proxy, id) = (self.proxy.clone(), tab.id);
            select.connect_clicked(move |_| {
                let _ = proxy.send_event(UserEvent::Action(Action::SelectTab(id)));
            });
            let proxy = self.proxy.clone();
            select.connect_button_press_event(move |_, event| {
                if event.button() != 2 {
                    return gtk::glib::Propagation::Proceed;
                }
                let _ = proxy.send_event(UserEvent::Action(Action::CloseTab(Some(id))));
                gtk::glib::Propagation::Stop
            });
            let close = gtk::Button::from_icon_name(Some("window-close-symbolic"), gtk::IconSize::Menu);
            close.set_can_focus(false);
            close.set_tooltip_text(Some("Cerrar pestaña (Ctrl+W)"));
            let proxy = self.proxy.clone();
            close.connect_clicked(move |_| {
                let _ = proxy.send_event(UserEvent::Action(Action::CloseTab(Some(id))));
            });
            item.pack_start(&select, true, true, 0);
            item.pack_start(&close, false, false, 0);
            self.tab_list.pack_start(&item, false, true, 0);
        }
        let add = gtk::Button::from_icon_name(Some("list-add-symbolic"), gtk::IconSize::Menu);
        add.set_can_focus(false);
        add.set_valign(gtk::Align::Center);
        add.set_tooltip_text(Some("Nueva pestaña (Ctrl+T)"));
        let (proxy, pane) = (self.proxy.clone(), self.id);
        add.connect_clicked(move |_| {
            let _ = proxy.send_event(UserEvent::Action(Action::FocusPane(pane)));
            let _ = proxy.send_event(UserEvent::Action(Action::NewTab));
        });
        self.tab_list.pack_start(&add, false, false, 0);
        self.tab_list.show_all();
    }

    /// Shows the active tab's view and hides the others, which stay loaded.
    pub fn show_active(&self) {
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some((_, view)) = &tab.view {
                view.webview().set_visible(i == self.active);
            }
        }
    }

    pub fn css(theme: &Theme) -> String {
        format!(
            ".visor-strip {{ padding: 4px 6px; }}
.visor-tab {{ border-radius: 6px; padding: 0 2px; }}
.visor-tab button {{ min-height: 24px; color: {muted}; }}
.visor-tab button:hover {{ background: none; color: {fg}; }}
.visor-tab.active {{ background: {active}; }}
.visor-strip.focused .visor-tab.active {{ box-shadow: inset 0 -2px {accent}; }}
.visor-tab.active button {{ color: {fg}; }}",
            fg = theme.fg,
            muted = theme.muted,
            accent = theme.accent,
            active = theme.tone(9.0),
        )
    }
}
