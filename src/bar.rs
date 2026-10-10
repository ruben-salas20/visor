use crate::app::{Action, UserEvent};
use crate::icons::{draw_back, draw_forward, draw_reload, icon_button};
use crate::theme::Theme;
use gtk::prelude::*;
use tao::event_loop::EventLoopProxy;

/// Native toolbar above a page: back, forward, reload and the address field.
pub struct Bar {
    pub root: gtk::Box,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub address: gtk::Entry,
}

impl Bar {
    /// `pane` is the id of the side it belongs to: its controls act there.
    pub fn new(proxy: &EventLoopProxy<UserEvent>, pane: u64) -> Bar {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        root.style_context().add_class("visor-bar");

        let button = |draw: fn(&gtk::cairo::Context), tooltip: &str, action: fn() -> Action| {
            let b = icon_button(tooltip, draw);
            b.set_can_focus(false);
            let proxy = proxy.clone();
            b.connect_clicked(move |_| {
                let _ = proxy.send_event(UserEvent::Action(Action::FocusPane(pane)));
                let _ = proxy.send_event(UserEvent::Action(action()));
            });
            root.pack_start(&b, false, false, 0);
            b
        };
        let back = button(draw_back, "Atrás (Alt+←)", || Action::Back);
        let forward = button(draw_forward, "Adelante (Alt+→)", || Action::Forward);
        button(draw_reload, "Recargar (Ctrl+R)", || Action::Reload);

        let address = gtk::Entry::new();
        address.set_hexpand(true);
        address.set_placeholder_text(Some("Ruta de la bóveda o dirección web (Ctrl+L)"));
        let p = proxy.clone();
        address.connect_activate(move |entry| {
            let _ = p.send_event(UserEvent::Action(Action::Go(entry.text().to_string())));
        });
        let p = proxy.clone();
        address.connect_key_press_event(move |_, event| {
            if event.keyval() == gtk::gdk::keys::constants::Escape {
                let _ = p.send_event(UserEvent::Action(Action::LeaveAddress));
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        let p = proxy.clone();
        address.connect_focus_in_event(move |_, _| {
            let _ = p.send_event(UserEvent::Action(Action::FocusPane(pane)));
            gtk::glib::Propagation::Proceed
        });
        address.connect_changed(|entry| entry.style_context().remove_class("error"));
        let p = proxy.clone();
        address.connect_focus_out_event(move |_, _| {
            let _ = p.send_event(UserEvent::Action(Action::AddressDone));
            gtk::glib::Propagation::Proceed
        });
        root.pack_start(&address, true, true, 4);
        root.show_all();

        Bar { root, back, forward, address }
    }

    /// Shows `text` unless the person is typing in the field.
    pub fn show(&self, text: &str) {
        if !self.address.has_focus() {
            self.address.set_text(text);
        }
    }

    pub fn css(theme: &Theme) -> String {
        format!(
            ".visor-bar {{ background: {bg}; border-bottom: 1px solid {line}; padding: 5px 8px; }}
.visor-bar button {{ background: none; border: none; box-shadow: none; color: {fg};
  min-height: 26px; min-width: 26px; padding: 2px; border-radius: 6px; }}
.visor-bar button:hover {{ background: {hover}; }}
.visor-bar button:disabled {{ color: {muted}; opacity: .5; }}
.visor-bar entry {{ background: {side}; color: {fg}; border: 1px solid {line}; border-radius: 6px;
  box-shadow: none; min-height: 26px; padding: 0 10px; caret-color: {fg}; }}
.visor-bar entry:focus {{ border-color: {accent}; }}
.visor-bar entry.error {{ border-color: {red}; }}
.visor-bar entry selection {{ background: {selection}; color: {fg}; }}
.visor-bar entry image {{ color: {muted}; }}",
            bg = theme.bg,
            fg = theme.fg,
            muted = theme.muted,
            accent = theme.accent,
            selection = theme.accent_tone(30.0),
            red = theme.palette.iter().find(|(n, _)| *n == "red").map_or("#d33", |(_, c)| c.as_str()),
            line = theme.tone(14.0),
            hover = theme.tone(9.0),
            side = theme.tone(4.0),
        )
    }
}
