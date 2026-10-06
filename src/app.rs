use crate::annot;
use crate::bar::Bar;
use crate::paths;
use crate::theme::{self, Theme};
use crate::vault::{self, Config};
use gtk::prelude::*;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
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

pub type Reply = mpsc::Sender<Result<Option<String>, String>>;

pub enum UserEvent {
    Sidebar(String),
    /// A message from the page, with the URL of the page that sent it.
    Page { url: String, body: String },
    Navigated(String),
    PageLoaded,
    Open { path: PathBuf, raw: String, reply: Reply },
    OpenUrl { url: String, reply: Reply },
    Picked(Option<PathBuf>),
    VaultChanged(Vec<PathBuf>),
    ThemeChanged,
    MonoFont(Option<String>),
    Fullscreen(bool),
    Action(Action),
}

pub enum Action {
    PickVault,
    ToggleSidebar,
    Reload,
    Back,
    Forward,
    FocusAddress,
    LeaveAddress,
    NewNote,
    ToggleEdit,
    PickElement,
    ToggleAnnotations,
    Go(String),
}

/// What the socket server can read without going through the event loop.
#[derive(Default)]
pub struct Shared {
    pub vault: Option<PathBuf>,
    pub file: Option<String>,
    /// Set when the page shows a website instead of a vault file.
    pub url: Option<String>,
    pub loaded: bool,
    pub load_seq: u64,
    pub console: Vec<Value>,
    pub events: Vec<Value>,
    pub next_id: u64,
    /// Annotations of the current page that could not be placed, as reported by it.
    pub orphans: Option<Vec<String>>,
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

