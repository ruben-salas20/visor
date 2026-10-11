use crate::vault::annot;
use crate::session::{Loading, MAX_CONSOLE, Page, Place, AgentRequest, Shared, SharedRef, now, push_event};
use crate::ui::action::{Action, SidebarMsg, UserEvent};
use crate::ui::bar::Bar;
use crate::vault::config::{Config, PaneState, Workspace};
use crate::vault::files;
use crate::vault::links;
use crate::ui::page_calls;
use crate::ui::page_view::{build_page, clean_position, is_web};
use crate::ui::panes::Pane;
use crate::ui::tabs::{Effect, Tabs};
use crate::agent::socket::{ClaimError, claim};
use crate::paths;
use crate::ui::sidebar::Sidebar;
use crate::ui::theme::{self, Theme};
use crate::ui::watch::{self, LivePage, LooseWatchers, OwnWrite, PageAction, RealDisk, watch};
use crate::vault::{self, Access, Vault};
use gtk::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::cell::Cell;
use std::fs;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use webkit2gtk::{UserContentInjectedFrames, UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebViewExt};
use wry::{WebContext, WebView, WebViewExtUnix};

const SIDEBAR_WIDTH: i32 = 260;
const CONFIG_SAVE_DELAY: Duration = Duration::from_millis(500);
const ZOOM_STEPS: [f64; 13] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

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
    /// The person's reply in the conversation of one of the page's annotations.
    AnnotReply { id: String, text: String },
    /// Which annotations the page could not place.
    AnnotStatus { orphans: Vec<String>, total: usize },
}

pub fn launch() -> i32 {
    let (listener, claim) = match claim(&paths::socket()) {
        Ok(claimed) => claimed,
        Err(ClaimError::Running) => {
            eprintln!("visor ya está abierto.");
            return 0;
        }
        Err(ClaimError::Failed(e)) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut claim = Some(claim);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let shared: SharedRef = Arc::new(Mutex::new(Shared::with_saved_events()));
    let agent_proxy = proxy.clone();
    crate::agent::server::spawn(listener, shared.clone(), move |request| {
        let _ = agent_proxy.send_event(UserEvent::Agent(request));
    });

    let mut config = Config::load();
    let Some(crate::http::Started { port, base, loose_base }) = crate::http::start(config.port, shared.clone()) else {
        eprintln!("No se pudo iniciar el servidor local.");
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
    // WebKitGTK's defaults are decisions here, measured, so this code sets
    // neither: DMA-BUF stays on (off starts ~0.5 s sooner, but Three.js drops
    // from 60 to 26 fps), and the cache stays WebBrowser-sized without a limit
    // (DocumentViewer frees ~700 MB after heavy sites, but CDN libraries then
    // reload slower and fail offline; the disk cache does not slow startup).
    let context = WebContext::new(None);
    let sidebar = Sidebar::new(&proxy);
    sidebar.set_settings(config.theme.as_deref(), config.zoom.unwrap_or(1.0), config.auto_update != Some(false));
    // Started for a page outside every vault: that page alone, without the
    // last vault and its tabs. See cli::start.
    let loose = std::env::var_os("VISOR_LOOSE").is_some();
    if loose {
        sidebar.root.hide();
    }
    paned.pack1(&sidebar.root, false, false);
    paned.pack2(&split, true, true);
    let tabs = Tabs::new();
    let first = Pane::new(tabs.side(0).id(), &proxy);
    first.bar().set_visible(config.bar_shown);
    split.pack1(first.root(), true, false);
    split.show();
    paned.show();

    let mut app = App {
        sidebar,
        context,
        background,
        panes: vec![first],
        tabs,
        views: vec![],
        split,
        place_split,
        mono: None,
        dragging: None,
        sidebar_before_fullscreen: None,
        own_write: None,
        paned,
        styles,
        shared,
        proxy: proxy.clone(),
        config,
        config_dirty: false,
        save_scheduled: false,
        theme,
        watcher: None,
        loose_watchers: LooseWatchers::default(),
        _theme_watcher: theme::dir().and_then(|d| watch(&d, RecursiveMode::Recursive, 300, proxy, |_| UserEvent::ThemeChanged)),
    };
    app.apply_theme();
    // Once the window and the first page exist, so the check never delays them.
    if app.config.auto_update != Some(false) {
        crate::update::spawn();
    }
    // Load the page now instead of after the sidebar: both webviews start in parallel.
    let current = app.config.current.clone().filter(|p| !loose && p.is_dir());
    app.show_vault(current);
    app.draw_strips();
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                app.save_layout();
                app.flush_config();
                if let Some(claim) = claim.take() {
                    claim.release();
                }
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
            _ if ctrl && k == key::comma => Action::Settings,
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
  color: var(--visor-fg, CanvasText); border: 0; box-shadow: 0 0 0 1px light-dark(oklch(0 0 0 / .06), oklch(1 0 0 / .08)), 0 1px 2px -1px light-dark(oklch(0 0 0 / .06), transparent), 0 2px 4px 0 light-dark(oklch(0 0 0 / .04), transparent); }}
button:hover {{ box-shadow: 0 0 0 1px light-dark(oklch(0 0 0 / .08), oklch(1 0 0 / .13)), 0 1px 2px -1px light-dark(oklch(0 0 0 / .08), transparent), 0 2px 4px 0 light-dark(oklch(0 0 0 / .06), transparent); }}
</style><p>{message}</p>{button}"#
    )
}

