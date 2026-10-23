//! tao over GTK: the loop, the windows, the shortcuts caught on the GTK window.

use super::{Handler, Key, Mods, Signal, shortcut};
use crate::ui::action::{self, Proxy, Routed, UserEvent};
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopWindowTarget};
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use wry::WebContext;

pub type Loop = EventLoop<Routed>;
pub type Target = EventLoopWindowTarget<Routed>;
pub type WindowId = tao::window::WindowId;

/// A window of visor.
pub struct Window(tao::window::Window);

impl Window {
    pub fn id(&self) -> WindowId {
        self.0.id()
    }

    pub fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }

    /// The inner size, in logical pixels.
    pub fn logical_size(&self) -> (f64, f64) {
        let size = self.0.inner_size().to_logical::<f64>(self.0.scale_factor());
        (size.width, size.height)
    }

    /// Asks the compositor for the focus. `token` is the activation token of
    /// the launch that asked for this window, without which Wayland may refuse.
    pub fn present(&self, token: Option<&str>) {
        if let Some(token) = token {
            self.0.gtk_window().set_startup_id(token);
        }
        self.0.set_focus();
    }

    /// A page asked for fullscreen, or left it. WebKitGTK makes the window
    /// fullscreen itself.
    pub fn set_page_fullscreen(&self, _on: bool) {}

    pub(in crate::ui) fn tao(&self) -> &tao::window::Window {
        &self.0
    }
}

pub fn event_loop() -> (Loop, action::Send) {
    let event_loop = EventLoopBuilder::<Routed>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let send: action::Send = Arc::new(move |routed| {
        let _ = proxy.send_event(routed);
    });
    (event_loop, send)
}

/// Runs the loop until a handler asks to end. tao ends the process itself.
pub fn run(event_loop: Loop, mut handler: impl Handler + 'static) -> i32 {
    handler.start(&event_loop);
    event_loop.run(move |event, target, flow| {
        *flow = ControlFlow::Wait;
        let exit = match event {
            Event::WindowEvent { window_id, event, .. } => match event {
                WindowEvent::CloseRequested => handler.window(window_id, Signal::Close),
                WindowEvent::Focused(true) => handler.window(window_id, Signal::Focused),
                WindowEvent::Resized(_) => handler.window(window_id, Signal::Resized),
                _ => false,
            },
            Event::UserEvent(routed) => handler.user(target, routed),
            _ => false,
        };
        if exit {
            *flow = ControlFlow::Exit;
        }
    })
}

pub fn create_window(target: &Target, title: &str, (width, height): (f64, f64)) -> Window {
    let window = WindowBuilder::new().with_title(title).with_inner_size(LogicalSize::new(width, height)).build(target).expect("ventana");
    Window(window)
}

/// Calls `f` on the main loop after `delay`.
pub fn after(delay: Duration, f: impl FnOnce() + Send + 'static) {
    gtk::glib::timeout_add_local_once(delay, f);
}

/// One context for every view of every window: one network process, and a
/// single writer for cookies and storage, which two contexts on one folder
/// would fight over.
/// WebKitGTK's defaults are decisions here, measured, so this code sets
/// neither: DMA-BUF stays on (off starts ~0.5 s sooner, but Three.js drops
/// from 60 to 26 fps), and the cache stays WebBrowser-sized without a limit
/// (DocumentViewer frees ~700 MB after heavy sites, but CDN libraries then
/// reload slower and fail offline; the disk cache does not slow startup).
pub fn web_context() -> WebContext {
    WebContext::new(None)
}

/// Once for the screen, so every window takes the same styles. GTK only
/// draws the window now: the frame and the pages cover it.
pub fn prepare() {
    let styles = gtk::CssProvider::new();
    // GTK3 repaints the window background under the pages on every frame of
    // an animation; they cover it, so it would cost ~2% CPU for nothing.
    let _ = styles.load_from_data(b"window.background { background: none; }");
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(&screen, &styles, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

/// The app-wide shortcuts (`shell::shortcut`), caught on the GTK window before
/// the focused webview sees the key.
pub fn connect_shortcuts(window: &Window, proxy: Proxy, in_drawing: Rc<Cell<bool>>) {
    use gtk::gdk::ModifierType;
    window.0.gtk_window().connect_key_press_event(move |_, event| {
        let state = event.state();
        let mut mods = Mods {
            ctrl: state.contains(ModifierType::CONTROL_MASK),
            alt: state.contains(ModifierType::MOD1_MASK),
            shift: state.contains(ModifierType::SHIFT_MASK),
        };
        let Some(key) = key_of(event.keyval().to_lower(), &mut mods) else { return gtk::glib::Propagation::Proceed };
        let Some(action) = shortcut(key, mods, in_drawing.get()) else { return gtk::glib::Propagation::Proceed };
        proxy.send(UserEvent::Action(action));
        gtk::glib::Propagation::Stop
    });
}

/// The shortcuts' name for a GDK key, if they use it.
fn key_of(k: gtk::gdk::keys::Key, mods: &mut Mods) -> Option<Key> {
    use gtk::gdk::keys::constants as key;
    let digits = [
        (key::_0, key::KP_0),
        (key::_1, key::KP_1),
        (key::_2, key::KP_2),
        (key::_3, key::KP_3),
        (key::_4, key::KP_4),
        (key::_5, key::KP_5),
        (key::_6, key::KP_6),
        (key::_7, key::KP_7),
        (key::_8, key::KP_8),
        (key::_9, key::KP_9),
    ];
    if let Some(n) = digits.iter().position(|(top, pad)| k == *top || k == *pad) {
        return Some(Key::Digit(n as u8));
    }
    Some(match k {
        key::plus | key::equal | key::KP_Add => Key::Plus,
        key::minus | key::KP_Subtract => Key::Minus,
        key::Tab => Key::Tab,
        // Shift+Tab, which GTK names on its own.
        key::ISO_Left_Tab => {
            mods.shift = true;
            Key::Tab
        }
        key::Left => Key::Left,
        key::Right => Key::Right,
        key::F5 => Key::F5,
        key::comma => Key::Char(','),
        // Also «|»: on Spanish layouts «\» needs AltGr and «|» does not.
        key::backslash | key::bar => Key::Char('\\'),
        _ => {
            let c = k.to_unicode().filter(char::is_ascii_lowercase)?;
            Key::Char(c)
        }
    })
}