    pub fn status(&self) -> Value {
        let path = match (&self.vault, &self.file) {
            (Some(v), Some(f)) => Some(v.join(f)),
            _ => None,
        };
        let errors = self.console.iter().filter(|e| e["level"] == "error").count();
        json!({
            "ok": true,
            "vault": self.vault,
            "file": self.file,
            "path": path,
            "url": self.url,
            "loaded": self.loaded,
            "errors": errors,
            "last_event_id": self.next_id - 1,
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
    Open { path: String },
    CreateNote { dir: String, name: String },
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

    // Layout: [sidebar | bar over page], split by a draggable divider.
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_position(config.sidebar_width.unwrap_or(SIDEBAR_WIDTH));
    window.default_vbox().expect("vbox de GTK").pack_start(&paned, true, true, 0);
    let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let bar = Bar::new(&proxy);
    right.pack_start(&bar.root, false, false, 0);
    let styles = gtk::CssProvider::new();
    if let Some(screen) = WidgetExt::screen(&paned) {
        gtk::StyleContext::add_provider_for_screen(&screen, &styles, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let background = theme.as_ref().map_or((255, 255, 255, 255), Theme::rgba);
    // One context for both views: one network process, and a single writer
    // for cookies and storage, which two contexts on one folder would fight over.
    let mut context = WebContext::new(None);
    let sidebar = build_sidebar(&paned, &mut context, background, proxy.clone());
    paned.pack2(&right, true, true);
    let page = build_page(&right, &mut context, background, proxy.clone());
    let (sidebar, page) = match (sidebar, page) {
        (Ok(s), Ok(p)) => (s, p),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("No se pudo crear el webview: {e}");
            return 1;
        }
    };
    right.show();
    paned.show();

    let mut app = App {
        sidebar,
        page,
        _context: context,
        mono: None,
        sidebar_before_fullscreen: None,
        own_write: None,
        bar,
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
    app.show_vault(current, None);
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
            _ if k == key::F5 => Action::Reload,
            _ if ctrl && k == key::r => Action::Reload,
            _ if ctrl && k == key::o => Action::PickVault,
            _ if ctrl && k == key::b => Action::ToggleSidebar,
            _ if ctrl && k == key::l => Action::FocusAddress,
            _ if ctrl && k == key::n => Action::NewNote,
            _ if ctrl && k == key::e => Action::ToggleEdit,
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

fn build_page(
    container: &gtk::Box,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: EventLoopProxy<UserEvent>,
) -> wry::Result<WebView> {
    let ipc = proxy.clone();
    let new_window = proxy.clone();
    let page = WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_initialization_script(BRIDGE.replace("__VISOR_BASE__", vault::base()))
        .with_initialization_script(ANNOT.replace("__VISOR_BASE__", vault::base()))
        .with_ipc_handler(move |request: Request<String>| {
            let url = request.uri().to_string();
            let _ = ipc.send_event(UserEvent::Page { url, body: request.into_body() });
        })
        // target="_blank" and window.open stay in this view; there are no tabs.
        .with_new_window_req_handler(move |url, _| {
            if is_web(&url) {
                let _ = new_window.send_event(UserEvent::Action(Action::Go(url)));
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
    view.connect_decide_policy(|_, decision, kind| {
        if kind != PolicyDecisionType::NavigationAction {
            return false;
        }
        let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else { return false };
        let Some(action) = nav.navigation_action() else { return false };
        if action.navigation_type() != NavigationType::LinkClicked {
            return false;
        }
        let uri = action.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
        if is_web(&uri) {
            return false;
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
            LoadEvent::Committed => UserEvent::Navigated(view.uri().map(|u| u.to_string()).unwrap_or_default()),
            LoadEvent::Finished => UserEvent::PageLoaded,
            _ => return,
        };
        let _ = p.send_event(event);
    });

    // WebKit makes the whole window fullscreen; the app hides its own chrome
    // so only the element (a video, usually) remains. Returning false lets
    // WebKit go on with its default handling.
    let p = proxy.clone();
    view.connect_enter_fullscreen(move |_| {
        let _ = p.send_event(UserEvent::Fullscreen(true));
        false
    });
    let p = proxy.clone();
    view.connect_leave_fullscreen(move |_| {
        let _ = p.send_event(UserEvent::Fullscreen(false));
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
    page: WebView,
    _context: WebContext,
    mono: Option<String>,
    /// Whether the sidebar was showing when fullscreen began, to restore it after.
    sidebar_before_fullscreen: Option<bool>,
    /// The last file the app wrote and a hash of what it wrote. A change on disk
    /// that still matches is the app's own; anything else came from outside.
    own_write: Option<(PathBuf, u64)>,
    bar: Bar,
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
        let file = self.shared.lock().unwrap().file.clone();
        self.sidebar_js(format!("app.show({}); app.current({})", self.info(tree), json!(file)));
    }

    fn show_vault(&mut self, root: Option<PathBuf>, open: Option<String>) {
        {
            let mut shared = self.shared.lock().unwrap();
            shared.vault = root.clone();
            shared.file = None;
            shared.loaded = false;
            shared.console.clear();
        }
        self.watcher = None;
        if let Some(root) = &root {
            self.config.add(root);
            self.config.save();
            self.watcher = watch(root, 150, self.proxy.clone(), UserEvent::VaultChanged);
        }
        let tree = root.as_deref().map(vault::tree);
        self.sidebar_js(format!("app.show({})", self.info(tree)));
        let last = root
            .as_ref()
            .and_then(|r| self.config.last.get(r).filter(|f| r.join(f).is_file()))
            .cloned();
        match open.or(last) {
            Some(rel) => self.load(rel),
            None => self.empty(),
        }
    }

    fn load(&mut self, rel: String) {
        if let Some(root) = self.vault() {
            self.config.last.insert(root, rel.clone());
            self.config.save();
        }
        let _ = self.page.load_url(&vault::url(&rel));
        self.sidebar_js(format!("app.current({})", json!(rel)));
    }

    fn empty(&mut self) {
        {
            let mut shared = self.shared.lock().unwrap();
            shared.file = None;
            shared.url = None;
        }
        let message = match self.vault() {
            None => "Abre una carpeta con archivos HTML para empezar.",
            Some(root) => {
                let tree = json!(vault::tree(&root));
                let none = |key: &str| tree[key].as_array().is_some_and(Vec::is_empty);
                if none("files") && none("dirs") {
                    "Esta carpeta todavía no tiene archivos HTML. Aparecerán aquí en cuanto se creen."
                } else {
                    "Elige un archivo de la lista."
                }
            }
        };
        let _ = self.page.load_html(&empty_page(message, self.vault().is_none()));
        self.sidebar_js("app.current(null)".into());
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
            let _ = self.page.load_url(text);
        } else if let Some(rel) = in_vault {
            self.load(rel);
        } else if !text.contains(char::is_whitespace) && text.contains('.') {
            let _ = self.page.load_url(&format!("https://{text}"));
        } else {
            self.bar.address.style_context().add_class("error");
            return;
        }
        self.page.webview().grab_focus();
    }

    fn update_nav(&self) {
        let view = self.page.webview();
        self.bar.back.set_sensitive(view.can_go_back());
        self.bar.forward.set_sensitive(view.can_go_forward());
    }

    fn apply_theme(&mut self) {
        self.theme = Theme::load(self.mono.clone());
        if let Some(manager) = self.page.webview().user_content_manager() {
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
        let css = self.theme.as_ref().map_or(String::new(), |t| {
            format!(
                "paned > separator {{ background: {}; background-image: none; min-width: 1px; }}\n{}",
                t.tone(14.0),
                Bar::css(t)
            )
        });
        let _ = self.styles.load_from_data(css.as_bytes());
        if let Some(theme) = &self.theme {
            let _ = self.sidebar.set_background_color(theme.rgba());
            let _ = self.page.set_background_color(theme.rgba());
        }
        let json = self.theme.as_ref().map_or(Value::Null, Theme::json);
        self.sidebar_js(format!("app.setTheme({json})"));
    }

    fn save_layout(&mut self) {
        if self.sidebar.webview().is_visible() {
            self.config.sidebar_width = Some(self.paned.position());
            self.config.save();
        }
    }

    fn handle(&mut self, event: UserEvent) {
        match event {
            UserEvent::Sidebar(body) => match serde_json::from_str(&body) {
                Ok(msg) => self.on_sidebar(msg),
                Err(e) => eprintln!("Mensaje no válido del panel: {e}"),
            },
            UserEvent::Page { url, body } => {
                if let Ok(msg) = serde_json::from_str(&body) {
                    self.on_page(&url, msg);
                }
            }
            UserEvent::Navigated(url) => self.navigated(url),
            UserEvent::PageLoaded => {
                {
                    let mut shared = self.shared.lock().unwrap();
                    shared.loaded = true;
                    shared.load_seq += 1;
                }
                self.update_nav();
            }
            UserEvent::Open { path, raw, reply } => {
                let _ = reply.send(self.open(path, &raw));
            }
            UserEvent::OpenUrl { url, reply } => {
                let _ = self.page.load_url(&url);
                let _ = reply.send(Ok(Some(url)));
            }
            UserEvent::Picked(Some(path)) => self.show_vault(Some(path), None),
            UserEvent::Picked(None) => {}
            UserEvent::VaultChanged(changed) => self.vault_changed(changed),
            UserEvent::ThemeChanged => self.apply_theme(),
            UserEvent::Fullscreen(on) => self.fullscreen(on),
            UserEvent::MonoFont(font) => {
                self.mono = font;
                self.apply_theme();
            }
            UserEvent::Action(action) => self.act(action),
        }
    }

    fn act(&mut self, action: Action) {
        let view = self.page.webview();
        match action {
            Action::Reload => {
                let _ = self.page.reload();
            }
            Action::Back => view.go_back(),
            Action::Forward => view.go_forward(),
            Action::PickVault => self.pick_vault(),
            Action::ToggleSidebar => {
                let side = self.sidebar.webview();
                if side.is_visible() {
                    self.save_layout();
                }
                side.set_visible(!side.is_visible());
            }
            Action::FocusAddress => self.bar.address.grab_focus(),
            Action::LeaveAddress => {
                view.grab_focus();
                self.bar.show(&self.address_text());
            }
            Action::Go(text) => self.go(&text),
            Action::PickElement => {
                view.grab_focus();
                let _ = self.page.evaluate_script("window.__visorAnnot && window.__visorAnnot.pickElement()");
            }
            Action::ToggleAnnotations => {
                view.grab_focus();
                let _ = self.page.evaluate_script("window.__visorAnnot && window.__visorAnnot.togglePanel()");
            }
            Action::ToggleEdit => {
                view.grab_focus();
                let _ = self.page.evaluate_script("window.__visorNote && window.__visorNote.toggle()");
            }
            Action::NewNote => {
                if self.vault().is_none() {
                    return;
                }
                let dir = self.shared.lock().unwrap().file.as_deref().and_then(|f| f.rsplit_once('/')).map_or("", |(d, _)| d).to_string();
                let side = self.sidebar.webview();
                side.show();
                side.grab_focus();
                self.sidebar_js(format!("app.newNote({})", json!(dir)));
            }
        }
    }

    fn fullscreen(&mut self, on: bool) {
        let side = self.sidebar.webview();
        if on {
            self.sidebar_before_fullscreen.get_or_insert(side.is_visible());
            self.bar.root.hide();
            side.hide();
        } else {
            self.bar.root.show();
            if self.sidebar_before_fullscreen.take().unwrap_or(true) {
                side.show();
            }
        }
    }

    fn address_text(&self) -> String {
        let shared = self.shared.lock().unwrap();
        shared.file.clone().or_else(|| shared.url.clone()).unwrap_or_default()
    }

    fn navigated(&mut self, url: String) {
        let file = vault::rel_from_url(&url);
        {
            let mut shared = self.shared.lock().unwrap();
            shared.file = file.clone();
            shared.url = (file.is_none() && is_web(&url)).then(|| url.clone());
            shared.loaded = false;
            shared.console.clear();
            shared.orphans = None;
        }
        if let (Some(root), Some(rel)) = (self.vault(), &file) {
            self.config.last.insert(root, rel.clone());
        }
        self.sidebar_js(format!("app.current({})", json!(file)));
        self.bar.show(&self.address_text());
        self.update_nav();
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
            SidebarMsg::SwitchVault { path } => self.show_vault(Some(path), None),
            SidebarMsg::Open { path } => self.load(path),
            SidebarMsg::CreateNote { dir, name } => match self.create_note(&dir, &name) {
                Ok(()) => self.sidebar_js("app.noteCreated()".into()),
                Err(e) => self.sidebar_js(format!("app.noteError({})", json!(e))),
            },
            SidebarMsg::RemoveVault { path } => {
                self.config.vaults.retain(|v| *v != path);
                self.config.last.remove(&path);
                if self.vault().as_deref() == Some(&path) {
                    self.config.current = self.config.vaults.first().cloned();
                    self.config.save();
                    let next = self.config.current.clone();
                    self.show_vault(next, None);
                } else {
                    self.config.save();
                    self.refresh_sidebar();
                }
            }
        }
    }

    /// Only vault pages may log or send events; the empty page may only ask for a folder.
    /// A website could otherwise feed instructions to the agents that read them.
    fn on_page(&mut self, url: &str, msg: PageMsg) {
        let trusted = vault::rel_from_url(url).is_some();
        match msg {
            PageMsg::PickVault if trusted || url.starts_with("about:") => self.pick_vault(),
            PageMsg::SaveNote { text } if trusted => {
                let result = self.save_note(url, &text);
                let (ok, error) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };
                let _ = self.page.evaluate_script(&format!("window.__visorNote && window.__visorNote.saved({ok}, {})", json!(error)));
            }
            PageMsg::ToggleTask { offset, checked } if trusted => {
                let error = self.toggle_task(url, offset, checked).err().unwrap_or_default();
                let _ = self.page.evaluate_script(&format!("window.__visorNote && window.__visorNote.toggled({})", json!(error)));
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
                self.shared.lock().unwrap().orphans = (total > 0).then_some(orphans);
            }
            PageMsg::CreateNote { dir, name } if trusted => {
                if let Err(e) = self.create_note(&dir, &name) {
                    eprintln!("No se pudo crear la nota: {e}");
                }
            }
            PageMsg::Console { level, text } if trusted => {
                let mut shared = self.shared.lock().unwrap();
                if shared.console.len() >= MAX_CONSOLE {
                    shared.console.remove(0);
                }
                // Vault paths read better than server URLs, and keep the secret out of logs.
                let text = text.replace(vault::base(), "");
                shared.console.push(json!({ "time": now(), "level": level, "text": text }));
            }
            PageMsg::Event { data } if trusted => {
                let mut shared = self.shared.lock().unwrap();
                let event = json!({
                    "id": shared.next_id,
                    "time": now(),
                    "vault": shared.vault,
                    "file": shared.file,
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

    /// Creates `name`.md in `dir` and opens it in the editor.
    fn create_note(&mut self, dir: &str, name: &str) -> Result<(), String> {
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
        self.config.last.insert(root, rel.clone());
        let _ = self.page.load_url(&format!("{}#editar", vault::url(&rel)));
        // The name was typed in the sidebar; writing continues in the note.
        self.page.webview().grab_focus();
        self.sidebar_js(format!("app.current({})", json!(rel)));
        Ok(())
    }

    /// Opens a folder as vault or a file inside the vault that contains it.
    fn open(&mut self, path: PathBuf, raw: &str) -> Result<Option<String>, String> {
        let current = self.vault();
        let path = path
            .canonicalize()
            .or_else(|_| current.as_ref().ok_or(()).and_then(|v| v.join(raw).canonicalize().map_err(|_| ())))
            .map_err(|_| format!("No existe: {raw}"))?;
        if path.is_dir() {
            self.show_vault(Some(path), None);
            return Ok(None);
        }
        let root = current
            .filter(|v| path.starts_with(v))
            .or_else(|| self.config.vault_for(&path))
            .or_else(|| path.parent().map(Path::to_path_buf))
            .ok_or_else(|| format!("Sin carpeta para {}", path.display()))?;
        let rel = vault::rel(&root, &path);
        if self.vault().as_ref() == Some(&root) {
            self.load(rel.clone());
        } else {
            self.show_vault(Some(root), Some(rel.clone()));
        }
        Ok(Some(rel))
    }

    fn vault_changed(&mut self, changed: Vec<PathBuf>) {
        let Some(root) = self.vault() else { return };
        // An agent replied or the page saved: the layer re-reads its notes.
        let current = self.shared.lock().unwrap().file.clone();
        if current.is_some() && changed.iter().any(|p| annot::page_of(&root, p) == current) {
            let _ = self.page.evaluate_script("window.__visorAnnot && window.__visorAnnot.reload()");
        }
        let changed: Vec<_> = changed.into_iter().filter(|p| !vault::ignored(&root, p)).collect();
        if changed.is_empty() {
            return;
        }
        self.sidebar_js(format!("app.setTree({})", json!(vault::tree(&root))));
        let (file, url) = {
            let shared = self.shared.lock().unwrap();
            (shared.file.clone(), shared.url.clone())
        };
        let Some(current) = file.map(|f| root.join(f)) else {
            // On the empty page the message depends on whether the vault has files.
            if url.is_none() {
                self.empty();
            }
            return;
        };
        // What the app just saved itself is already on screen.
        let own = |p: &PathBuf| {
            self.own_write.as_ref().is_some_and(|(o, h)| o == p && fs::read(p).is_ok_and(|b| hash(&b) == *h))
        };
        let changed: Vec<_> = changed.into_iter().filter(|p| !own(p)).collect();
        // Pages may load sibling CSS, JS or data, so any change outside pages reloads too.
        let reload = changed.iter().any(|p| *p == current || (!vault::is_page(p) && !p.is_dir()));
        if reload && vault::is_markdown(&current) {
            // A note being edited must not lose its text to a reload.
            let _ = self.page.evaluate_script("window.__visorNote ? window.__visorNote.changed() : location.reload()");
        } else if reload {
            let _ = self.page.reload();
        }
    }
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