struct App {
    sidebar: Sidebar,
    context: WebContext,
    background: (u8, u8, u8, u8),
    /// The widgets of the left side and, when split, of the right one. They
    /// follow the sides of `tabs`, which the effects keep in step.
    panes: Vec<Pane>,
    tabs: Tabs,
    /// The live pages' views, by view id.
    views: Vec<(u64, WebView)>,
    split: gtk::Paned,
    /// The share of the width for the left side, applied once the new right side has a size.
    place_split: Rc<Cell<Option<f64>>>,
    mono: Option<String>,
    /// The file being dragged from the sidebar.
    dragging: Option<String>,
    /// Whether the sidebar was showing when fullscreen began, to restore it after.
    sidebar_before_fullscreen: Option<bool>,
    /// The last file the app wrote and a hash of what it wrote. A change on disk
    /// that still matches is the app's own; anything else came from outside.
    own_write: Option<OwnWrite>,
    paned: gtk::Paned,
    styles: gtk::CssProvider,
    shared: SharedRef,
    proxy: EventLoopProxy<UserEvent>,
    config: Config,
    /// Changes in `config` not yet written, and whether a write is already scheduled.
    config_dirty: bool,
    save_scheduled: bool,
    theme: Option<Theme>,
    watcher: Option<RecommendedWatcher>,
    /// One per folder of the loose pages in the tabs, without subfolders.
    loose_watchers: LooseWatchers,
    _theme_watcher: Option<RecommendedWatcher>,
}

impl App {
    fn vault(&self) -> Option<Arc<Vault>> {
        self.shared.lock().unwrap().vault.clone()
    }

    fn open_vault(&self) -> Result<Arc<Vault>, String> {
        self.vault().ok_or_else(|| "No hay ninguna bóveda abierta".into())
    }

    fn pane(&self) -> &Pane {
        &self.panes[self.tabs.focused()]
    }

