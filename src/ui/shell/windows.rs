//! winit on Windows: the loop and the windows. tao hung its loop with several
//! child webviews (experiment W2); winit did not (docs/adr/0040).

use super::{Handler, Key, Mods, Signal, shortcut};
use crate::ui::action::{self, Proxy, Routed, UserEvent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::Fullscreen;
use webview2_com::AcceleratorKeyPressedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, ICoreWebView2Controller,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use wry::WebContext;

pub type Loop = EventLoop<Routed>;
pub type Target = ActiveEventLoop;
pub type WindowId = winit::window::WindowId;

/// A window of visor, and who hears its shortcuts.
pub struct Window(winit::window::Window, RefCell<Option<(Proxy, Rc<Cell<bool>>)>>);

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

    /// Brings the window to the front. Windows needs no activation token.
    pub fn present(&self, _token: Option<&str>) {
        self.0.focus_window();
    }

    /// A page asked for fullscreen, or left it. WebKitGTK does this itself;
    /// WebView2 only tells, so the window follows here.
    pub fn set_page_fullscreen(&self, on: bool) {
        self.0.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
    }

    pub(in crate::ui) fn winit(&self) -> &winit::window::Window {
        &self.0
    }

    /// Catches the app-wide shortcuts in one of this window's views: the keys
    /// go to the focused view, never to the window.
    pub(in crate::ui) fn catch_shortcuts(&self, controller: &ICoreWebView2Controller) {
        let Some((proxy, in_drawing)) = self.1.borrow().clone() else { return };
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let (mut kind, mut vk) = (COREWEBVIEW2_KEY_EVENT_KIND::default(), 0u32);
            // SAFETY: COM getters on the live event arguments.
            unsafe {
                args.KeyEventKind(&mut kind)?;
                args.VirtualKey(&mut vk)?;
            }
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                return Ok(());
            }
            // SAFETY: GetKeyState only reads the keyboard state of this thread.
            let down = |key: u16| unsafe { GetKeyState(i32::from(key)) } < 0;
            let mods = Mods { ctrl: down(VK_CONTROL.0), alt: down(VK_MENU.0), shift: down(VK_SHIFT.0) };
            let Some(key) = key_of(vk) else { return Ok(()) };
            // AltGr is Ctrl+Alt on Windows: with it, «\», «|» or «@» are typing, not shortcuts.
            if mods.ctrl && mods.alt && !matches!(key, Key::Left | Key::Right) {
                return Ok(());
            }
            if let Some(action) = shortcut(key, mods, in_drawing.get()) {
                // SAFETY: a COM setter on the live event arguments.
                unsafe { args.SetHandled(true)? };
                proxy.send(UserEvent::Action(action));
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: registers a handler on a live controller; it lives as long as the view.
        let _ = unsafe { controller.add_AcceleratorKeyPressed(&handler, &mut token) };
    }
}

/// The shortcuts' name for a virtual key, if they use it.
fn key_of(vk: u32) -> Option<Key> {
    Some(match vk {
        0x41..=0x5A => Key::Char(char::from(vk as u8).to_ascii_lowercase()),
        0x30..=0x39 => Key::Digit((vk - 0x30) as u8),
        // The keypad's digits.
        0x60..=0x69 => Key::Digit((vk - 0x60) as u8),
        // VK_OEM_PLUS (the «=» and «+» key) and the keypad's «+».
        0xBB | 0x6B => Key::Plus,
        0xBD | 0x6D => Key::Minus,
        0xBC => Key::Char(','),
        // VK_OEM_5: «\» on US keyboards, «º» on Spanish ones.
        0xDC => Key::Char('\\'),
        0x09 => Key::Tab,
        0x25 => Key::Left,
        0x27 => Key::Right,
        0x74 => Key::F5,
        _ => return None,
    })
}

pub fn event_loop() -> (Loop, action::Send) {
    let event_loop = EventLoop::<Routed>::with_user_event().build().expect("bucle de eventos");
    let proxy = event_loop.create_proxy();
    let send: action::Send = Arc::new(move |routed| {
        let _ = proxy.send_event(routed);
    });
    (event_loop, send)
}

struct Driver<H> {
    handler: H,
    started: bool,
}

impl<H: Handler> ApplicationHandler<Routed> for Driver<H> {
    // winit makes windows only once the loop runs, so the first windows open here.
    fn resumed(&mut self, target: &ActiveEventLoop) {
        if !std::mem::replace(&mut self.started, true) {
            self.handler.start(target);
        }
    }

    fn window_event(&mut self, target: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let signal = match event {
            WindowEvent::CloseRequested => Signal::Close,
            WindowEvent::Focused(true) => Signal::Focused,
            WindowEvent::Resized(_) => Signal::Resized,
            _ => return,
        };
        if self.handler.window(id, signal) {
            target.exit();
        }
    }

    fn user_event(&mut self, target: &ActiveEventLoop, routed: Routed) {
        if self.handler.user(target, routed) {
            target.exit();
        }
    }
}

/// Runs the loop until a handler asks to end, then ends the process, as tao
/// does on Linux: everything was saved before, and the webviews need no teardown.
pub fn run(event_loop: Loop, handler: impl Handler + 'static) -> i32 {
    let mut driver = Driver { handler, started: false };
    let code = match event_loop.run_app(&mut driver) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    };
    std::process::exit(code);
}

pub fn create_window(target: &Target, title: &str, (width, height): (f64, f64)) -> Window {
    let attributes = winit::window::Window::default_attributes().with_title(title).with_inner_size(LogicalSize::new(width, height));
    Window(target.create_window(attributes).expect("ventana"), RefCell::new(None))
}

/// Calls `f` after `delay`, from a thread of its own: winit has no timers on
/// the loop. Every caller only sends an event to the loop.
pub fn after(delay: Duration, f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        f();
    });
}

/// One web environment for every view of every window, on one folder of
/// visor's own: WebView2's default is a folder next to the program, which an
/// installed program may not write.
pub fn web_context() -> WebContext {
    WebContext::new(dirs::data_local_dir().map(|d| d.join("visor").join("WebView2")))
}

/// Nothing to prepare before the first window.
pub fn prepare() {}

/// The app-wide shortcuts (`shell::shortcut`). The keys go to the focused
/// webview, not the window, so each view built later catches them: see
/// `Window::catch_shortcuts`.
pub fn connect_shortcuts(window: &Window, proxy: Proxy, in_drawing: Rc<Cell<bool>>) {
    window.1.replace(Some((proxy, in_drawing)));
}
