use crate::annot;
use crate::bar::Bar;
use crate::files;
use crate::links;
use crate::panes::{Pane, Tab};
use crate::paths;
use crate::theme::{self, Theme};
use crate::vault::{self, Config, PaneState, Workspace};
use gtk::prelude::*;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use webkit2gtk::{
    LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, NavigationType,
    PolicyDecisionExt, PolicyDecisionType, URIRequestExt, UserContentInjectedFrames,
    UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebViewExt,
};
use wry::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use wry::http::{Request, Response};
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const SIDEBAR: &str = include_str!("sidebar.html");
const BRIDGE: &str = include_str!("bridge.js");
const ANNOT: &str = include_str!("annot.js");
const SIDEBAR_WIDTH: i32 = 260;
const MAX_CONSOLE: usize = 500;
const MAX_EVENTS: usize = 1000;
const ZOOM_STEPS: [f64; 13] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
/// Pages kept loaded across all tabs. Each one is a WebKit process of
/// 60–90 MB; the two on screen always count. See docs/decisiones.md.
const MAX_LIVE: usize = 3;

/// The view that will load and the load count it had before, to wait for the next one.
pub type Loading = (u64, u64);
pub type Reply = mpsc::Sender<Result<Option<Loading>, String>>;

/// Where to open something: replacing the focused tab, in a new tab, or on the other side.
#[derive(Clone, Copy, PartialEq)]
pub enum Place {
    Here,
    NewTab,
    Beside,
}

pub enum UserEvent {
    Sidebar(String),
    /// A message from a page view, with the URL of the page that sent it.
    Page { view: u64, url: String, body: String },
    Navigated { view: u64, url: String },
    PageLoaded(u64),
    /// A page view got the keyboard focus.
    Focused(u64),
    Open { path: PathBuf, raw: String, place: Place, reply: Reply },
    OpenUrl { url: String, place: Place, reply: Reply },
    Picked(Option<PathBuf>),
    VaultChanged(Vec<PathBuf>),
    ThemeChanged,
    MonoFont(Option<String>),
    Fullscreen { view: u64, on: bool },
    /// A link the page asked to open in a new tab: in the background, for
    /// Ctrl+click and middle click, or shown, for target="_blank".
    OpenTab { view: u64, url: String, background: bool },
    /// The reading position a page reported as its tab was hidden.
    Position { view: u64, json: String },
    /// A file dragged from the sidebar was dropped on a side: on one half of
    /// it (0 left, 1 right) when the window is not split, or on all of it.
    Dropped { pane: u64, half: Option<usize> },
    Action(Action),
}

pub enum Action {
    PickVault,
    ToggleSidebar,
    ToggleBar,
    /// Steps the page zoom in (1) or out (-1), or resets it (0).
    Zoom(i8),
    Reload,
    Back,
    Forward,
    FocusAddress,
    LeaveAddress,
    /// The address field lost focus; a hidden bar hides again.
    AddressDone,
    NewNote,
    ToggleEdit,
    PickElement,
    ToggleAnnotations,
    Go(String),
    NewTab,
    /// Closes the tab with this id, or the focused one.
    CloseTab(Option<u64>),
    /// Moves to the next (1) or previous (-1) tab of the focused side.
    CycleTab(i8),
    SelectTab(u64),
    ToggleSplit,
    /// Makes the pane with this id the one keys and agents act on.
    FocusPane(u64),
    /// Moves the keyboard focus to the left (0) or right (1) side.
    FocusSide(usize),
}

/// What a page view is showing, for the socket server.
#[derive(Default)]
pub struct Page {
    pub file: Option<String>,
    /// Set when the view shows a website instead of a vault file.
    pub url: Option<String>,
    pub loaded: bool,
    pub load_seq: u64,
    pub console: Vec<Value>,
    /// Annotations of the page that could not be placed, as reported by it.
    pub orphans: Option<Vec<String>>,
}

static NO_PAGE: Page = Page { file: None, url: None, loaded: false, load_seq: 0, console: Vec::new(), orphans: None };

/// What the socket server can read without going through the event loop.
#[derive(Default)]
pub struct Shared {
    pub vault: Option<PathBuf>,
    /// Every live page view, by view id.
    pub pages: HashMap<u64, Page>,
    /// The view of the focused side's active tab: the one agents act on.
    pub focused: u64,
    /// Sides and tabs, as `visor status` reports them.
    pub layout: Value,
    pub events: Vec<Value>,
    pub next_id: u64,
}

pub type SharedRef = Arc<Mutex<Shared>>;

impl Shared {
    fn with_saved_events() -> Self {
        let mut events: Vec<Value> = fs::File::open(paths::events())
            .map(|f| {
                BufReader::new(f)
                    .lines()
                    .map_while(Result::ok)
                    .filter_map(|line| serde_json::from_str(&line).ok())
                    .collect()
            })
            .unwrap_or_default();
        let next_id = events.last().and_then(|e| e["id"].as_u64()).unwrap_or(0) + 1;
        let skip = events.len().saturating_sub(MAX_EVENTS);
        events.drain(..skip);
        Shared { events, next_id, ..Default::default() }
    }

    /// The page on the focused side.
    pub fn page(&self) -> &Page {
        self.page_of(self.focused)
    }

    pub fn page_of(&self, view: u64) -> &Page {
        self.pages.get(&view).unwrap_or(&NO_PAGE)
    }

    pub fn status(&self) -> Value {
        let page = self.page();
        let path = match (&self.vault, &page.file) {
            (Some(v), Some(f)) => Some(v.join(f)),
            _ => None,
        };
        let errors = page.console.iter().filter(|e| e["level"] == "error").count();
        json!({
            "ok": true,
            "vault": self.vault,
            "file": page.file,
            "path": path,
            "url": page.url,
            "loaded": page.loaded,
            "errors": errors,
            "last_event_id": self.next_id - 1,
            "sides": self.layout,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SidebarMsg {
    Ready,
    PickVault,
    SwitchVault { path: PathBuf },
    RemoveVault { path: PathBuf },
    Open {
        path: String,
        #[serde(default)]
        tab: bool,
        #[serde(default)]
        beside: bool,
    },
    CreateNote { dir: String, name: String },
    CreateFolder { dir: String, name: String },
    Trash { path: String },
    Move { path: String, dir: String },
    /// A file started being dragged; the pages can take it while it lasts.
    DragStart { path: String },
    DragEnd,
}

/// Messages from vault pages. Websites never get to send these.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PageMsg {
    Console { level: String, text: String },
    Event { data: Value },
    PickVault,
    /// From a note's editor: replaces that note's text.
    SaveNote { text: String },
    /// From a checkbox in a rendered note: the task whose `[ ]` starts at `offset`.
    ToggleTask { offset: usize, checked: bool },
    /// From a `[[link]]` to a note that does not exist yet.
    CreateNote { dir: String, name: String },
    AnnotSave { annotation: Value },
    AnnotDelete { id: String },
    /// Which annotations the page could not place.
    AnnotStatus { orphans: Vec<String>, total: usize },
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub fn launch() -> i32 {
    let socket = paths::socket();
    if UnixStream::connect(&socket).is_ok() {
        eprintln!("visor ya está abierto.");
        return 0;
    }
    let _ = fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("No se pudo crear {}: {e}", socket.display());
            return 1;
        }
    };

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let shared: SharedRef = Arc::new(Mutex::new(Shared::with_saved_events()));
    crate::server::spawn(listener, shared.clone(), proxy.clone());

    let mut config = Config::load();
    let Some((port, base)) = crate::http::start(config.port, shared.clone()) else {
        eprintln!("No se pudo iniciar el servidor local.");
        return 1;
    };
    vault::set_base(base);
    if config.port != Some(port) {
        config.port = Some(port);
        config.save();
    }
    let theme = Theme::load(None);
    let p = proxy.clone();
    std::thread::spawn(move || {
        let _ = p.send_event(UserEvent::MonoFont(theme::monospace_font()));
    });
    let window = WindowBuilder::new()
        .with_title("Visor")
        .with_inner_size(LogicalSize::new(1200.0, 820.0))
        .build(&event_loop)
        .expect("ventana");
    connect_shortcuts(&window, proxy.clone());

    // Layout: [sidebar | left side | right side], split by draggable dividers.
    // Each side stacks its tab strip, its address bar and its page views.
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_position(config.sidebar_width.unwrap_or(SIDEBAR_WIDTH));
    window.default_vbox().expect("vbox de GTK").pack_start(&paned, true, true, 0);
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    // A new right side gets its saved share of the width once the width is known.
    let place_split: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let pending = place_split.clone();
    split.connect_size_allocate(move |split, alloc| {
        if let Some(ratio) = pending.get()
            && alloc.width() > 1
        {
            pending.set(None);
            split.set_position((alloc.width() as f64 * ratio) as i32);
        }
    });
    let styles = gtk::CssProvider::new();
    if let Some(screen) = WidgetExt::screen(&paned) {
        gtk::StyleContext::add_provider_for_screen(&screen, &styles, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let background = theme.as_ref().map_or((255, 255, 255, 255), Theme::rgba);
    // One context for every view: one network process, and a single writer
    // for cookies and storage, which two contexts on one folder would fight over.
    let mut context = WebContext::new(None);
    let sidebar = match build_sidebar(&paned, &mut context, background, proxy.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("No se pudo crear el webview: {e}");
            return 1;
        }
    };
    paned.pack2(&split, true, true);
    let first = Pane::new(1, &proxy);
    first.bar.root.set_visible(config.bar_shown);
    split.pack1(&first.root, true, false);
    split.show();
    paned.show();

    let mut app = App {
        sidebar,
        context,
        background,
        panes: vec![first],
        focused: 0,
        split,
        place_split,
        next_id: 2,
        clock: 0,
        mono: None,
        dragging: None,
        sidebar_before_fullscreen: None,
        own_write: None,
        paned,
        styles,
        shared,
        proxy: proxy.clone(),
        config,
        theme,
        watcher: None,
        _theme_watcher: theme::dir().and_then(|d| watch(&d, 300, proxy, |_| UserEvent::ThemeChanged)),
    };
    app.apply_theme();
    // Load the page now instead of after the sidebar: both webviews start in parallel.
    let current = app.config.current.clone().filter(|p| p.is_dir());
    app.show_vault(current);
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                app.save_layout();
                let _ = fs::remove_file(paths::socket());
                *flow = ControlFlow::Exit;
            }
            Event::UserEvent(event) => app.handle(event),
            _ => {}
        }
    })
}