    /// The view of the focused side's active tab. It is always loaded.
    fn view(&self) -> Option<&WebView> {
        self.view_of(self.tabs.focused_view()?)
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

    fn view_of(&self, view: u64) -> Option<&WebView> {
        self.views.iter().find(|(id, _)| *id == view).map(|(_, v)| v)
    }

    /// Shows the vault list and the tree of the current vault in the sidebar.
    fn show_sidebar(&self) {
        let vault = self.vault();
        let tree = vault.as_ref().map(|v| v.tree());
        let opened = vault.as_ref().and_then(|v| self.config.open_folders.get(v.path())).map_or(&[][..], Vec::as_slice);
        self.sidebar.show(&self.config.vaults, vault.as_ref().map(|v| v.path()), tree, opened);
    }

    fn refresh_sidebar(&self) {
        self.show_sidebar();
        self.sidebar.current(self.focused_file());
    }

    /// Opens a vault and restores its tabs. Without a vault the window shows one empty tab.
    fn show_vault(&mut self, root: Option<PathBuf>) {
        if let Some(Err(e)) = root.as_deref().map(Config::check_vault) {
            eprintln!("{e}");
            return;
        }
        if root.is_some() && self.vault().is_none() {
            // It was hidden for loose pages.
            self.sidebar.root.show();
        }
        self.publish();
        {
            let mut shared = self.shared.lock().unwrap();
            shared.vault = root.as_deref().map(|r| Arc::new(Vault::open(r)));
            shared.pages.clear();
        }
        self.watcher = None;
        if let Some(root) = &root {
            self.config.add(root);
            self.save_config_soon();
            self.watcher = watch(root, RecursiveMode::Recursive, 150, self.proxy.clone(), UserEvent::VaultChanged);
        }
        self.show_sidebar();

        let saved = root.as_deref().and_then(|r| self.saved_workspace(r));
        let effects = self.tabs.reset(saved, |t| root.as_deref().is_some_and(|r| r.join(t).is_file()));
        self.apply(effects);
        self.focus_changed();
    }

    /// The workspace saved for `root`, or one tab with the last file opened
    /// there, from before the workspaces.
    fn saved_workspace(&self, root: &Path) -> Option<Workspace> {
        self.config.workspaces.get(root).cloned().or_else(|| {
            let last = self.config.last.get(root)?.clone();
            Some(Workspace { panes: vec![PaneState { tabs: vec![last], active: 0 }], focused: 0 })
        })
    }

    /// Records the sides and tabs for `visor status` and in the vault's workspace.
    fn publish(&mut self) {
        let focused = self.tabs.focused_view().unwrap_or(0);
        let root = {
            let mut shared = self.shared.lock().unwrap();
            shared.layout = self.tabs.layout();
            shared.focused = focused;
            shared.vault.as_ref().map(|v| v.path().to_path_buf())
        };
        self.watch_loose();
        let Some(root) = root else { return };
        self.config.workspaces.insert(root, self.tabs.workspace());
        self.save_config_soon();
    }

    /// Adds the right side, empty.
    fn add_pane(&mut self, id: u64) {
        let pane = Pane::new(id, &self.proxy);
        pane.bar().set_visible(self.config.bar_shown);
        self.split.pack2(pane.root(), true, false);
        self.place_split.set(Some(self.config.split_ratio.unwrap_or(0.5)));
        self.split.queue_resize();
        self.panes.push(pane);
    }

    fn remove_pane(&mut self, p: usize) {
        self.save_split();
        let pane = self.panes.remove(p);
        self.split.remove(pane.root());
        // The right side moves to the left.
        if p == 0
            && let Some(left) = self.panes.first()
        {
            self.split.remove(left.root());
            self.split.pack1(left.root(), true, false);
        }
    }

    fn swap_panes(&mut self) {
        self.panes.swap(0, 1);
        for pane in &self.panes {
            self.split.remove(pane.root());
        }
        self.split.pack1(self.panes[0].root(), true, false);
        self.split.pack2(self.panes[1].root(), true, false);
    }

    /// Does what the tabs model asks, in order. Returns the load of the first
    /// view it creates or points elsewhere: the one `open` waits for.
    fn apply(&mut self, effects: Vec<Effect>) -> Option<Loading> {
        let mut loading = None;
        for effect in effects {
            match effect {
                Effect::AddSide { id } => self.add_pane(id),
                Effect::RemoveSide { side } => self.remove_pane(side),
                Effect::SwapSides => self.swap_panes(),
                Effect::Create { side, view, target, fragment } => {
                    if self.create_view(side, view, &target, &fragment) {
                        loading.get_or_insert((view, 0));
                    } else {
                        self.tabs.abort_view(view);
                    }
                }
                Effect::Unload { view } => {
                    self.views.retain(|(id, _)| *id != view);
                    self.shared.lock().unwrap().pages.remove(&view);
                }
                Effect::MoveView { view, from, to } => {
                    // Views move to the other side's container; WebKit keeps the page loaded.
                    if let Some(v) = self.view_of(view) {
                        let widget = v.webview();
                        self.panes[from].views().remove(&widget);
                        self.panes[to].views().pack_start(&widget, true, true, 0);
                    }
                }
                Effect::Navigate { view, target, fragment } => {
                    if let Some(v) = self.view_of(view) {
                        let before = self.shared.lock().unwrap().page_of(view).load_seq;
                        self.load_into(v, &target, &fragment);
                        loading.get_or_insert((view, before));
                    }
                }
                Effect::SavePosition { view } => {
                    // Asked while still on screen: the tab may be unloaded later.
                    if let Some(v) = self.view_of(view)
                        && v.webview().is_visible()
                    {
                        let proxy = self.proxy.clone();
                        let _ = v.evaluate_script_with_callback(page_calls::hide(), move |json| {
                            let _ = proxy.send_event(UserEvent::Position { view, json });
                        });
                    }
                }
                Effect::Show { side } => {
                    self.show_active(side);
                    self.panes[side].bar().set_text(self.tabs.side(side).tab().target());
                    self.update_nav(side);
                }
                Effect::Focus => {
                    if let Some(view) = self.view() {
                        view.webview().grab_focus();
                    }
                }
            }
        }
        loading
    }

    /// Builds the hidden view of a tab. False if the system could not.
    fn create_view(&mut self, side: usize, id: u64, target: &str, fragment: &str) -> bool {
        let built = build_page(self.panes[side].views(), &mut self.context, self.background, self.proxy.clone(), id);
        let view = match built {
            Ok(v) => v,
            Err(e) => {
                eprintln!("No se pudo crear el webview: {e}");
                return false;
            }
        };
        view.webview().set_visible(false);
        self.style_view(&view);
        view.webview().set_zoom_level(self.config.zoom.unwrap_or(1.0));
        self.shared.lock().unwrap().pages.insert(id, Page::default());
        self.load_into(&view, target, fragment);
        self.views.push((id, view));
        true
    }

    /// Shows the active view of side `p` and hides the others, which stay loaded.
    /// A new view stays hidden until its page starts loading: on screen,
    /// WebKitGTK blocks the app up to 500 ms waiting for its first paint while
    /// the new process waits for the app, twice per new view (docs/adr/0013).
    fn show_active(&self, p: usize) {
        let side = self.tabs.side(p);
        if !side.tab().is_ready() {
            return;
        }
        for (i, tab) in side.tabs().iter().enumerate() {
            if let Some(view) = tab.view().and_then(|id| self.view_of(id)) {
                view.webview().set_visible(i == side.active());
            }
        }
    }

    /// Draws again the tab strips that changed since the last time, once per event.
    fn draw_strips(&mut self) {
        for (p, pane) in self.panes.iter().enumerate() {
            if self.tabs.take_dirty(p) {
                pane.render(self.tabs.side(p), p == self.tabs.focused());
            }
        }
    }

    fn load_into(&self, view: &WebView, target: &str, fragment: &str) {
        if target.is_empty() {
            let _ = view.load_html(&empty_page(self.empty_message(), self.vault().is_none()));
        } else if is_web(target) {
            let _ = view.load_url(target);
        } else if vault::is_loose(target) {
            if let Some(dir) = Path::new(target).parent() {
                let mut shared = self.shared.lock().unwrap();
                if !shared.loose.iter().any(|d| d == dir) {
                    shared.loose.push(dir.to_path_buf());
                }
            }
            let _ = view.load_url(&format!("{}{fragment}", vault::url(target)));
        } else {
            let _ = view.load_url(&format!("{}{fragment}", vault::url(target)));
        }
    }

    fn empty_message(&self) -> &'static str {
        match self.vault() {
            None => "Abre una carpeta con archivos HTML para empezar.",
            Some(root) => {
                if !root.has_pages() {
                    "Esta carpeta todavía no tiene archivos HTML. Aparecerán aquí en cuanto se creen."
                } else {
                    "Elige un archivo de la lista."
                }
            }
        }
    }

    /// Updates what depends on the focused side: the sidebar and the agents' view.
    fn focus_changed(&mut self) {
        self.publish();
        self.sidebar.current(self.focused_file());
    }

