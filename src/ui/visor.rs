//! The whole process: the agent socket, the local web server, the one event
//! loop, the windows and the state they share (`Common`). Each window has an
//! `App` (see `app.rs`); the loop hands each event to the `App` named by
//! `Routed::window`. Window 0 is the process itself.
//!
//! The `RefCell` of `Common` follows one rule. A borrow must not span a call to
//! an `App` method: that method may borrow `common` again, and the second
//! borrow panics with "already borrowed". A borrow may span a GTK call only when
//! the callee cannot reach `common`, as in `App::create_view`. When in doubt,
//! copy the value into a local and end the borrow before the call.

use crate::agent::command::{Command, Request};
use crate::agent::channel::{Claim, ClaimError, claim};
use crate::i18n::tr;
use crate::paths;
use crate::session::{AgentRequest, LOADED, Shared, SharedRef};
use crate::trf;
use crate::ui::action::{self, Proxy, Routed, UserEvent};
use crate::ui::app::App;
use crate::ui::common::{Common, CommonRef, Settings};
use crate::ui::theme::{self, Theme};
use crate::ui::watch::watch;
use crate::ui::windows::{Closed, WinId, Windows};
use crate::vault;
use crate::vault::config::{Config, WindowState, Workspace};
use notify::RecursiveMode;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
use wry::WebContext;