/// App-wide shortcuts. They are caught on the window before the focused
/// webview sees them, so they work whichever side has focus.
fn connect_shortcuts(window: &tao::window::Window, proxy: EventLoopProxy<UserEvent>) {
    use gtk::gdk::{ModifierType, keys::constants as key};
    window.gtk_window().connect_key_press_event(move |_, event| {
        let ctrl = event.state().contains(ModifierType::CONTROL_MASK);
        let alt = event.state().contains(ModifierType::MOD1_MASK);
        let shift = event.state().contains(ModifierType::SHIFT_MASK);
        let k = event.keyval().to_lower();
        let action = match () {
            _ if ctrl && shift && k == key::a => Action::PickElement,
            _ if ctrl && shift && k == key::l => Action::ToggleAnnotations,
            _ if ctrl && shift && k == key::b => Action::ToggleBar,
            _ if ctrl && matches!(k, key::plus | key::equal | key::KP_Add) => Action::Zoom(1),
            _ if ctrl && matches!(k, key::minus | key::KP_Subtract) => Action::Zoom(-1),
            _ if ctrl && matches!(k, key::_0 | key::KP_0) => Action::Zoom(0),
            _ if ctrl && k == key::ISO_Left_Tab => Action::CycleTab(-1),
            _ if ctrl && k == key::Tab => Action::CycleTab(if shift { -1 } else { 1 }),
            _ if ctrl && alt && k == key::Left => Action::FocusSide(0),
            _ if ctrl && alt && k == key::Right => Action::FocusSide(1),
            // Also «|»: on Spanish layouts «\» needs AltGr and «|» does not.
            _ if ctrl && matches!(k, key::backslash | key::bar) => Action::ToggleSplit,
            _ if k == key::F5 => Action::Reload,
            _ if ctrl && k == key::r => Action::Reload,
            _ if ctrl && k == key::o => Action::PickVault,
            _ if ctrl && k == key::b => Action::ToggleSidebar,
            _ if ctrl && k == key::l => Action::FocusAddress,
            _ if ctrl && k == key::n => Action::NewNote,
            _ if ctrl && k == key::e => Action::ToggleEdit,
            _ if ctrl && k == key::t => Action::NewTab,
            _ if ctrl && k == key::w => Action::CloseTab(None),
            _ if alt && k == key::Left => Action::Back,
            _ if alt && k == key::Right => Action::Forward,
            _ => return gtk::glib::Propagation::Proceed,
        };
        let _ = proxy.send_event(UserEvent::Action(action));
        gtk::glib::Propagation::Stop
    });
}

fn build_sidebar(
    paned: &gtk::Paned,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: EventLoopProxy<UserEvent>,
) -> wry::Result<WebView> {
    WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_custom_protocol("app".into(), |_, request| {
            if request.uri().path() == "/" {
                respond(200, "text/html; charset=utf-8", SIDEBAR.as_bytes().to_vec())
            } else {
                respond(404, "text/plain", b"No encontrado".to_vec())
            }
        })
        .with_ipc_handler(move |request: Request<String>| {
            let _ = proxy.send_event(UserEvent::Sidebar(request.into_body()));
        })
        .with_url("app://localhost/")
        .build_gtk(paned)
}

/// A page view inside `container`. Its events carry `id`, so the app knows
/// which tab they come from.
fn build_page(
    container: &gtk::Box,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: EventLoopProxy<UserEvent>,
    id: u64,
) -> wry::Result<WebView> {
    let ipc = proxy.clone();
    let new_window = proxy.clone();
    let clicks = proxy.clone();
    let page = WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_initialization_script(BRIDGE.replace("__VISOR_BASE__", vault::base()))
        .with_initialization_script(ANNOT.replace("__VISOR_BASE__", vault::base()))
        .with_ipc_handler(move |request: Request<String>| {
            let url = request.uri().to_string();
            let _ = ipc.send_event(UserEvent::Page { view: id, url, body: request.into_body() });
        })
        // target="_blank" and window.open open a tab beside this one.
        .with_new_window_req_handler(move |url, _| {
            if is_web(&url) {
                let _ = new_window.send_event(UserEvent::OpenTab { view: id, url, background: false });
            } else if url.starts_with("mailto:") {
                open_external(&url);
            }
            NewWindowResponse::Deny
        })
        .with_devtools(true)
        .build_gtk(container)?;
    let view = page.webview();

    // Clicked links may go to the vault or the web. Mail goes to the mail app,
    // and any other scheme is refused rather than handed to the system.
    // Only clicks are checked, so embedded iframes still load.
    view.connect_decide_policy(move |_, decision, kind| {
        if kind != PolicyDecisionType::NavigationAction && kind != PolicyDecisionType::NewWindowAction {
            return false;
        }
        let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else { return false };
        let Some(action) = nav.navigation_action() else { return false };
        if action.navigation_type() != NavigationType::LinkClicked {
            return false;
        }
        let uri = action.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
        if is_web(&uri) {
            // Ctrl+click and middle click open a tab in the background, as in a browser.
            let control = action.modifiers() & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;
            if !(control || action.mouse_button() == 2) {
                return false;
            }
            let _ = clicks.send_event(UserEvent::OpenTab { view: id, url: uri, background: true });
            decision.ignore();
            return true;
        }
        if uri.starts_with("mailto:") {
            open_external(&uri);
        }
        decision.ignore();
        true
    });

    // Followed here, not by the page script, so websites are tracked too.
    let p = proxy.clone();
    view.connect_load_changed(move |view, event| {
        let event = match event {
            LoadEvent::Committed => UserEvent::Navigated { view: id, url: view.uri().map(|u| u.to_string()).unwrap_or_default() },
            LoadEvent::Finished => UserEvent::PageLoaded(id),
            _ => return,
        };
        let _ = p.send_event(event);
    });

    let p = proxy.clone();
    view.connect_focus_in_event(move |_, _| {
        let _ = p.send_event(UserEvent::Focused(id));
        gtk::glib::Propagation::Proceed
    });

    // WebKit makes the whole window fullscreen; the app hides its own chrome
    // so only the element (a video, usually) remains. Returning false lets
    // WebKit go on with its default handling.
    let p = proxy.clone();
    view.connect_enter_fullscreen(move |_| {
        let _ = p.send_event(UserEvent::Fullscreen { view: id, on: true });
        false
    });
    let p = proxy.clone();
    view.connect_leave_fullscreen(move |_| {
        let _ = p.send_event(UserEvent::Fullscreen { view: id, on: false });
        false
    });

    // Mouse side buttons.
    let p = proxy;
    view.connect_button_press_event(move |_, event| {
        let action = match event.button() {
            8 => Action::Back,
            9 => Action::Forward,
            _ => return gtk::glib::Propagation::Proceed,
        };
        let _ = p.send_event(UserEvent::Action(action));
        gtk::glib::Propagation::Stop
    });
    Ok(page)
}

