//! The whole process: the agent socket, the local web server, the one event
//! loop and the state every window shares (`Common`). Each window has an `App`
//! (see `app.rs`); the loop hands each event to the `App` named by `Routed::window`.
//!
//! The `RefCell` of `Common` follows one rule. A borrow must not span a call to
//! an `App` method: that method may borrow `common` again, and the second
//! borrow panics with "already borrowed". A borrow may span a GTK call only when
//! the callee cannot reach `common`, as in `App::create_view`. When in doubt,
//! copy the value into a local and end the borrow before the call.

use crate::agent::socket::{ClaimError, claim};
use crate::i18n::tr;
use crate::paths;
use crate::session::{Shared, SharedRef};
use crate::ui::action::{Proxy, Routed, UserEvent};
use crate::ui::app::App;
use crate::ui::common::{Common, CommonRef};
use crate::ui::theme::{self, Theme};
use crate::ui::watch::watch;
use crate::ui::windows::{WinId, Windows};
use crate::vault;
use crate::vault::config::Config;
use gtk::prelude::*;
use notify::RecursiveMode;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::unix::WindowExtUnix;
use tao::window::{Window, WindowBuilder};
use wry::WebContext;

pub fn launch() -> i32 {
    let (listener, claim) = match claim(&paths::socket()) {
        Ok(claimed) => claimed,
        Err(ClaimError::Running) => {
            eprintln!("{}", tr("visor ya está abierto.", "visor is already open."));
            return 0;
        }
        Err(ClaimError::Failed(e)) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut claim = Some(claim);

    // One window for now. The socket server already addresses windows by id.
    let win = Windows::new().open();
    let event_loop = EventLoopBuilder::<Routed>::with_user_event().build();
    let proxy = Proxy::new(win, event_loop.create_proxy());
    let mut state = Shared::with_saved_events();
    state.focused_window = Some(win);
    state.window_mut(win);
    let shared: SharedRef = Arc::new(Mutex::new(state));
    let agent_proxy = proxy.clone();
    crate::agent::server::spawn(listener, shared.clone(), move |window, request| {
        agent_proxy.to(window).send(UserEvent::Agent(request));
    });

    let mut config = Config::load();
    let Some(crate::http::Started { port, base, loose_base }) = crate::http::start(config.port, shared.clone()) else {
        eprintln!("{}", tr("No se pudo iniciar el servidor local.", "Could not start the local server."));
        return 1;
    };
    vault::set_base(base);
    vault::set_loose_base(loose_base);
    if config.port != Some(port) {
        config.port = Some(port);
        config.save();
    }
    let theme = Theme::load(None, config.theme.as_deref());
    let p = proxy.clone();
    std::thread::spawn(move || {
        p.send(UserEvent::MonoFont(theme::monospace_font()));
    });
    let window = WindowBuilder::new()
        .with_title("Visor")
        .with_inner_size(LogicalSize::new(1200.0, 820.0))
        .build(&event_loop)
        .expect("ventana");
    // One context for every view: one network process, and a single writer
    // for cookies and storage, which two contexts on one folder would fight over.
    // WebKitGTK's defaults are decisions here, measured, so this code sets
    // neither: DMA-BUF stays on (off starts ~0.5 s sooner, but Three.js drops
    // from 60 to 26 fps), and the cache stays WebBrowser-sized without a limit
    // (DocumentViewer frees ~700 MB after heavy sites, but CDN libraries then
    // reload slower and fail offline; the disk cache does not slow startup).
    let context = WebContext::new(None);
    let styles = gtk::CssProvider::new();
    if let Some(screen) = WidgetExt::screen(window.gtk_window()) {
        gtk::StyleContext::add_provider_for_screen(&screen, &styles, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    // Started for a page outside every vault: that page alone, without the
    // last vault and its tabs. See cli::start.
    let loose = std::env::var_os("VISOR_LOOSE").is_some();
    let _theme_watcher = theme::dir().and_then(|d| watch(&d, RecursiveMode::Recursive, 300, proxy.clone(), |_| UserEvent::ThemeChanged));
    let common: CommonRef = Rc::new(RefCell::new(Common::new(config, theme, context, styles)));

    let mut app = App::new(&window, proxy, win, shared, common.clone(), loose);
    app.apply_theme();
    // Once the window and the first page exist, so the check never delays them.
    let auto_update = common.borrow().config.auto_update != Some(false);
    if auto_update {
        crate::update::spawn();
    }
    // Lets the system offer visor for .md and .html; a write only when the binary moved.
    if let (Some(file), Some(icon), Ok(exe)) = (paths::desktop_entry(), paths::desktop_icon(), std::env::current_exe()) {
        std::thread::spawn(move || {
            let _ = crate::desktop::write_if_changed(&icon, crate::desktop::ICON);
            let _ = crate::desktop::refresh(&exe, &file);
        });
    }
    // Load the page now instead of after the sidebar: both webviews start in parallel.
    let current = common.borrow().config.current.clone().filter(|p| !loose && p.is_dir());
    app.show_vault(current);
    app.draw_strips();

    let mut apps: HashMap<WinId, (Window, App)> = HashMap::new();
    apps.insert(win, (window, app));
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                for (_, app) in apps.values_mut() {
                    app.save_layout();
                }
                common.borrow_mut().flush_config();
                if let Some(claim) = claim.take() {
                    claim.release();
                }
                *flow = ControlFlow::Exit;
            }
            Event::UserEvent(Routed { window, event }) => {
                debug_assert!(apps.contains_key(&window), "an event reached a window that is not open");
                if let Some((_, app)) = apps.get_mut(&window) {
                    app.handle(event);
                }
            }
            _ => {}
        }
    })
}