pub fn launch() -> i32 {
    // Started for a page outside every vault: that page alone, without the
    // saved windows. See cli::start.
    let loose = std::env::var_os("VISOR_LOOSE").is_some();
    let (listener, claim) = match claim(&paths::socket()) {
        Ok(claimed) => claimed,
        Err(ClaimError::Running) => return ask_for_window(loose),
        Err(ClaimError::Failed(e)) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let event_loop = EventLoopBuilder::<Routed>::with_user_event().build();
    let loop_proxy = event_loop.create_proxy();
    let send: action::Send = Arc::new(move |routed| {
        let _ = loop_proxy.send_event(routed);
    });
    let process = Proxy::new(0, send.clone());
    let shared: SharedRef = Arc::new(Mutex::new(Shared::with_saved_events()));
    let agent_proxy = process.clone();
    crate::agent::server::spawn(listener, shared.clone(), move |window, request| {
        agent_proxy.to(window).send(UserEvent::Agent(request));
    });

    let mut config = Config::load();
    let Some(crate::http::Started { port, loose_base }) = crate::http::start(config.port, shared.clone()) else {
        eprintln!("{}", tr("No se pudo iniciar el servidor local.", "Could not start the local server."));
        return 1;
    };
    vault::set_loose_base(loose_base);
    if config.port != Some(port) {
        config.port = Some(port);
        config.save();
    }
    let theme = Theme::load(None, config.theme.as_deref());
    let p = process.clone();
    std::thread::spawn(move || {
        p.send(UserEvent::MonoFont(theme::monospace_font()));
    });
    // One context for every view of every window: one network process, and a
    // single writer for cookies and storage, which two contexts on one folder
    // would fight over.
    // WebKitGTK's defaults are decisions here, measured, so this code sets
    // neither: DMA-BUF stays on (off starts ~0.5 s sooner, but Three.js drops
    // from 60 to 26 fps), and the cache stays WebBrowser-sized without a limit
    // (DocumentViewer frees ~700 MB after heavy sites, but CDN libraries then
    // reload slower and fail offline; the disk cache does not slow startup).
    let context = WebContext::new(None);
    // Once for the screen, so every window takes the same styles.
    let styles = gtk::CssProvider::new();
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(&screen, &styles, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    let auto_update = config.auto_update != Some(false);
    let (states, focus) = if loose { (vec![WindowState::default()], 0) } else { config.restore_windows() };
    let common: CommonRef = Rc::new(RefCell::new(Common::new(config, theme, context, styles, port, process.clone())));

    let mut visor = Visor {
        common,
        shared,
        windows: Windows::new(),
        apps: HashMap::new(),
        by_tao: HashMap::new(),
        send,
        claim: Some(claim),
        loose,
    };
    visor.shared.lock().unwrap().restoring = true;
    for state in states {
        let workspace = Some(state.workspace.clone()).filter(|w| !w.panes.is_empty());
        visor.open_window(&event_loop, state, workspace, loose, None);
    }
    if let Some(&w) = visor.windows.ids().get(focus) {
        visor.focus(w);
        visor.apps[&w].present(None);
    }
    visor.shared.lock().unwrap().restoring = false;
    LOADED.notify_all();
    let _theme_watcher = theme::dir().and_then(|d| watch(&d, RecursiveMode::Recursive, 300, process, |_| UserEvent::ThemeChanged));
    // Outside the check: a leftover from an update must go even when updates are off.
    crate::update::remove_leftover();
    // Once the windows and their first pages exist, so the check never delays them.
    if auto_update {
        crate::update::spawn();
    }
    // Lets the system offer visor for .md and .html; a write only when the binary moved.
    // A package brings its own entry.
    if let (Some(file), Some(icon), Ok(exe)) = (paths::desktop_entry(), paths::desktop_icon(), std::env::current_exe())
        && !crate::package::managed(&exe)
    {
        std::thread::spawn(move || {
            let _ = crate::desktop::write_if_changed(&icon, crate::desktop::ICON);
            let _ = crate::desktop::refresh(&exe, &file);
        });
    }

    event_loop.run(move |event, target, flow| {
        let _keep = &_theme_watcher;
        *flow = ControlFlow::Wait;
        let exit = match event {
            Event::WindowEvent { window_id, event, .. } => {
                let Some(&w) = visor.by_tao.get(&window_id) else { return };
                match event {
                    WindowEvent::CloseRequested => visor.close(w),
                    WindowEvent::Focused(true) => {
                        visor.focus(w);
                        false
                    }
                    WindowEvent::Resized(_) => {
                        if let Some(app) = visor.apps.get_mut(&w) {
                            app.resized();
                        }
                        visor.window_changed(w);
                        false
                    }
                    _ => false,
                }
            }
            Event::UserEvent(Routed { window: 0, event }) => visor.process_event(event, target),
            Event::UserEvent(Routed { window, event }) => {
                visor.window_event(window, event);
                false
            }
            _ => false,
        };
        if exit {
            *flow = ControlFlow::Exit;
        }
    })
}

/// A second `visor` while one runs: asks it for another window.
fn ask_for_window(loose: bool) -> i32 {
    // A loose start comes from `cli::start`, which sends its `open` itself.
    if loose {
        return 0;
    }
    let token = std::env::var("XDG_ACTIVATION_TOKEN").or_else(|_| std::env::var("DESKTOP_STARTUP_ID")).ok();
    let answer = crate::agent::cli::call(&Request { command: Command::NewWindow { token }, window: None });
    if answer["ok"] != true {
        // An older visor does not know `new_window`.
        eprintln!("{}", tr("visor ya está abierto.", "visor is already open."));
    }
    0
}

struct Visor {
    common: CommonRef,
    shared: SharedRef,
    windows: Windows,
    apps: HashMap<WinId, App>,
    by_tao: HashMap<tao::window::WindowId, WinId>,
    send: action::Send,
    claim: Option<Claim>,
    /// Started for a loose page; see `save_windows`.
    loose: bool,
}

impl Visor {
    /// Opens a window on `state`'s vault with `workspace`'s tabs, or with the
    /// vault's last tabs when there is none.
    fn open_window(&mut self, target: &EventLoopWindowTarget<Routed>, state: WindowState, workspace: Option<Workspace>, loose: bool, token: Option<&str>) -> WinId {
        let w = self.windows.open();
        {
            let mut shared = self.shared.lock().unwrap();
            shared.window_mut(w);
            shared.focused_window = Some(w);
        }
        let proxy = Proxy::new(w, self.send.clone());
        let mut app = App::new(target, proxy, self.shared.clone(), self.common.clone(), &state, loose);
        app.restyle();
        let vault = state.vault.filter(|p| !loose && p.is_dir());
        app.show_vault_with(vault, workspace);
        app.draw_strips();
        app.present(token);
        self.by_tao.insert(app.tao_id(), w);
        self.apps.insert(w, app);
        w
    }

    /// Another window, on the focused window's vault, with one empty tab.
    fn new_window(&mut self, target: &EventLoopWindowTarget<Routed>, token: Option<&str>) -> WinId {
        let vault = self.windows.focused().and_then(|w| self.apps.get(&w)).and_then(|app| app.window_state().vault);
        let w = self.open_window(target, WindowState { vault, ..Default::default() }, Some(Workspace::default()), false, token);
        self.window_changed(w);
        w
    }

    fn focus(&mut self, w: WinId) {
        if self.windows.focus(w) {
            self.shared.lock().unwrap().focused_window = Some(w);
        }
    }

    /// Closes a window. True when it was the last one and the process must end.
    fn close(&mut self, w: WinId) -> bool {
        if self.windows.ids().len() <= 1 {
            self.quit();
            return true;
        }
        let Some(mut app) = self.apps.remove(&w) else { return false };
        // Its vault keeps these tabs as its last ones; the window itself is forgotten.
        app.save_layout();
        self.by_tao.remove(&app.tao_id());
        let closed = self.windows.close(w);
        {
            let mut shared = self.shared.lock().unwrap();
            shared.windows.remove(&w);
            if let Closed::Other { focus } = closed {
                shared.focused_window = focus;
            }
            // An agent waiting on one of its pages stops waiting.
            LOADED.notify_all();
        }
        drop(app);
        self.save_windows();
        self.common.borrow_mut().flush_config();
        false
    }

    /// Saves every window, to reopen them all, and lets the socket go.
    fn quit(&mut self) {
        for app in self.apps.values_mut() {
            app.save_layout();
        }
        self.save_windows();
        self.common.borrow_mut().flush_config();
        if let Some(claim) = self.claim.take() {
            claim.release();
        }
    }

    /// Puts the open windows in the settings, in opening order.
    fn save_windows(&mut self) {
        let (ids, focused) = self.windows.to_save();
        let focused = ids.get(focused).copied();
        let kept: Vec<(WinId, WindowState)> = ids
            .iter()
            .filter_map(|w| self.apps.get(w).map(|app| (*w, app)))
            .filter(|(_, app)| app.is_remembered())
            .map(|(w, app)| (w, app.window_state()))
            .collect();
        // A start for a loose page leaves the saved windows alone until a vault opens.
        if kept.is_empty() && self.loose {
            return;
        }
        let index = kept.iter().position(|(w, _)| Some(*w) == focused).unwrap_or(0);
        let mut c = self.common.borrow_mut();
        c.config.windows = kept.into_iter().map(|(_, state)| state).collect();
        c.config.focused_window = index;
        c.mark_config_dirty();
    }

    /// A window's tabs, sides or size changed: saves the windows shortly after.
    fn window_changed(&mut self, w: WinId) {
        let changed = self.apps.get_mut(&w).is_some_and(App::take_layout_changed);
        if !changed {
            return;
        }
        self.save_windows();
    }

    fn window_event(&mut self, w: WinId, event: UserEvent) {
        let before = Settings::of(&self.common.borrow().config);
        // Late answers for a window that closed meanwhile are dropped.
        let Some(app) = self.apps.get_mut(&w) else { return };
        app.handle(event);
        let after = Settings::of(&self.common.borrow().config);
        let mut changed = vec![w];
        if let Some(change) = before.change(&after) {
            for (id, other) in self.apps.iter_mut().filter(|(id, _)| **id != w) {
                other.settings_changed(&change);
                changed.push(*id);
            }
        }
        // Another window may have left a removed vault, so its layout changed too.
        for id in changed {
            self.window_changed(id);
        }
    }

    /// An event for the process. True when the process must end.
    fn process_event(&mut self, event: UserEvent, target: &EventLoopWindowTarget<Routed>) -> bool {
        match event {
            UserEvent::SaveConfig => {
                let mut c = self.common.borrow_mut();
                c.config_write_due();
                c.flush_config();
            }
            UserEvent::ThemeChanged => {
                self.common.borrow_mut().reload_theme();
                for app in self.apps.values_mut() {
                    app.restyle();
                    app.announce_theme();
                }
            }
            UserEvent::MonoFont(font) => {
                {
                    let mut c = self.common.borrow_mut();
                    c.mono = font;
                    c.reload_theme();
                }
                for app in self.apps.values_mut() {
                    app.restyle();
                }
            }
            UserEvent::Quit => {
                self.quit();
                return true;
            }
            UserEvent::Restart { from } => self.restart(from),
            UserEvent::Agent(AgentRequest::NewWindow { token, reply }) => {
                let w = self.new_window(target, token.as_deref());
                let _ = reply.send(Ok(w));
            }
            _ => {}
        }
        false
    }

    /// Replaces this process with the installed binary, with the same
    /// arguments. Every window is saved first, since exec skips the exit path,
    /// and the new process reopens them all. It returns only when exec fails,
    /// and then visor keeps running.
    fn restart(&mut self, from: WinId) {
        for app in self.apps.values_mut() {
            app.save_layout();
        }
        self.save_windows();
        self.common.borrow_mut().flush_config();
        let failed = match crate::update::running_exe() {
            None => tr("No se pudo reiniciar: no se encuentra visor.", "Could not restart: visor not found.").to_string(),
            Some(exe) => {
                let error = crate::platform::restart(std::process::Command::new(exe).args(std::env::args_os().skip(1)).env_remove("VISOR_LOOSE"));
                trf!("No se pudo reiniciar: {error}.", "Could not restart: {error}.")
            }
        };
        if let Some(app) = self.apps.get(&from) {
            app.restart_failed(&failed);
        }
    }
}