fn open_external(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

fn respond(status: u16, mime: &str, body: Vec<u8>) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, mime)
        .header(CACHE_CONTROL, "no-store")
        .body(Cow::Owned(body))
        .unwrap()
}

/// Watches `path` and forwards changes in batches once they settle.
/// Access events are dropped: serving a page opens it, which would reload it forever.
fn watch(
    path: &Path,
    settle_ms: u64,
    proxy: EventLoopProxy<UserEvent>,
    wrap: fn(Vec<PathBuf>) -> UserEvent,
) -> Option<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && !matches!(event.kind, EventKind::Access(_))
        {
            let _ = tx.send(event.paths);
        }
    })
    .ok()?;
    watcher.watch(path, RecursiveMode::Recursive).ok()?;
    // Ends on its own when the watcher, and with it the sender, is dropped.
    std::thread::spawn(move || {
        while let Ok(mut batch) = rx.recv() {
            while let Ok(more) = rx.recv_timeout(Duration::from_millis(settle_ms)) {
                batch.extend(more);
            }
            batch.sort();
            batch.dedup();
            let _ = proxy.send_event(wrap(batch));
        }
    });
    Some(watcher)
}

fn empty_page(message: &str, pick: bool) -> String {
    let button = if pick {
        r#"<button onclick="ipc.postMessage(JSON.stringify({type:'pick_vault'}))">Abrir carpeta…</button>"#
    } else {
        ""
    };
    format!(
        r#"<!doctype html><html lang="es"><meta charset="utf-8"><style>
html {{ color-scheme: var(--visor-mode, light dark); }}
body {{ margin: 0; height: 100vh; display: grid; place-content: center; justify-items: center; gap: 12px;
  font: 14px/1.4 system-ui, sans-serif; text-align: center; padding: 24px; box-sizing: border-box;
  color: var(--visor-muted, GrayText); background: var(--visor-bg, Canvas); }}
p {{ margin: 0; max-width: 36ch; text-wrap: balance; }}
button {{ font: inherit; padding: 7px 14px; border-radius: 6px; cursor: pointer; background: none;
  color: var(--visor-fg, CanvasText); border: 1px solid color-mix(in srgb, currentColor 30%, transparent); }}
</style><p>{message}</p>{button}"#
    )
}

struct App {
    sidebar: WebView,
    context: WebContext,
    background: (u8, u8, u8, u8),
    /// The left side and, when split, the right one.
    panes: Vec<Pane>,
    /// Index in `panes` of the side keys and agents act on.
    focused: usize,
    split: gtk::Paned,
    /// The share of the width for the left side, applied once the new right side has a size.
    place_split: Rc<Cell<Option<f64>>>,
    /// Source of ids for panes, tabs and views.
    next_id: u64,
    /// Counts tab switches, to find the tab used longest ago.
    clock: u64,
    mono: Option<String>,
    /// The file being dragged from the sidebar.
    dragging: Option<String>,
    /// Whether the sidebar was showing when fullscreen began, to restore it after.
    sidebar_before_fullscreen: Option<bool>,
    /// The last file the app wrote and a hash of what it wrote. A change on disk
    /// that still matches is the app's own; anything else came from outside.
    own_write: Option<(PathBuf, u64)>,
    paned: gtk::Paned,
    styles: gtk::CssProvider,
    shared: SharedRef,
    proxy: EventLoopProxy<UserEvent>,
    config: Config,
    theme: Option<Theme>,
    watcher: Option<RecommendedWatcher>,
    _theme_watcher: Option<RecommendedWatcher>,
}

impl App {
    fn sidebar_js(&self, code: String) {
        let _ = self.sidebar.evaluate_script(&code);
    }

