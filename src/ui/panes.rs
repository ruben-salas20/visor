use crate::ui::app::{Action, UserEvent};
use crate::ui::bar::Bar;
use crate::ui::icons::{draw_close, draw_list, draw_plus, draw_target, drawn_icon, icon_button};
use crate::ui::theme::Theme;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
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
    /// Whether the view has started showing its page. Until then it stays hidden.
    pub ready: bool,
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
        Tab { id, target, title: None, view: None, ready: false, used: 0, position: None, restore: None, fragment: String::new() }
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
    /// The annotation buttons, shown only on vault pages: elsewhere they do nothing.
    annot_tools: gtk::Box,
    pub bar: Bar,
    pub views: gtk::Box,
    /// Covers the pages while a file is dragged from the sidebar and says
    /// which zone is under the pointer: one half of the side, or all of it.
    /// It paints nothing; the page draws the mark (see App::frost).
    drop: gtk::EventBox,
    /// Whether a drop takes the whole side (the window is split) or a half.
    whole: Rc<Cell<bool>>,
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
        let annot_tools = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        strip.pack_end(&annot_tools, false, false, 0);
        for (draw, tip, action) in [
            (draw_target as fn(&gtk::cairo::Context), "Anotar un elemento (Ctrl+Shift+A)", (|| Action::PickElement) as fn() -> Action),
            (draw_list, "Anotaciones de la página (Ctrl+Shift+L)", || Action::ToggleAnnotations),
        ]
        .into_iter()
        .rev()
        {
            let b = icon_button(tip, draw);
            b.set_can_focus(false);
            b.set_valign(gtk::Align::Center);
            let proxy = proxy.clone();
            b.connect_clicked(move |_| {
                let _ = proxy.send_event(UserEvent::Action(Action::FocusPane(id)));
                let _ = proxy.send_event(UserEvent::Action(action()));
            });
            annot_tools.pack_end(&b, false, false, 0);
        }
        let bar = Bar::new(proxy, id);
        let views = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let overlay = gtk::Overlay::new();
        overlay.add(&views);
        let (drop, whole) = drop_zone(id, proxy);
        overlay.add_overlay(&drop);
        root.pack_start(&strip, false, false, 0);
        root.pack_start(&bar.root, false, false, 0);
        root.pack_start(&overlay, true, true, 0);
        strip.show_all();
        views.show();
        overlay.show();
        root.show();
        Pane { id, root, strip, tab_list, annot_tools, bar, views, drop, whole, tabs: vec![], active: 0, proxy: proxy.clone() }
    }

    /// Shows or hides the drop zone. `whole`: a drop takes the side, not a half.
    pub fn show_drop(&self, on: bool, whole: bool) {
        self.whole.set(whole);
        self.drop.set_visible(on);
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// Rebuilds the tab strip. `focused` marks the pane that keys and agents act on.
    pub fn render(&self, focused: bool) {
        for child in self.tab_list.children() {
            self.tab_list.remove(&child);
        }
        let in_vault = self.tabs.get(self.active).is_some_and(|t| {
            !t.target.is_empty() && !t.target.contains("://") && !crate::vault::is_loose(&t.target)
        });
        self.annot_tools.set_visible(in_vault);
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
            select.style_context().add_class("visor-tab-name");
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
            let close = gtk::Button::new();
            close.add(&drawn_icon(12, draw_close));
            close.set_can_focus(false);
            close.style_context().add_class("visor-tab-close");
            close.set_tooltip_text(Some("Cerrar pestaña (Ctrl+W)"));
            let proxy = self.proxy.clone();
            close.connect_clicked(move |_| {
                let _ = proxy.send_event(UserEvent::Action(Action::CloseTab(Some(id))));
            });
            item.pack_start(&select, true, true, 0);
            item.pack_start(&close, false, false, 0);
            self.tab_list.pack_start(&item, false, true, 0);
        }
        let add = icon_button("Nueva pestaña (Ctrl+T)", draw_plus);
        add.set_can_focus(false);
        add.set_valign(gtk::Align::Center);
        let (proxy, pane) = (self.proxy.clone(), self.id);
        add.connect_clicked(move |_| {
            let _ = proxy.send_event(UserEvent::Action(Action::FocusPane(pane)));
            let _ = proxy.send_event(UserEvent::Action(Action::NewTab));
        });
        self.tab_list.pack_start(&add, false, false, 0);
        self.tab_list.show_all();
    }

    /// Shows the active tab's view and hides the others, which stay loaded.
    /// A new view stays hidden until its page starts loading: on screen,
    /// WebKitGTK blocks the app up to 500 ms waiting for its first paint while
    /// the new process waits for the app, twice per new view.
    pub fn show_active(&self) {
        if !self.tab().ready {
            return;
        }
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
.visor-tab button.visor-tab-name {{ padding: 0 6px 0 10px; }}
.visor-tab button.visor-tab-close {{ padding: 0 8px 0 2px; min-width: 0; }}
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

/// The drop zone over a side's pages. It tells the app which zone is under
/// the pointer, only when it changes: 0 left half, 1 right half, 2 the whole
/// side (the window is split), or none once the pointer leaves.
fn drop_zone(pane: u64, proxy: &EventLoopProxy<UserEvent>) -> (gtk::EventBox, Rc<Cell<bool>>) {
    let zone = gtk::EventBox::new();
    zone.set_visible_window(false);
    zone.set_no_show_all(true);
    let whole = Rc::new(Cell::new(false));
    let zone_at = {
        let whole = whole.clone();
        move |zone: &gtk::EventBox, x: i32| if whole.get() { 2 } else { u8::from(x > zone.allocated_width() / 2) }
    };
    let last: Rc<Cell<Option<u8>>> = Rc::new(Cell::new(None));
    let hover = {
        let (proxy, last) = (proxy.clone(), last.clone());
        move |zone: Option<u8>| {
            if last.replace(zone) != zone {
                let _ = proxy.send_event(UserEvent::DropHover { pane, zone });
            }
        }
    };
    // No target list: the drag comes from the sidebar's tree, and
    // the app already knows which file it carries.
    zone.drag_dest_set(gtk::DestDefaults::empty(), &[], gtk::gdk::DragAction::MOVE | gtk::gdk::DragAction::COPY);
    let (h, at) = (hover.clone(), zone_at.clone());
    zone.connect_drag_motion(move |zone, context, x, _, time| {
        h(Some(at(zone, x)));
        context.drag_status(gtk::gdk::DragAction::MOVE, time);
        true
    });
    zone.connect_drag_leave(move |_, _, _| hover(None));
    let proxy = proxy.clone();
    zone.connect_drag_drop(move |zone, context, x, _, time| {
        last.set(None);
        let half = match zone_at(zone, x) {
            2 => None,
            h => Some(h as usize),
        };
        let _ = proxy.send_event(UserEvent::Dropped { pane, half });
        context.drag_finish(true, false, time);
        true
    });
    (zone, whole)
}