    fn focus_pane(&mut self, p: usize, grab: bool) {
        if self.tabs.focus(p) {
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
        let effects = self.tabs.open(target, place, fragment);
        let loading = self.apply(effects);
        self.focus_changed();
        loading
    }

    /// Resolves what was typed in the address field: a URL, a vault path, an
    /// absolute path, as the field shows for loose pages, or a bare domain.
    fn go(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if vault::is_loose(text) {
            if self.open(PathBuf::from(text), text, Place::Here).is_err() {
                self.pane().bar().set_error();
            }
            return;
        }
        let in_vault = self.vault().and_then(|v| Some(v.rel(&v.resolve(text)?)));
        if is_web(text) {
            self.open_in(text.to_string(), Place::Here);
        } else if let Some(rel) = in_vault {
            self.open_in(rel, Place::Here);
        } else if !text.contains(char::is_whitespace) && text.contains('.') {
            self.open_in(format!("https://{text}"), Place::Here);
        } else {
            self.pane().bar().set_error();
        }
    }

    fn update_nav(&self, p: usize) {
        let Some(view) = self.tabs.side(p).tab().view().and_then(|id| self.view_of(id)) else { return };
        let view = view.webview();
        self.panes[p].bar().set_nav(view.can_go_back(), view.can_go_forward());
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
        self.views.iter().map(|(_, v)| v)
    }

    fn apply_theme(&mut self) {
        self.theme = Theme::load(self.mono.clone(), self.config.theme.as_deref());
        if let Some(theme) = &self.theme {
            self.background = theme.rgba();
        }
        for view in self.live_views() {
            self.style_view(view);
        }
        let css = self.theme.as_ref().map_or(String::new(), |t| {
            format!(
                // GTK3 repaints the window background under the page on every
                // frame of an animation; the page covers it, so it costs ~2% CPU
                // for nothing. The bars and the sidebar paint their own.
                "window.background {{ background: none; }}\n\
                 paned > separator {{ background: {}; background-image: none; min-width: 1px; }}\n{}\n{}\n{}",
                t.tone(14.0),
                Bar::css(t),
                Pane::css(t),
                Sidebar::css(t)
            )
        });
        let _ = self.styles.load_from_data(css.as_bytes());
        self.sidebar.set_theme(self.theme.as_ref());
    }

    /// Tells the open pages of visor that the theme changed, once their
    /// `--visor-*` variables hold the new values. Only pages of visor get it:
    /// `Page::file` is set from the committed address, the one `vault::access`
    /// trusts, and is `None` for websites. Called on changes only, not at
    /// startup, when no page has been opened yet.
    fn announce_theme(&self) {
        let Some(theme) = &self.theme else { return };
        let code = page_calls::theme_changed(theme.dark);
        let shared = self.shared.lock().unwrap();
        for (id, view) in &self.views {
            if shared.page_of(*id).file.is_some() {
                let _ = view.evaluate_script(&code);
            }
        }
    }

    /// Marks the settings as changed and writes them once, shortly after,
    /// instead of on every navigation: the write blocks the window. A crash
    /// loses at most this delay of state.
    fn save_config_soon(&mut self) {
        self.config_dirty = true;
        if !self.save_scheduled {
            self.save_scheduled = true;
            let proxy = self.proxy.clone();
            gtk::glib::timeout_add_local_once(CONFIG_SAVE_DELAY, move || {
                let _ = proxy.send_event(UserEvent::SaveConfig);
            });
        }
    }

    /// Writes the settings now if a change is waiting.
    fn flush_config(&mut self) {
        if std::mem::take(&mut self.config_dirty) {
            self.config.save();
        }
    }

    fn save_layout(&mut self) {
        if self.sidebar.root.is_visible() {
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
        self.dispatch(event);
        self.draw_strips();
    }

    fn dispatch(&mut self, event: UserEvent) {
        match event {
            UserEvent::Sidebar(msg) => self.on_sidebar(msg),
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
                    crate::session::LOADED.notify_all();
                }
                let Some(loaded) = self.tabs.loaded(view) else { return };
                if let Some(position) = &loaded.restore
                    && let Some(v) = self.view_of(view)
                {
                    let _ = v.evaluate_script(&page_calls::place(position));
                }
                if loaded.web {
                    let title = self.view_of(view).and_then(|v| v.webview().title()).map(|t| t.to_string());
                    self.tabs.set_title(view, title);
                }
                self.update_nav(loaded.side);
            }
            UserEvent::Focused(view) => {
                if let Some((p, true)) = self.tabs.locate(view) {
                    self.focus_pane(p, false);
                }
            }
            UserEvent::Agent(AgentRequest::Open { path, raw, place, reply }) => {
                let _ = reply.send(self.open(path, &raw, place));
            }
            UserEvent::Agent(AgentRequest::OpenUrl { url, place, reply }) => {
                let _ = reply.send(Ok(self.open_in(url, place)));
            }
            UserEvent::Agent(AgentRequest::Close { target, reply }) => {
                let _ = reply.send(self.close_for_agent(target));
            }
            UserEvent::Agent(AgentRequest::Reload) => self.act(Action::Reload),
            UserEvent::Picked(Some(path)) => self.show_vault(Some(path)),
            UserEvent::Picked(None) => {}
            UserEvent::VaultChanged(changed) => self.vault_changed(changed),
            UserEvent::LooseChanged(changed) => self.loose_changed(changed),
            UserEvent::ThemeChanged => {
                self.apply_theme();
                self.announce_theme();
            }
            UserEvent::Fullscreen { view, on } => self.fullscreen(view, on),
            UserEvent::OpenTab { view, url, background } => self.open_tab(view, &url, background),
            UserEvent::Dropped { pane, half } => self.dropped(pane, half),
            UserEvent::DropHover { pane, zone } => {
                if let Some(p) = self.panes.iter().position(|x| x.id() == pane) {
                    self.frost(p, zone);
                }
            }
            UserEvent::Position { view, json } => {
                self.tabs.set_position(view, clean_position(&json));
            }
            UserEvent::MonoFont(font) => {
                self.mono = font;
                self.apply_theme();
            }
            UserEvent::Action(action) => self.act(action),
            UserEvent::SaveConfig => {
                self.save_scheduled = false;
                self.flush_config();
            }
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
                let side = self.sidebar.root.clone();
                if side.is_visible() {
                    self.save_layout();
                }
                side.set_visible(!side.is_visible());
            }
            Action::FocusAddress => {
                self.pane().bar().focus_address();
            }
            Action::AddressDone => {
                if !self.config.bar_shown {
                    for pane in &self.panes {
                        if !pane.bar().is_typing() {
                            pane.bar().set_visible(false);
                        }
                    }
                }
            }
            Action::ToggleBar => {
                self.config.bar_shown = !self.config.bar_shown;
                self.save_config_soon();
                let typing = self.panes.iter().any(|p| p.bar().is_typing());
                for pane in &self.panes {
                    pane.bar().set_visible(self.config.bar_shown);
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
                self.sidebar.set_zoom(zoom);
                self.save_config_soon();
            }
            Action::SetTheme(choice) => {
                self.config.theme = choice;
                self.save_config_soon();
                self.apply_theme();
                self.announce_theme();
            }
            Action::SetAutoUpdate(on) => {
                self.config.auto_update = Some(on).filter(|on| !on);
                self.save_config_soon();
            }
            Action::Settings => {
                let side = self.sidebar.root.clone();
                if !side.is_visible() {
                    side.set_visible(true);
                }
                self.sidebar.popup_settings();
            }
            Action::LeaveAddress => {
                if let Some(view) = self.view() {
                    view.webview().grab_focus();
                }
                self.pane().bar().set_text(self.tabs.focused_side().tab().target());
            }
            Action::Go(text) => self.go(&text),
            Action::PickElement => self.page_call(page_calls::annot_pick_element()),
            Action::ToggleAnnotations => self.page_call(page_calls::annot_toggle_panel()),
            Action::ToggleEdit => self.page_call(page_calls::note_toggle()),
            Action::NewNote => {
                if self.vault().is_none() {
                    return;
                }
                let dir = self.focused_file().as_deref().and_then(|f| f.rsplit_once('/')).map_or("", |(d, _)| d).to_string();
                let side = self.sidebar.root.clone();
                side.show();
                self.sidebar.new_note(&dir, false);
            }
            Action::NewTab => {
                self.open_in(String::new(), Place::NewTab);
            }
            Action::CloseTab(id) => {
                let effects = self.tabs.close(id);
                self.apply_and_publish(effects);
            }
            Action::CycleTab(step) => {
                let effects = self.tabs.cycle(step);
                self.apply_and_publish(effects);
            }
            Action::SelectTab(id) => {
                let effects = self.tabs.select(id);
                self.apply_and_publish(effects);
            }
            Action::ToggleSplit => {
                let effects = self.tabs.toggle_split();
                self.apply_and_publish(effects);
            }
            Action::FocusPane(id) => {
                if let Some(p) = self.panes.iter().position(|pane| pane.id() == id) {
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
        let Some((p, _)) = self.tabs.locate(view) else { return };
        let (address, fragment) = url.split_once('#').map_or((url, ""), |(a, f)| (a, f));
        let target = vault::rel_from_url(address).unwrap_or_else(|| url.to_string());
        let fragment = if target == url || fragment.is_empty() { String::new() } else { format!("#{fragment}") };
        if background {
            if self.tabs.open_background(view, target, fragment) {
                self.publish();
            }
        } else {
            self.tabs.focus(p);
            self.open_with(target, Place::NewTab, &fragment);
        }
    }

    /// Applies what an intent on the tabs asks for, if it did anything, and
    /// tells the agents and the saved workspace.
    fn apply_and_publish(&mut self, effects: Vec<Effect>) {
        if effects.is_empty() {
            return;
        }
        self.apply(effects);
        self.focus_changed();
    }

    fn end_drag(&mut self) {
        for p in 0..self.panes.len() {
            self.panes[p].show_drop(false, false);
            self.frost(p, None);
        }
        self.dragging = None;
    }

    fn frost(&self, p: usize, zone: Option<u8>) {
        let Some(view) = self.tabs.side(p).tab().view().and_then(|id| self.view_of(id)) else { return };
        let accent = self.theme.as_ref().map_or("#3584e4", |t| t.accent.as_str());
        let _ = view.evaluate_script(&page_calls::frost(zone, accent));
    }

    fn dropped(&mut self, pane: u64, half: Option<usize>) {
        let target = self.dragging.take();
        self.end_drag();
        let (Some(target), Some(p)) = (target, self.panes.iter().position(|x| x.id() == pane)) else { return };
        if self.panes.len() == 2 {
            self.tabs.focus(p);
            self.open_in(target, Place::NewTab);
            return;
        }
        self.tabs.focus(0);
        self.open_in(target, Place::Beside);
        if half == Some(0) {
            // The new side was added on the right; the file goes on the left.
            let effects = self.tabs.swap_sides();
            self.apply_and_publish(effects);
        }
    }

    /// Runs a page function on the focused side, giving that page the keys.
    fn page_call(&self, code: &str) {
        if let Some(view) = self.view() {
            view.webview().grab_focus();
        }
        self.eval(code);
    }

    fn fullscreen(&mut self, view: u64, on: bool) {
        let side = self.sidebar.root.clone();
        let holder = self.tabs.locate(view).map(|(p, _)| p);
        if on {
            self.sidebar_before_fullscreen.get_or_insert(side.is_visible());
            side.hide();
            for (p, pane) in self.panes.iter().enumerate() {
                pane.fullscreen(Some(p) == holder);
            }
        } else {
            for pane in &self.panes {
                pane.windowed(self.config.bar_shown);
            }
            if self.sidebar_before_fullscreen.take().unwrap_or(true) {
                side.show();
            }
        }
    }

    fn navigated(&mut self, view: u64, url: String) {
        let file = vault::rel_from_url(&url);
        let target = file.clone().or_else(|| is_web(&url).then(|| url.clone())).unwrap_or_default();
        let Some(nav) = self.tabs.navigated(view, target) else { return };
        let focused_active = nav.active && nav.side == self.tabs.focused();
        if nav.revealed {
            self.show_active(nav.side);
            if focused_active {
                self.focus_pane(nav.side, true);
            }
        }
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
        if nav.active {
            self.panes[nav.side].bar().set_text(self.tabs.side(nav.side).tab().target());
            self.update_nav(nav.side);
        }
        self.publish();
        if focused_active {
            self.sidebar.current(file);
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
            SidebarMsg::OpenFolders(open) => {
                if let Some(vault) = self.vault() {
                    self.config.open_folders.insert(vault.path().to_path_buf(), open);
                    self.save_config_soon();
                }
            }
            SidebarMsg::PickVault => self.pick_vault(),
            SidebarMsg::SwitchVault { path } => self.show_vault(Some(path)),
            SidebarMsg::Open { path, tab, beside } => {
                let place = if beside { Place::Beside } else if tab { Place::NewTab } else { Place::Here };
                self.open_in(path, place);
            }
            SidebarMsg::CreateNote { dir, name } => match self.create_note(&dir, &name) {
                Ok(()) => self.sidebar.note_created(),
                Err(e) => self.sidebar.note_error(&e),
            },
            SidebarMsg::CreateFolder { dir, name } => match self.create_folder(&dir, &name) {
                Ok(()) => self.sidebar.note_created(),
                Err(e) => self.sidebar.note_error(&e),
            },
            SidebarMsg::Trash { path } => {
                let result = self.open_vault().and_then(|v| files::trash(&v, &path));
                match result {
                    Ok(()) => {
                        self.sidebar.notice(&format!("«{path}» está en la papelera."), false);
                        self.retarget(&path, None);
                    }
                    Err(e) => self.sidebar.notice(&e, true),
                }
            }
            SidebarMsg::Move { path, dir } => {
                let result = self.open_vault().and_then(|v| files::move_to(&v, &path, &dir));
                match result {
                    Ok(Some(new)) => {
                        let root = self.vault().map(|v| v.path().to_path_buf()).unwrap_or_default();
                        match links::relink(&root, &path, &new) {
                            Ok(changed) if !changed.is_empty() => {
                                let n = changed.len();
                                let text = if n == 1 { "Se actualizaron los enlaces de 1 nota.".to_string() } else { format!("Se actualizaron los enlaces de {n} notas.") };
                                self.sidebar.notice(&text, false);
                            }
                            Ok(_) => {}
                            Err(e) => self.sidebar.notice(&e, true),
                        }
                        self.retarget(&path, Some(&new));
                    }
                    Ok(None) => {}
                    Err(e) => self.sidebar.notice(&e, true),
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
                self.config.open_folders.remove(&path);
                if self.vault().is_some_and(|v| v.path() == path) {
                    self.config.current = self.config.vaults.first().cloned();
                    self.save_config_soon();
                    // The removed vault must not get its workspace saved back.
                    self.shared.lock().unwrap().vault = None;
                    let next = self.config.current.clone();
                    self.show_vault(next);
                } else {
                    self.save_config_soon();
                    self.refresh_sidebar();
                }
            }
        }
    }

    /// Points the tabs showing `old`, or a file inside the folder `old`, at
    /// where it moved, or at the empty page if it was deleted.
    fn retarget(&mut self, old: &str, new: Option<&str>) {
        let (effects, hidden_changed) = self.tabs.retarget(old, new);
        self.apply(effects);
        if hidden_changed {
            self.publish();
        }
    }

    /// Only vault and loose pages may log or send events; the empty page may only ask for a folder.
    /// A website could otherwise feed instructions to the agents that read them.
    fn on_page(&mut self, view: u64, url: &str, msg: PageMsg) {
        // Loose pages may log and send events too, but have no vault to write to.
        let access = vault::access(url);
        let local = access != Access::None;
        let trusted = access == Access::Trusted;
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
                reply(self, page_calls::note_saved(ok, &error));
            }
            PageMsg::ToggleTask { offset, checked } if trusted => {
                let error = self.toggle_task(url, offset, checked).err().unwrap_or_default();
                reply(self, page_calls::note_toggled(&error));
            }
            PageMsg::AnnotSave { annotation } if trusted => {
                match self.with_page_annotations(url, |root, page| annot::upsert(root, page, &annotation)) {
                    Ok(true) => {
                        let file = vault::rel_from_url(url).unwrap_or_default();
                        let (id, text) = (annotation["id"].as_str().unwrap_or_default(), annotation["comment"].as_str().unwrap_or_default());
                        self.note_annotation(&file, id, "comment", text);
                    }
                    Ok(false) => {}
                    Err(e) => {
                        eprintln!("No se pudo guardar la anotación: {e}");
                        reply(self, page_calls::annot_save_failed(annotation["id"].as_str().unwrap_or_default(), &e));
                    }
                }
            }
            PageMsg::AnnotDelete { id } if trusted => {
                if let Err(e) = self.with_page_annotations(url, |root, page| annot::delete(root, page, &id)) {
                    eprintln!("No se pudo borrar la anotación: {e}");
                    reply(self, page_calls::annot_delete_failed(&id, &e));
                }
            }
            PageMsg::AnnotReply { id, text } if trusted => {
                let file = vault::rel_from_url(url).unwrap_or_default();
                match self.with_page_annotations(url, |root, page| annot::person_reply(root, page, &id, &text)) {
                    Ok(()) => self.note_annotation(&file, &id, "reply", &text),
                    // The page took the message as sent; it takes it back and shows why.
                    Err(e) => reply(self, page_calls::annot_reply_failed(&id, &text, &e)),
                }
            }
            PageMsg::AnnotStatus { orphans, total } if trusted => {
                if let Some(page) = self.shared.lock().unwrap().pages.get_mut(&view) {
                    page.orphans = (total > 0).then_some(orphans);
                }
            }
            PageMsg::CreateNote { dir, name } if trusted => {
                if let Err(e) = self.create_note(&dir, files::wiki_note_name(&name)) {
                    eprintln!("No se pudo crear la nota: {e}");
                    reply(self, page_calls::note_create_failed(&e));
                }
            }
            PageMsg::Console { level, text } if local => {
                let mut shared = self.shared.lock().unwrap();
                let Some(page) = shared.pages.get_mut(&view) else { return };
                if page.console.len() >= MAX_CONSOLE {
                    page.console.remove(0);
                }
                // Vault paths read better than server URLs, and keep the secret out of logs.
                let text = text.replace(vault::base(), "").replace(vault::loose_base(), "");
                page.console.push(json!({ "time": now(), "level": level, "text": text }));
            }
            PageMsg::Event { data } if local => {
                let mut shared = self.shared.lock().unwrap();
                let file = shared.page_of(view).file.clone();
                push_event(&mut shared, json!({ "file": file, "data": data }));
            }
            _ => {}
        }
    }

    /// Tells agents what the person wrote in an annotation. It carries
    /// `annotation` and no `data`: a page's own `visor.send` always has `data`,
    /// so it cannot pass as one of these.
    fn note_annotation(&self, file: &str, id: &str, action: &str, text: &str) {
        let mut shared = self.shared.lock().unwrap();
        push_event(&mut shared, json!({ "file": file, "annotation": { "id": id, "action": action, "text": text } }));
    }

    /// Runs an annotation change for the page at `url`, which must be a real
    /// file of the open vault: the page cannot name any other.
    fn with_page_annotations<T>(&self, url: &str, change: impl FnOnce(&Path, &str) -> Result<T, String>) -> Result<T, String> {
        let vault = self.open_vault()?;
        let (page, _) = vault.page(url)?;
        change(vault.path(), &page)
    }

    /// Writes a note's text. Only the note itself may save, and only Markdown:
    /// an HTML page in the vault cannot overwrite files through this.
    fn save_note(&mut self, url: &str, text: &str) -> Result<(), String> {
        let (_, path) = self.open_vault()?.note(url)?;
        self.own_write = Some(OwnWrite::new(&path, text));
        vault::write_atomic(&path, text).map_err(|e| e.to_string())
    }

    /// Ticks or unticks one task of the note at `url`. Rust re-reads the file and
    /// checks that a task really starts at `offset`, so the page cannot write anything else.
    fn toggle_task(&mut self, url: &str, offset: usize, checked: bool) -> Result<(), String> {
        let (_, path) = self.open_vault()?.page(url)?;
        if !vault::is_markdown(&path) {
            return Err("Solo se pueden cambiar notas .md".into());
        }
        let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let text = crate::vault::markdown::toggle_task(&source, offset, checked).ok_or("La tarea ya no está en la nota")?;
        self.save_note(url, &text)
    }

    fn create_folder(&mut self, dir: &str, name: &str) -> Result<(), String> {
        let (folder, name) = self.open_vault()?.new_in(dir, name)?;
        let path = folder.join(name);
        if path.exists() {
            return Err(format!("Ya existe {name}"));
        }
        fs::create_dir(&path).map_err(|e| e.to_string())
    }

    /// Creates `name`.md in `dir` and opens it in the editor.
    fn create_note(&mut self, dir: &str, name: &str) -> Result<(), String> {
        let vault = self.open_vault()?;
        let (folder, name) = files::note_target(&vault, dir, name)?;
        let file = if name.to_lowercase().ends_with(".md") { name.to_string() } else { format!("{name}.md") };
        let path = folder.join(&file);
        if path.exists() {
            return Err(format!("Ya existe {file}"));
        }
        let title = file.trim_end_matches(".md");
        let text = format!("# {title}\n\n");
        self.own_write = Some(OwnWrite::new(&path, &text));
        vault::write_atomic(&path, text).map_err(|e| e.to_string())?;
        let rel = vault.rel(&path);
        // The name was typed in the sidebar; writing continues in the note.
        self.open_with(rel, Place::Here, "#editar");
        Ok(())
    }

    /// Opens a folder as vault or a file inside the vault that contains it.
    /// An HTML page outside every known vault opens on its own.
    fn open(&mut self, path: PathBuf, raw: &str, place: Place) -> Result<Option<Loading>, String> {
        let current = self.vault().map(|v| v.path().to_path_buf());
        let path = self.resolve_page(path, raw)?;
        if path.is_dir() {
            Config::check_vault(&path)?;
            self.show_vault(Some(path));
            return Ok(None);
        }
        let known = current.filter(|v| path.starts_with(v)).or_else(|| self.config.vault_for(&path));
        let target = match known {
            None if vault::is_html(&path) => {
                // Turning its folder into a vault would list and watch all of it, maybe a whole home.
                if self.vault().is_none() {
                    self.sidebar.root.hide();
                }
                path.to_string_lossy().into_owned()
            }
            None if !vault::is_page(&path) => return Err("visor abre carpetas, páginas .html y notas .md.".into()),
            known => {
                let root = known
                    .or_else(|| path.parent().map(Path::to_path_buf))
                    .ok_or_else(|| format!("Sin carpeta para {}", path.display()))?;
                let rel = vault::rel(&root, &path);
                if self.vault().is_none_or(|v| v.path() != root) {
                    Config::check_vault(&root)?;
                    self.show_vault(Some(root));
                }
                rel
            }
        };
        // A lone empty tab, as in a vault opened for the first time, is used rather than kept.
        let side = self.tabs.focused_side();
        let lone_empty = self.tabs.sides().len() == 1 && side.tabs().len() == 1 && side.tab().target().is_empty();
        Ok(self.open_in(target, if lone_empty { Place::Here } else { place }))
    }

    /// The file a path names, canonical: as given, or else relative to the open vault.
    fn resolve_page(&self, path: PathBuf, raw: &str) -> Result<PathBuf, String> {
        let current = self.vault().map(|v| v.path().to_path_buf());
        path.canonicalize()
            .or_else(|_| current.as_ref().ok_or(()).and_then(|v| v.join(raw).canonicalize().map_err(|_| ())))
            .map_err(|_| format!("No existe: {raw}"))
    }

    /// Closes the focused side's active tab, or every tab showing the page
    /// `target` (path as given, raw as written). Returns how many closed.
    fn close_for_agent(&mut self, target: Option<(PathBuf, String)>) -> Result<usize, String> {
        let Some((path, raw)) = target else {
            if self.tabs.is_blank() {
                return Ok(0);
            }
            let effects = self.tabs.close(None);
            self.apply_and_publish(effects);
            return Ok(1);
        };
        let root = self.vault().map(|v| v.path().to_path_buf());
        let canonical = path.canonicalize().ok();
        let names = vault::tab_names(&path, canonical.as_deref(), &raw, root.as_deref(), |p| {
            self.config.vault_for(p).is_some()
        });
        // The first name a tab shows wins; a deleted page is found by its lexical name.
        for name in &names {
            let (effects, closed) = self.tabs.close_showing(name);
            if closed > 0 {
                self.apply_and_publish(effects);
                return Ok(closed);
            }
        }
        Err(format!("No hay ninguna pestaña abierta con {raw}."))
    }

    /// Watches the folders of the loose pages in the tabs, and stops watching the rest.
    /// The server may read exactly those folders.
    fn watch_loose(&mut self) {
        let mut dirs: Vec<PathBuf> = Vec::new();
        for target in self.tabs.targets().filter(|t| vault::is_loose(t)) {
            if let Some(dir) = Path::new(target).parent()
                && !dirs.iter().any(|d| d == dir)
            {
                dirs.push(dir.to_path_buf());
            }
        }
        self.loose_watchers.sync(&dirs, &self.proxy);
        self.shared.lock().unwrap().loose = dirs;
    }

    fn live_pages(&self) -> Vec<(u64, Option<String>, &str)> {
        self.tabs
            .live()
            .filter_map(|(id, target)| {
                let view = self.view_of(id)?;
                Some((id, vault::rel_from_url(&view.url().unwrap_or_default()), target))
            })
            .collect()
    }

    fn loose_changed(&mut self, changed: Vec<PathBuf>) {
        let live = self.live_pages();
        let pages: Vec<LivePage> = live.iter().map(|(view, file, target)| LivePage { view: *view, file: file.as_deref(), target }).collect();
        for view in watch::loose_reaction(&changed, &pages) {
            if let Some(v) = self.view_of(view) {
                let _ = v.reload();
            }
        }
    }

    fn vault_changed(&mut self, changed: Vec<PathBuf>) {
        let Some(vault) = self.vault() else { return };
        let live = self.live_pages();
        let pages: Vec<LivePage> = live.iter().map(|(view, file, target)| LivePage { view: *view, file: file.as_deref(), target }).collect();
        let reaction = watch::vault_reaction(vault.path(), &changed, self.own_write.as_ref(), &pages, &RealDisk);
        for view in reaction.reload_annotations {
            if let Some(v) = self.view_of(view) {
                let _ = v.evaluate_script(page_calls::annot_reload());
            }
        }
        if reaction.refresh_tree {
            self.sidebar.set_tree(Some(vault.tree()));
        }
        for (view, action) in reaction.pages {
            let Some(v) = self.view_of(view) else { continue };
            match action {
                // On the empty page the message depends on whether the vault has files.
                PageAction::ReloadEmpty => self.load_into(v, "", ""),
                // A note being edited must not lose its text to a reload.
                PageAction::NoteChanged => {
                    let _ = v.evaluate_script(page_calls::note_changed());
                }
                PageAction::Reload => {
                    let _ = v.reload();
                }
            }
        }
    }
}