    fn vault(&self) -> Option<PathBuf> {
        self.shared.lock().unwrap().vault.clone()
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn pane(&self) -> &Pane {
        &self.panes[self.focused]
    }

    /// The view of the focused side's active tab. It is always loaded.
    fn view(&self) -> Option<&WebView> {
        self.pane().tab().view.as_ref().map(|(_, v)| v)
    }

    fn eval(&self, code: &str) {
        if let Some(view) = self.view() {
            let _ = view.evaluate_script(code);
        }
    }

    fn focused_file(&self) -> Option<String> {
        let shared = self.shared.lock().unwrap();
        shared.page().file.clone()
    }

    /// The side and tab of a view.
    fn find_view(&self, view: u64) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(p, pane)| {
            pane.tabs.iter().position(|t| t.view_id() == Some(view)).map(|i| (p, i))
        })
    }

    fn find_tab(&self, id: u64) -> Option<(usize, usize)> {
        self.panes
            .iter()
            .enumerate()
            .find_map(|(p, pane)| pane.tabs.iter().position(|t| t.id == id).map(|i| (p, i)))
    }

    fn view_of(&self, view: u64) -> Option<&WebView> {
        let (p, i) = self.find_view(view)?;
        self.panes[p].tabs[i].view.as_ref().map(|(_, v)| v)
    }

    fn info(&self, tree: Option<vault::Dir>) -> Value {
        let vaults: Vec<Value> = self
            .config
            .vaults
            .iter()
            .map(|v| {
                let name = v.file_name().map_or_else(|| v.display().to_string(), |n| n.to_string_lossy().into());
                json!({ "path": v, "name": name })
            })
            .collect();
        json!({ "vaults": vaults, "current": self.vault(), "tree": tree })
    }

    fn refresh_sidebar(&self) {
        let tree = self.vault().as_deref().map(vault::tree);
        self.sidebar_js(format!("app.show({}); app.current({})", self.info(tree), json!(self.focused_file())));
    }

    /// Opens a vault and restores its tabs. Without a vault the window shows one empty tab.
    fn show_vault(&mut self, root: Option<PathBuf>) {
        self.publish();
        while self.panes.len() > 1 {
            self.remove_pane(1);
        }
        for tab in std::mem::take(&mut self.panes[0].tabs) {
            self.drop_view(tab);
        }
        {
            let mut shared = self.shared.lock().unwrap();
            shared.vault = root.clone();
            shared.pages.clear();
        }
        self.watcher = None;
        if let Some(root) = &root {
            self.config.add(root);
            self.config.save();
            self.watcher = watch(root, 150, self.proxy.clone(), UserEvent::VaultChanged);
        }
        let tree = root.as_deref().map(vault::tree);
        self.sidebar_js(format!("app.show({})", self.info(tree)));

        let workspace = root.as_ref().and_then(|r| self.restore(r)).unwrap_or_default();
        for (p, state) in workspace.panes.iter().enumerate().take(2) {
            if p == 1 {
                self.add_pane();
            }
            let tabs: Vec<Tab> = state.tabs.iter().map(|t| Tab::new(self.new_id(), t.clone())).collect();
            let pane = &mut self.panes[p];
            pane.tabs = tabs;
            pane.active = state.active.min(pane.tabs.len().saturating_sub(1));
        }
        if self.panes[0].tabs.is_empty() {
            let id = self.new_id();
            self.panes[0].tabs.push(Tab::new(id, String::new()));
            self.panes[0].active = 0;
        }
        self.focused = workspace.focused.min(self.panes.len() - 1);
        for p in 0..self.panes.len() {
            self.show_pane(p);
        }
        self.focus_changed();
        // Otherwise GTK gives the keys to whichever view it finds first.
        self.focus_pane(self.focused, true);
    }

    /// The saved tabs of `root`, without files that no longer exist.
    fn restore(&mut self, root: &Path) -> Option<Workspace> {
        let keep = |t: &String| t.is_empty() || is_web(t) || root.join(t).is_file();
        let mut workspace = self.config.workspaces.get(root).cloned().or_else(|| {
            let last = self.config.last.get(root)?.clone();
            Some(Workspace { panes: vec![PaneState { tabs: vec![last], active: 0 }], focused: 0 })
        })?;
        for pane in &mut workspace.panes {
            let active = pane.tabs.get(pane.active).cloned();
            pane.tabs.retain(keep);
            pane.active = active.and_then(|a| pane.tabs.iter().position(|t| *t == a)).unwrap_or(0);
        }
        workspace.panes.retain(|p| !p.tabs.is_empty());
        (!workspace.panes.is_empty()).then_some(workspace)
    }

    /// Records the sides and tabs for `visor status` and in the vault's workspace.
    fn publish(&mut self) {
        let sides: Vec<Value> = self
            .panes
            .iter()
            .map(|pane| {
                let tabs: Vec<&str> = pane.tabs.iter().map(|t| t.target.as_str()).collect();
                json!({ "tabs": tabs, "active": pane.active })
            })
            .collect();
        let focused = self.pane().tabs.get(self.pane().active).and_then(Tab::view_id).unwrap_or(0);
        let root = {
            let mut shared = self.shared.lock().unwrap();
            shared.layout = json!({ "list": sides, "focused": self.focused });
            shared.focused = focused;
            shared.vault.clone()
        };
        let Some(root) = root else { return };
        let workspace = Workspace {
            panes: self
                .panes
                .iter()
                .map(|p| PaneState { tabs: p.tabs.iter().map(|t| t.target.clone()).collect(), active: p.active })
                .collect(),
            focused: self.focused,
        };
        self.config.workspaces.insert(root, workspace);
        self.config.save();
    }

    /// Adds the right side, empty.
    fn add_pane(&mut self) {
        let id = self.new_id();
        let pane = Pane::new(id, &self.proxy);
        pane.bar.root.set_visible(self.config.bar_shown);
        self.split.pack2(&pane.root, true, false);
        self.place_split.set(Some(self.config.split_ratio.unwrap_or(0.5)));
        self.split.queue_resize();
        self.panes.push(pane);
    }

    fn remove_pane(&mut self, p: usize) {
        self.save_split();
        let mut pane = self.panes.remove(p);
        for tab in std::mem::take(&mut pane.tabs) {
            self.drop_view(tab);
        }
        self.split.remove(&pane.root);
        // The right side moves to the left.
        if p == 0
            && let Some(left) = self.panes.first()
        {
            self.split.remove(&left.root);
            self.split.pack1(&left.root, true, false);
        }
        self.focused = 0;
    }

    fn drop_view(&mut self, mut tab: Tab) -> Tab {
        tab.ready = false;
        if let Some((id, _)) = tab.view.take() {
            self.shared.lock().unwrap().pages.remove(&id);
        }
        tab
    }

    /// Loads the view of a tab if it has none, and unloads the tab hidden
    /// longest ago when there are more than MAX_LIVE.
    fn ensure_live(&mut self, p: usize, i: usize) {
        self.clock += 1;
        self.panes[p].tabs[i].used = self.clock;
        if self.panes[p].tabs[i].view.is_none() {
            let id = self.new_id();
            let built = build_page(&self.panes[p].views, &mut self.context, self.background, self.proxy.clone(), id);
            let view = match built {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("No se pudo crear el webview: {e}");
                    return;
                }
            };
            view.webview().set_visible(false);
            self.style_view(&view);
            view.webview().set_zoom_level(self.config.zoom.unwrap_or(1.0));
            self.shared.lock().unwrap().pages.insert(id, Page::default());
            let tab = &mut self.panes[p].tabs[i];
            let (target, fragment) = (tab.target.clone(), std::mem::take(&mut tab.fragment));
            self.load_into(&view, &target, &fragment);
            self.panes[p].tabs[i].view = Some((id, view));
        }
        loop {
            let live: Vec<(usize, usize, u64)> = self
                .panes
                .iter()
                .enumerate()
                .flat_map(|(p, pane)| {
                    pane.tabs.iter().enumerate().filter(|(_, t)| t.view.is_some()).map(move |(i, t)| (p, i, t.used))
                })
                .collect();
            if live.len() <= MAX_LIVE {
                break;
            }
            let oldest = live
                .into_iter()
                .filter(|(p, i, _)| self.panes[*p].active != *i)
                .min_by_key(|(_, _, used)| *used);
            let Some((p, i, _)) = oldest else { break };
            let tab = &mut self.panes[p].tabs[i];
            tab.restore = tab.position.take();
            tab.ready = false;
            if let Some((id, _)) = tab.view.take() {
                self.shared.lock().unwrap().pages.remove(&id);
            }
        }
    }

    fn load_into(&self, view: &WebView, target: &str, fragment: &str) {
        if target.is_empty() {
            let _ = view.load_html(&empty_page(self.empty_message(), self.vault().is_none()));
        } else if is_web(target) {
            let _ = view.load_url(target);
        } else {
            let _ = view.load_url(&format!("{}{fragment}", vault::url(target)));
        }
    }

    fn empty_message(&self) -> &'static str {
        match self.vault() {
            None => "Abre una carpeta con archivos HTML para empezar.",
            Some(root) => {
                fn has_pages(dir: &Value) -> bool {
                    dir["files"].as_array().is_some_and(|f| !f.is_empty())
                        || dir["dirs"].as_array().is_some_and(|d| d.iter().any(has_pages))
                }
                if !has_pages(&json!(vault::tree(&root))) {
                    "Esta carpeta todavía no tiene archivos HTML. Aparecerán aquí en cuanto se creen."
                } else {
                    "Elige un archivo de la lista."
                }
            }
        }
    }

    /// Loads the active tab of side `p` if needed, shows it and updates the side's controls.
    fn show_pane(&mut self, p: usize) {
        let active = self.panes[p].active;
        self.ensure_live(p, active);
        let pane = &self.panes[p];
        // Asked while still on screen: the tab may be unloaded later.
        for (i, tab) in pane.tabs.iter().enumerate() {
            if let Some((id, view)) = &tab.view
                && i != active
                && view.webview().is_visible()
            {
                let (proxy, id) = (self.proxy.clone(), *id);
                let _ = view.evaluate_script_with_callback(
                    "window.__visorNote ? __visorNote.hide() : { y: scrollY }",
                    move |json| {
                        let _ = proxy.send_event(UserEvent::Position { view: id, json });
                    },
                );
            }
        }
        pane.show_active();
        pane.render(p == self.focused);
        pane.bar.show(&pane.tab().target);
        self.update_nav(p);
    }

    /// Updates what depends on the focused side: the sidebar, the strips and the agents' view.
    fn focus_changed(&mut self) {
        for (p, pane) in self.panes.iter().enumerate() {
            pane.render(p == self.focused);
        }
        self.publish();
        self.sidebar_js(format!("app.current({})", json!(self.focused_file())));
    }

    fn focus_pane(&mut self, p: usize, grab: bool) {
        if p != self.focused {
            self.focused = p;
            self.focus_changed();
        }
        if grab && let Some(view) = self.view() {
            view.webview().grab_focus();
        }
    }

    /// Opens `target` (a vault path, a web address or empty) and returns the
    /// view that loads it, to wait for the load.
    fn open_in(&mut self, target: String, place: Place) -> Option<Loading> {
        self.open_with(target, place, "")
    }

    fn open_with(&mut self, target: String, place: Place, fragment: &str) -> Option<Loading> {
        let p = match place {
            Place::Here => self.focused,
            Place::NewTab => self.focused,
            Place::Beside => {
                if self.panes.len() == 1 {
                    self.add_pane();
                }
                1 - self.focused
            }
        };
        let i = if place == Place::Here && !self.panes[p].tabs.is_empty() {
            self.panes[p].active
        } else {
            let at = if self.panes[p].tabs.is_empty() { 0 } else { self.panes[p].active + 1 };
            let id = self.new_id();
            self.panes[p].tabs.insert(at, Tab::new(id, String::new()));
            at
        };
        self.panes[p].active = i;
        let tab = &mut self.panes[p].tabs[i];
        tab.target = target.clone();
        tab.title = None;
        tab.position = None;
        tab.restore = None;
        let loading = match &self.panes[p].tabs[i].view {
            Some((id, view)) => {
                let before = self.shared.lock().unwrap().page_of(*id).load_seq;
                self.load_into(view, &target, fragment);
                Some((*id, before))
            }
            None => {
                self.panes[p].tabs[i].fragment = fragment.to_string();
                self.ensure_live(p, i);
                self.panes[p].tabs[i].view_id().map(|id| (id, 0))
            }
        };
        self.show_pane(p);
        self.focused = p;
        self.focus_changed();
        if let Some(view) = self.view() {
            view.webview().grab_focus();
        }
        loading
    }

    /// Resolves what was typed in the address field: a URL, a vault path or a bare domain.
    fn go(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let in_vault = self.vault().and_then(|root| {
            let path = vault::resolve(&root, text)?;
            Some(vault::rel(&root.canonicalize().ok()?, &path))
        });
        if is_web(text) {
            self.open_in(text.to_string(), Place::Here);
        } else if let Some(rel) = in_vault {
            self.open_in(rel, Place::Here);
        } else if !text.contains(char::is_whitespace) && text.contains('.') {
            self.open_in(format!("https://{text}"), Place::Here);
        } else {
            self.pane().bar.address.style_context().add_class("error");
        }
    }

    fn update_nav(&self, p: usize) {
        let pane = &self.panes[p];
        let Some((_, view)) = &pane.tab().view else { return };
        let view = view.webview();
        pane.bar.back.set_sensitive(view.can_go_back());
        pane.bar.forward.set_sensitive(view.can_go_forward());
    }

    fn style_view(&self, view: &WebView) {
        if let Some(manager) = view.webview().user_content_manager() {
            manager.remove_all_style_sheets();
            if let Some(theme) = &self.theme {
                manager.add_style_sheet(&UserStyleSheet::new(
                    &theme.page_css(),
                    UserContentInjectedFrames::AllFrames,
                    UserStyleLevel::User,
                    &[],
                    &[],
                ));
            }
        }
        if let Some(theme) = &self.theme {
            let _ = view.set_background_color(theme.rgba());
        }
    }

    fn live_views(&self) -> impl Iterator<Item = &WebView> {
        self.panes.iter().flat_map(|p| p.tabs.iter().filter_map(|t| t.view.as_ref().map(|(_, v)| v)))
    }

    fn apply_theme(&mut self) {
        self.theme = Theme::load(self.mono.clone());
        if let Some(theme) = &self.theme {
            self.background = theme.rgba();
        }
        for view in self.live_views() {
            self.style_view(view);
        }
        let css = self.theme.as_ref().map_or(String::new(), |t| {
            format!(
                "paned > separator {{ background: {}; background-image: none; min-width: 1px; }}\n{}\n{}",
                t.tone(14.0),
                Bar::css(t),
                Pane::css(t)
            )
        });
        let _ = self.styles.load_from_data(css.as_bytes());
        if let Some(theme) = &self.theme {
            let _ = self.sidebar.set_background_color(theme.rgba());
        }
        let json = self.theme.as_ref().map_or(Value::Null, Theme::json);
        self.sidebar_js(format!("app.setTheme({json})"));
    }

    fn save_layout(&mut self) {
        if self.sidebar.webview().is_visible() {
            self.config.sidebar_width = Some(self.paned.position());
        }
        self.save_split();
        self.publish();
    }

    /// Remembers where the divider is, while there are two sides to measure.
    fn save_split(&mut self) {
        let width = self.split.allocated_width();
        if self.panes.len() == 2 && width > 1 && self.place_split.get().is_none() {
            let ratio = self.split.position() as f64 / width as f64;
            self.config.split_ratio = Some(ratio.clamp(0.1, 0.9));
        }
    }

    fn handle(&mut self, event: UserEvent) {
        match event {
            UserEvent::Sidebar(body) => match serde_json::from_str(&body) {
                Ok(msg) => self.on_sidebar(msg),
                Err(e) => eprintln!("Mensaje no válido del panel: {e}"),
            },
            UserEvent::Page { view, url, body } => {
                if let Ok(msg) = serde_json::from_str(&body) {
                    self.on_page(view, &url, msg);
                }
            }
            UserEvent::Navigated { view, url } => self.navigated(view, url),
            UserEvent::PageLoaded(view) => {
                {
                    let mut shared = self.shared.lock().unwrap();
                    let Some(page) = shared.pages.get_mut(&view) else { return };
                    page.loaded = true;
                    page.load_seq += 1;
                }
                let Some((p, i)) = self.find_view(view) else { return };
                let tab = &mut self.panes[p].tabs[i];
                if let Some(position) = tab.restore.take()
                    && let Some((_, v)) = &tab.view
                {
                    let _ = v.evaluate_script(&format!(
                        "(t => window.__visorNote ? __visorNote.place(t) : scrollTo(0, t.y))({position})"
                    ));
                }
                if is_web(&tab.target)
                    && let Some((_, v)) = &tab.view
                {
                    tab.title = v.webview().title().map(|t| t.to_string());
                    self.panes[p].render(p == self.focused);
                }
                self.update_nav(p);
            }
            UserEvent::Focused(view) => {
                if let Some((p, i)) = self.find_view(view)
                    && self.panes[p].active == i
                {
                    self.focus_pane(p, false);
                }
            }
            UserEvent::Open { path, raw, place, reply } => {
                let _ = reply.send(self.open(path, &raw, place));
            }
            UserEvent::OpenUrl { url, place, reply } => {
                let _ = reply.send(Ok(self.open_in(url, place)));
            }
            UserEvent::Picked(Some(path)) => self.show_vault(Some(path)),
            UserEvent::Picked(None) => {}
            UserEvent::VaultChanged(changed) => self.vault_changed(changed),
            UserEvent::ThemeChanged => self.apply_theme(),
            UserEvent::Fullscreen { view, on } => self.fullscreen(view, on),
            UserEvent::OpenTab { view, url, background } => self.open_tab(view, &url, background),
            UserEvent::Dropped { pane, half } => self.dropped(pane, half),
            UserEvent::Position { view, json } => {
                if let Some((p, i)) = self.find_view(view) {
                    self.panes[p].tabs[i].position = clean_position(&json);
                }
            }
            UserEvent::MonoFont(font) => {
                self.mono = font;
                self.apply_theme();
            }
            UserEvent::Action(action) => self.act(action),
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Reload => {
                if let Some(view) = self.view() {
                    let _ = view.reload();
                }
            }
            Action::Back => self.view().map_or((), |v| v.webview().go_back()),
            Action::Forward => self.view().map_or((), |v| v.webview().go_forward()),
            Action::PickVault => self.pick_vault(),
            Action::ToggleSidebar => {
                let side = self.sidebar.webview();
                if side.is_visible() {
                    self.save_layout();
                }
                side.set_visible(!side.is_visible());
            }
            Action::FocusAddress => {
                let bar = &self.pane().bar;
                bar.root.show();
                bar.address.grab_focus();
            }
            Action::AddressDone => {
                if !self.config.bar_shown {
                    for pane in &self.panes {
                        if !pane.bar.address.has_focus() {
                            pane.bar.root.hide();
                        }
                    }
                }
            }
            Action::ToggleBar => {
                self.config.bar_shown = !self.config.bar_shown;
                self.config.save();
                let typing = self.panes.iter().any(|p| p.bar.address.has_focus());
                for pane in &self.panes {
                    pane.bar.root.set_visible(self.config.bar_shown);
                }
                if !self.config.bar_shown && typing && let Some(view) = self.view() {
                    view.webview().grab_focus();
                }
            }
            Action::Zoom(step) => {
                let Some(now) = self.view().map(|v| v.webview().zoom_level()) else { return };
                let zoom = match step {
                    0 => 1.0,
                    1 => ZOOM_STEPS.into_iter().find(|z| *z > now + 0.01).unwrap_or(now),
                    _ => ZOOM_STEPS.into_iter().rev().find(|z| *z < now - 0.01).unwrap_or(now),
                };
                for view in self.live_views() {
                    view.webview().set_zoom_level(zoom);
                }
                self.config.zoom = Some(zoom).filter(|z| *z != 1.0);
                self.config.save();
            }
            Action::LeaveAddress => {
                if let Some(view) = self.view() {
                    view.webview().grab_focus();
                }
                let pane = self.pane();
                pane.bar.show(&pane.tab().target);
            }
            Action::Go(text) => self.go(&text),
            Action::PickElement => self.page_call("window.__visorAnnot && window.__visorAnnot.pickElement()"),
            Action::ToggleAnnotations => self.page_call("window.__visorAnnot && window.__visorAnnot.togglePanel()"),
            Action::ToggleEdit => self.page_call("window.__visorNote && window.__visorNote.toggle()"),
            Action::NewNote => {
                if self.vault().is_none() {
                    return;
                }
                let dir = self.focused_file().as_deref().and_then(|f| f.rsplit_once('/')).map_or("", |(d, _)| d).to_string();
                let side = self.sidebar.webview();
                side.show();
                side.grab_focus();
                self.sidebar_js(format!("app.newNote({})", json!(dir)));
            }
            Action::NewTab => {
                self.open_in(String::new(), Place::NewTab);
            }
            Action::CloseTab(id) => {
                let found = match id {
                    Some(id) => self.find_tab(id),
                    None => Some((self.focused, self.pane().active)),
                };
                if let Some((p, i)) = found {
                    self.close_tab(p, i);
                }
            }
            Action::CycleTab(step) => {
                let pane = &mut self.panes[self.focused];
                let n = pane.tabs.len() as isize;
                pane.active = (pane.active as isize + step as isize).rem_euclid(n) as usize;
                self.show_pane(self.focused);
                self.focus_changed();
                self.focus_pane(self.focused, true);
            }
            Action::SelectTab(id) => {
                if let Some((p, i)) = self.find_tab(id) {
                    self.panes[p].active = i;
                    self.show_pane(p);
                    self.focused = p;
                    self.focus_changed();
                    self.focus_pane(p, true);
                }
            }
            Action::ToggleSplit => self.toggle_split(),
            Action::FocusPane(id) => {
                if let Some(p) = self.panes.iter().position(|pane| pane.id == id) {
                    self.focus_pane(p, false);
                }
            }
            Action::FocusSide(p) => {
                if p < self.panes.len() {
                    self.focus_pane(p, true);
                }
            }
        }
    }

    /// Opens a link from a page in a new tab on the page's side. A background
    /// tab is not loaded until it is shown, so it costs nothing until then.
    fn open_tab(&mut self, view: u64, url: &str, background: bool) {
        let Some((p, _)) = self.find_view(view) else { return };
        let (address, fragment) = url.split_once('#').map_or((url, ""), |(a, f)| (a, f));
        let target = vault::rel_from_url(address).unwrap_or_else(|| url.to_string());
        let fragment = if target == url || fragment.is_empty() { String::new() } else { format!("#{fragment}") };
        if background {
            let mut tab = Tab::new(self.new_id(), target);
            tab.fragment = fragment;
            self.panes[p].tabs.push(tab);
            self.panes[p].render(p == self.focused);
            self.publish();
        } else {
            self.focused = p;
            self.open_with(target, Place::NewTab, &fragment);
        }
    }

    fn end_drag(&mut self) {
        for pane in &self.panes {
            pane.show_drop(false, false);
        }
        self.dragging = None;
    }

    /// Opens the dragged file where it was dropped. On an unsplit window it
    /// splits it and takes the half it fell on; on a split one, a new tab of that side.
    fn dropped(&mut self, pane: u64, half: Option<usize>) {
        let target = self.dragging.take();
        self.end_drag();
        let (Some(target), Some(p)) = (target, self.panes.iter().position(|x| x.id == pane)) else { return };
        if self.panes.len() == 2 {
            self.focused = p;
            self.open_in(target, Place::NewTab);
            return;
        }
        self.focused = 0;
        self.open_in(target, Place::Beside);
        if half == Some(0) {
            // The new side was added on the right; the file goes on the left.
            self.panes.swap(0, 1);
            for pane in &self.panes {
                self.split.remove(&pane.root);
            }
            self.split.pack1(&self.panes[0].root, true, false);
            self.split.pack2(&self.panes[1].root, true, false);
            self.focused = 0;
            self.focus_changed();
        }
    }

    /// Runs a page function on the focused side, giving that page the keys.
    fn page_call(&self, code: &str) {
        if let Some(view) = self.view() {
            view.webview().grab_focus();
        }
        self.eval(code);
    }

    fn close_tab(&mut self, p: usize, i: usize) {
        let tab = self.panes[p].tabs.remove(i);
        self.drop_view(tab);
        if self.panes[p].tabs.is_empty() {
            if self.panes.len() > 1 {
                self.remove_pane(p);
                self.show_pane(0);
                self.focus_changed();
                self.focus_pane(0, true);
                return;
            }
            let id = self.new_id();
            self.panes[p].tabs.push(Tab::new(id, String::new()));
        }
        let pane = &mut self.panes[p];
        if i < pane.active || pane.active >= pane.tabs.len() {
            pane.active = pane.active.saturating_sub(1);
        }
        self.show_pane(p);
        self.focus_changed();
        if p == self.focused {
            self.focus_pane(p, true);
        }
    }

    /// Splits the window, with the focused document on both sides, or joins
    /// the two sides keeping every tab on the left.
    fn toggle_split(&mut self) {
        if self.panes.len() == 1 {
            let target = self.pane().tab().target.clone();
            self.open_in(target, Place::Beside);
            return;
        }
        let right_active = self.panes[1].active;
        let was_right = self.focused == 1;
        let moved = std::mem::take(&mut self.panes[1].tabs);
        let offset = self.panes[0].tabs.len();
        // Views move to the left side's container; WebKit keeps the page loaded.
        for tab in &moved {
            if let Some((_, view)) = &tab.view {
                let widget = view.webview();
                self.panes[1].views.remove(&widget);
                self.panes[0].views.pack_start(&widget, true, true, 0);
            }
        }
        self.panes[0].tabs.extend(moved);
        self.remove_pane(1);
        if was_right {
            self.panes[0].active = offset + right_active;
        }
        self.show_pane(0);
        self.focus_changed();
        self.focus_pane(0, true);
    }

    fn fullscreen(&mut self, view: u64, on: bool) {
        let side = self.sidebar.webview();
        let holder = self.find_view(view).map(|(p, _)| p);
        if on {
            self.sidebar_before_fullscreen.get_or_insert(side.is_visible());
            side.hide();
            for (p, pane) in self.panes.iter().enumerate() {
                pane.strip.hide();
                pane.bar.root.hide();
                pane.root.set_visible(Some(p) == holder);
            }
        } else {
            for pane in &self.panes {
                pane.strip.show();
                pane.bar.root.set_visible(self.config.bar_shown);
                pane.root.show();
            }
            if self.sidebar_before_fullscreen.take().unwrap_or(true) {
                side.show();
            }
        }
    }

    fn navigated(&mut self, view: u64, url: String) {
        let Some((p, i)) = self.find_view(view) else { return };
        if !self.panes[p].tabs[i].ready {
            self.panes[p].tabs[i].ready = true;
            self.panes[p].show_active();
            if p == self.focused && self.panes[p].active == i {
                self.focus_pane(p, true);
            }
        }
        let file = vault::rel_from_url(&url);
        {
            let mut shared = self.shared.lock().unwrap();
            if let Some(page) = shared.pages.get_mut(&view) {
                page.file = file.clone();
                page.url = (file.is_none() && is_web(&url)).then(|| url.clone());
                page.loaded = false;
                page.console.clear();
                page.orphans = None;
            }
        }
        let tab = &mut self.panes[p].tabs[i];
        tab.target = file.clone().or_else(|| is_web(&url).then(|| url.clone())).unwrap_or_default();
        tab.title = None;
        let pane = &self.panes[p];
        pane.render(p == self.focused);
        if pane.active == i {
            pane.bar.show(&pane.tab().target);
            self.update_nav(p);
        }
        self.publish();
        if p == self.focused && self.panes[p].active == i {
            self.sidebar_js(format!("app.current({})", json!(file)));
        }
    }

    fn pick_vault(&self) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().set_title("Abrir carpeta como bóveda").pick_folder();
            let picked = picked.and_then(|p| p.canonicalize().ok());
            let _ = proxy.send_event(UserEvent::Picked(picked));
        });
    }

    fn on_sidebar(&mut self, msg: SidebarMsg) {
        match msg {
            // Scripts sent before the sidebar loaded were lost, so send its state again.
            SidebarMsg::Ready => {
                self.apply_theme();
                self.refresh_sidebar();
            }
            SidebarMsg::PickVault => self.pick_vault(),
            SidebarMsg::SwitchVault { path } => self.show_vault(Some(path)),
            SidebarMsg::Open { path, tab, beside } => {
                let place = if beside { Place::Beside } else if tab { Place::NewTab } else { Place::Here };
                self.open_in(path, place);
            }
            SidebarMsg::CreateNote { dir, name } => match self.create_note(&dir, &name) {
                Ok(()) => self.sidebar_js("app.noteCreated()".into()),
                Err(e) => self.sidebar_js(format!("app.noteError({})", json!(e))),
            },
            SidebarMsg::CreateFolder { dir, name } => match self.create_folder(&dir, &name) {
                Ok(()) => self.sidebar_js("app.noteCreated()".into()),
                Err(e) => self.sidebar_js(format!("app.noteError({})", json!(e))),
            },
            SidebarMsg::Trash { path } => {
                let result = self.vault().ok_or("No hay ninguna bóveda abierta".into()).and_then(|root| files::trash(&root, &path));
                match result {
                    Ok(()) => {
                        self.sidebar_js(format!("app.notice({}, false)", json!(format!("«{path}» está en la papelera."))));
                        self.retarget(&path, None);
                    }
                    Err(e) => self.sidebar_js(format!("app.notice({}, true)", json!(e))),
                }
            }
            SidebarMsg::Move { path, dir } => {
                let result = self.vault().ok_or("No hay ninguna bóveda abierta".into()).and_then(|root| files::move_to(&root, &path, &dir));
                match result {
                    Ok(Some(new)) => {
                        let root = self.vault().unwrap_or_default();
                        match links::relink(&root, &path, &new) {
                            Ok(changed) if !changed.is_empty() => {
                                let n = changed.len();
                                let text = if n == 1 { "Se actualizaron los enlaces de 1 nota.".to_string() } else { format!("Se actualizaron los enlaces de {n} notas.") };
                                self.sidebar_js(format!("app.notice({}, false)", json!(text)));
                            }
                            Ok(_) => {}
                            Err(e) => self.sidebar_js(format!("app.notice({}, true)", json!(e))),
                        }
                        self.retarget(&path, Some(&new));
                    }
                    Ok(None) => {}
                    Err(e) => self.sidebar_js(format!("app.notice({}, true)", json!(e))),
                }
            }
            SidebarMsg::DragStart { path } => {
                let split = self.panes.len() == 2;
                for pane in &self.panes {
                    pane.show_drop(true, split);
                }
                self.dragging = Some(path);
            }
            SidebarMsg::DragEnd => self.end_drag(),
            SidebarMsg::RemoveVault { path } => {
                self.config.vaults.retain(|v| *v != path);
                self.config.last.remove(&path);
                self.config.workspaces.remove(&path);
                if self.vault().as_deref() == Some(&path) {
                    self.config.current = self.config.vaults.first().cloned();
                    self.config.save();
                    // The removed vault must not get its workspace saved back.
                    self.shared.lock().unwrap().vault = None;
                    let next = self.config.current.clone();
                    self.show_vault(next);
                } else {
                    self.config.save();
                    self.refresh_sidebar();
                }
            }
        }
    }

    /// Points the tabs showing `old`, or a file inside the folder `old`, at
    /// where it moved, or at the empty page if it was deleted.
    fn retarget(&mut self, old: &str, new: Option<&str>) {
        let inside = |t: &str| t == old || t.starts_with(&format!("{old}/"));
        let mut hidden_changed = false;
        for p in 0..self.panes.len() {
            for i in 0..self.panes[p].tabs.len() {
                let target = self.panes[p].tabs[i].target.clone();
                if !inside(&target) {
                    continue;
                }
                let next = new.map_or(String::new(), |n| format!("{n}{}", &target[old.len()..]));
                let tab = &mut self.panes[p].tabs[i];
                tab.target = next.clone();
                tab.position = None;
                tab.restore = None;
                match &self.panes[p].tabs[i].view {
                    Some((_, view)) => self.load_into(view, &next, ""),
                    None => hidden_changed = true,
                }
            }
            self.panes[p].render(p == self.focused);
        }
        if hidden_changed {
            self.publish();
        }
    }

    /// Only vault pages may log or send events; the empty page may only ask for a folder.
    /// A website could otherwise feed instructions to the agents that read them.
    fn on_page(&mut self, view: u64, url: &str, msg: PageMsg) {
        let trusted = vault::rel_from_url(url).is_some();
        let reply = |app: &App, code: String| {
            if let Some(v) = app.view_of(view) {
                let _ = v.evaluate_script(&code);
            }
        };
        match msg {
            PageMsg::PickVault if trusted || url.starts_with("about:") => self.pick_vault(),
            PageMsg::SaveNote { text } if trusted => {
                let result = self.save_note(url, &text);
                let (ok, error) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };
                reply(self, format!("window.__visorNote && window.__visorNote.saved({ok}, {})", json!(error)));
            }
            PageMsg::ToggleTask { offset, checked } if trusted => {
                let error = self.toggle_task(url, offset, checked).err().unwrap_or_default();
                reply(self, format!("window.__visorNote && window.__visorNote.toggled({})", json!(error)));
            }
            PageMsg::AnnotSave { annotation } if trusted => {
                if let Err(e) = self.with_page_annotations(url, |root, page| annot::upsert(root, page, &annotation)) {
                    eprintln!("No se pudo guardar la anotación: {e}");
                }
            }
            PageMsg::AnnotDelete { id } if trusted => {
                if let Err(e) = self.with_page_annotations(url, |root, page| annot::delete(root, page, &id)) {
                    eprintln!("No se pudo borrar la anotación: {e}");
                }
            }
            PageMsg::AnnotStatus { orphans, total } if trusted => {
                if let Some(page) = self.shared.lock().unwrap().pages.get_mut(&view) {
                    page.orphans = (total > 0).then_some(orphans);
                }
            }
            PageMsg::CreateNote { dir, name } if trusted => {
                if let Err(e) = self.create_note(&dir, &name) {
                    eprintln!("No se pudo crear la nota: {e}");
                }
            }
            PageMsg::Console { level, text } if trusted => {
                let mut shared = self.shared.lock().unwrap();
                let Some(page) = shared.pages.get_mut(&view) else { return };
                if page.console.len() >= MAX_CONSOLE {
                    page.console.remove(0);
                }
                // Vault paths read better than server URLs, and keep the secret out of logs.
                let text = text.replace(vault::base(), "");
                page.console.push(json!({ "time": now(), "level": level, "text": text }));
            }
            PageMsg::Event { data } if trusted => {
                let mut shared = self.shared.lock().unwrap();
                let event = json!({
                    "id": shared.next_id,
                    "time": now(),
                    "vault": shared.vault,
                    "file": shared.page_of(view).file,
                    "data": data,
                });
                shared.next_id += 1;
                if shared.events.len() >= MAX_EVENTS {
                    shared.events.remove(0);
                }
                shared.events.push(event.clone());
                append_event(&event);
            }
            _ => {}
        }
    }

    /// Runs an annotation change for the page at `url`, which must be a real
    /// file of the open vault: the page cannot name any other.
    fn with_page_annotations(&self, url: &str, change: impl FnOnce(&Path, &str) -> Result<(), String>) -> Result<(), String> {
        let root = self.vault().ok_or("No hay ninguna bóveda abierta")?;
        let page = vault::rel_from_url(url).ok_or("La página no es de la bóveda")?;
        vault::resolve(&root, &page).ok_or("La página no existe en la bóveda")?;
        change(&root, &page)
    }

    /// Writes a note's text. Only the note itself may save, and only Markdown:
    /// an HTML page in the vault cannot overwrite files through this.
    fn save_note(&mut self, url: &str, text: &str) -> Result<(), String> {
        let root = self.vault().ok_or("No hay ninguna bóveda abierta")?;
        let rel = vault::rel_from_url(url).ok_or("La página no es de la bóveda")?;
        let path = vault::resolve(&root, &rel).filter(|p| vault::is_markdown(p)).ok_or("Solo se pueden guardar notas .md")?;
        self.own_write = Some((path.clone(), hash(text.as_bytes())));
        write_atomic(&path, text).map_err(|e| e.to_string())
    }

    /// Ticks or unticks one task of the note at `url`. Rust re-reads the file and
    /// checks that a task really starts at `offset`, so the page cannot write anything else.
    fn toggle_task(&mut self, url: &str, offset: usize, checked: bool) -> Result<(), String> {
        let root = self.vault().ok_or("No hay ninguna bóveda abierta")?;
        let rel = vault::rel_from_url(url).ok_or("La página no es de la bóveda")?;
        let path = vault::resolve(&root, &rel).filter(|p| vault::is_markdown(p)).ok_or("Solo se pueden cambiar notas .md")?;
        let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let text = crate::markdown::toggle_task(&source, offset, checked).ok_or("La tarea ya no está en la nota")?;
        self.save_note(url, &text)
    }

    /// The vault folder `dir` and a checked `name` for something new inside it.
    fn new_in<'a>(&self, dir: &str, name: &'a str) -> Result<(PathBuf, &'a str), String> {
        let root = self.vault().ok_or("No hay ninguna bóveda abierta")?;
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            return Err("El nombre no puede estar vacío, empezar por punto ni contener «/».".into());
        }
        let folder = if dir.is_empty() { root.canonicalize().map_err(|e| e.to_string())? } else {
            let folder = root.join(dir).canonicalize().map_err(|_| format!("No existe la carpeta {dir}"))?;
            if !folder.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
                return Err("La carpeta está fuera de la bóveda.".into());
            }
            folder
        };
        Ok((folder, name))
    }

    fn create_folder(&mut self, dir: &str, name: &str) -> Result<(), String> {
        let (folder, name) = self.new_in(dir, name)?;
        let path = folder.join(name);
        if path.exists() {
            return Err(format!("Ya existe {name}"));
        }
        fs::create_dir(&path).map_err(|e| e.to_string())
    }

    /// Creates `name`.md in `dir` and opens it in the editor.
    fn create_note(&mut self, dir: &str, name: &str) -> Result<(), String> {
        let root = self.vault().ok_or("No hay ninguna bóveda abierta")?;
        let (folder, name) = self.new_in(dir, name)?;
        let file = if name.to_lowercase().ends_with(".md") { name.to_string() } else { format!("{name}.md") };
        let path = folder.join(&file);
        if path.exists() {
            return Err(format!("Ya existe {file}"));
        }
        let title = file.trim_end_matches(".md");
        let text = format!("# {title}\n\n");
        self.own_write = Some((path.clone(), hash(text.as_bytes())));
        fs::write(&path, text).map_err(|e| e.to_string())?;
        let rel = vault::rel(&root.canonicalize().map_err(|e| e.to_string())?, &path);
        // The name was typed in the sidebar; writing continues in the note.
        self.open_with(rel, Place::Here, "#editar");
        Ok(())
    }

    /// Opens a folder as vault or a file inside the vault that contains it.
    fn open(&mut self, path: PathBuf, raw: &str, place: Place) -> Result<Option<Loading>, String> {
        let current = self.vault();
        let path = path
            .canonicalize()
            .or_else(|_| current.as_ref().ok_or(()).and_then(|v| v.join(raw).canonicalize().map_err(|_| ())))
            .map_err(|_| format!("No existe: {raw}"))?;
        if path.is_dir() {
            self.show_vault(Some(path));
            return Ok(None);
        }
        let root = current
            .filter(|v| path.starts_with(v))
            .or_else(|| self.config.vault_for(&path))
            .or_else(|| path.parent().map(Path::to_path_buf))
            .ok_or_else(|| format!("Sin carpeta para {}", path.display()))?;
        let rel = vault::rel(&root, &path);
        if self.vault().as_ref() != Some(&root) {
            self.show_vault(Some(root));
        }
        // A lone empty tab, as in a vault opened for the first time, is used rather than kept.
        let lone_empty = self.panes.len() == 1 && self.pane().tabs.len() == 1 && self.pane().tab().target.is_empty();
        Ok(self.open_in(rel, if lone_empty { Place::Here } else { place }))
    }

    fn vault_changed(&mut self, changed: Vec<PathBuf>) {
        let Some(root) = self.vault() else { return };
        // An agent replied or the page saved: the layer re-reads its notes.
        for view in self.live_views() {
            let file = vault::rel_from_url(&view.url().unwrap_or_default());
            if file.is_some() && changed.iter().any(|p| annot::page_of(&root, p) == file) {
                let _ = view.evaluate_script("window.__visorAnnot && window.__visorAnnot.reload()");
            }
        }
        let changed: Vec<_> = changed.into_iter().filter(|p| !vault::ignored(&root, p)).collect();
        if changed.is_empty() {
            return;
        }
        self.sidebar_js(format!("app.setTree({})", json!(vault::tree(&root))));
        // What the app just saved itself is already on screen.
        let own = |p: &PathBuf| {
            self.own_write.as_ref().is_some_and(|(o, h)| o == p && fs::read(p).is_ok_and(|b| hash(&b) == *h))
        };
        let changed: Vec<_> = changed.into_iter().filter(|p| !own(p)).collect();
        // Pages may load sibling CSS, JS or data, so any change outside pages reloads too.
        let other = changed.iter().any(|p| !vault::is_page(p) && !p.is_dir());
        for pane in &self.panes {
            for tab in &pane.tabs {
                let Some((_, view)) = &tab.view else { continue };
                if tab.target.is_empty() {
                    // On the empty page the message depends on whether the vault has files.
                    self.load_into(view, "", "");
                    continue;
                }
                if is_web(&tab.target) {
                    continue;
                }
                let current = root.join(&tab.target);
                if !(other || changed.contains(&current)) {
                    continue;
                }
                if vault::is_markdown(&current) {
                    // A note being edited must not lose its text to a reload.
                    let _ = view.evaluate_script("window.__visorNote ? window.__visorNote.changed() : location.reload()");
                } else {
                    let _ = view.reload();
                }
            }
        }
    }
}

/// A reading position as a page reported it, keeping only its numbers: the
/// page may be a website, and the value is later written into a script.
fn clean_position(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json).ok()?;
    let y = value["y"].as_f64().filter(|y| y.is_finite())?;
    Some(match value["line"].as_u64() {
        Some(line) => json!({ "line": line, "y": y }).to_string(),
        None => json!({ "y": y }).to_string(),
    })
}

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// Writes through a temporary file, so a crash never leaves a half-written note.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let name = path.file_name().map_or("nota".into(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.visor-tmp"));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

fn append_event(event: &Value) {
    let path = paths::events();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{event}");
    }
}
