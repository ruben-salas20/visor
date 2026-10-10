use crate::ui::action::{Action, UserEvent};
use crate::ui::bar::Bar;
use crate::ui::icons::{draw_close, draw_list, draw_plus, draw_target, drawn_icon, icon_button};
use crate::ui::tabs::Side;
use crate::ui::theme::Theme;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use tao::event_loop::EventLoopProxy;

/// The widgets of one side of the window: its tab strip, address bar and
/// container of page views. What the tabs are lives in `tabs::Tabs`.
pub struct Pane {
    id: u64,
    root: gtk::Box,
    strip: gtk::Box,
    tab_list: gtk::Box,
    /// The annotation buttons, shown only on vault pages: elsewhere they do nothing.
    annot_tools: gtk::Box,
    bar: Bar,
    views: gtk::Box,
    /// Covers the pages while a file is dragged from the sidebar and says
    /// which zone is under the pointer: one half of the side, or all of it.
    /// It paints nothing; the page draws the mark (see App::frost).
    drop: gtk::EventBox,
    /// Whether a drop takes the whole side (the window is split) or a half.
    whole: Rc<Cell<bool>>,
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
        root.pack_start(bar.widget(), false, false, 0);
        root.pack_start(&overlay, true, true, 0);
        strip.show_all();
        views.show();
        overlay.show();
        root.show();
        Pane { id, root, strip, tab_list, annot_tools, bar, views, drop, whole, proxy: proxy.clone() }
    }

    /// Shows or hides the drop zone. `whole`: a drop takes the side, not a half.
    pub fn show_drop(&self, on: bool, whole: bool) {
        self.whole.set(whole);
        self.drop.set_visible(on);
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn root(&self) -> &gtk::Box {
        &self.root
    }

    /// Where the views of this side's pages go.
    pub fn views(&self) -> &gtk::Box {
        &self.views
    }

    pub fn bar(&self) -> &Bar {
        &self.bar
    }

    /// Fullscreen: only the side with the page shows, and without its chrome.
    pub fn fullscreen(&self, holder: bool) {
        self.strip.hide();
        self.bar.set_visible(false);
        self.root.set_visible(holder);
    }

    pub fn windowed(&self, bar_shown: bool) {
        self.strip.show();
        self.bar.set_visible(bar_shown);
        self.root.show();
    }

    /// Rebuilds the tab strip from `side`. `focused` marks the pane that keys and agents act on.
    pub fn render(&self, side: &Side, focused: bool) {
        for child in self.tab_list.children() {
            self.tab_list.remove(&child);
        }
        let target = side.tab().target();
        let in_vault = !target.is_empty() && !target.contains("://") && !crate::vault::is_loose(target);
        self.annot_tools.set_visible(in_vault);
        let style = self.strip.style_context();
        if focused { style.add_class("focused") } else { style.remove_class("focused") }
        for (i, tab) in side.tabs().iter().enumerate() {
            let item = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            item.style_context().add_class("visor-tab");
            if i == side.active() {
                item.style_context().add_class("active");
            }
            let label = gtk::Label::new(Some(&tab.label()));
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(28);
            let select = gtk::Button::new();
            select.add(&label);
            select.set_can_focus(false);
            select.style_context().add_class("visor-tab-name");
            select.set_tooltip_text(Some(if tab.target().is_empty() { "Nueva pestaña" } else { tab.target() }));
            let (proxy, id) = (self.proxy.clone(), tab.id());
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
