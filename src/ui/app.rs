use crate::i18n::tr;
use crate::trf;
use crate::vault::annot;
use crate::session::{Loading, Page, Place, AgentRequest, SharedRef, SizeReply, ValueReply, push_console, push_event};
use crate::ui::common::{CommonRef, SettingsChange};
use crate::ui::windows::WinId;
use crate::ui::action::{Action, LoadFailure, Proxy, RestartCheck, Routed, UserEvent};
use crate::ui::address::Engine;
use crate::ui::host::Host;
use crate::ui::protocol::PageMsg;
use crate::vault::config::{Config, PaneState, WindowState, Workspace};
use crate::vault::files as vault_files;
use crate::ui::page_calls;
use crate::ui::page_view::{Document, build_page, clean_position, host_of};
use crate::vault::is_web;
use crate::ui::tabs::{Effect, Tabs};
use crate::ui::theme::Theme;
use crate::ui::watch::{self, LivePage, LooseWatchers, OwnWrite, PageAction, RealDisk, watch};
use crate::vault::{self, Access, Vault};
use gtk::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode};
use serde_json::json;
use std::cell::Cell;
use std::collections::HashMap;
use std::fs;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use tao::dpi::LogicalSize;
use tao::event_loop::EventLoopWindowTarget;
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use webkit2gtk::{SnapshotOptions, SnapshotRegion, UserContentInjectedFrames, UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebViewExt};
use wry::{WebView, WebViewExtUnix};

mod bar;
mod files;
mod find;
mod frame;
mod settings;
mod sides;

const ZOOM_STEPS: [f64; 13] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// App-wide shortcuts. They are caught on the window before the focused
/// webview sees them, so they work whichever side has focus.
///
/// `in_drawing` is set while a drawing is in front: Ctrl+K then goes to the
/// page, which uses it to add a link.
fn connect_shortcuts(window: &tao::window::Window, proxy: Proxy, in_drawing: Rc<Cell<bool>>) {
    use gtk::gdk::{ModifierType, keys::constants as key};
    window.gtk_window().connect_key_press_event(move |_, event| {
        let ctrl = event.state().contains(ModifierType::CONTROL_MASK);
        let alt = event.state().contains(ModifierType::MOD1_MASK);
        let shift = event.state().contains(ModifierType::SHIFT_MASK);
        let k = event.keyval().to_lower();
        // Ctrl+1 … Ctrl+9 from the top row or the keypad. Ctrl+0 is left to the zoom.
        let digit = match k {
            key::_1 | key::KP_1 => 1,
            key::_2 | key::KP_2 => 2,
            key::_3 | key::KP_3 => 3,
            key::_4 | key::KP_4 => 4,
            key::_5 | key::KP_5 => 5,
            key::_6 | key::KP_6 => 6,
            key::_7 | key::KP_7 => 7,
            key::_8 | key::KP_8 => 8,
            key::_9 | key::KP_9 => 9,
            _ => 0,
        };
        let action = match () {
            _ if ctrl && shift && k == key::a => Action::PickElement,
            _ if ctrl && shift && k == key::l => Action::ToggleAnnotations,
            _ if ctrl && shift && k == key::b => Action::ToggleBar,
            _ if ctrl && matches!(k, key::plus | key::equal | key::KP_Add) => Action::ZoomKey(1),
            _ if ctrl && matches!(k, key::minus | key::KP_Subtract) => Action::ZoomKey(-1),
            _ if ctrl && matches!(k, key::_0 | key::KP_0) => Action::ZoomKey(0),
            _ if ctrl && digit > 0 => Action::SelectNth(digit),
            _ if ctrl && k == key::comma => Action::Settings,
            _ if ctrl && k == key::k && !in_drawing.get() => Action::Shortcuts,
            _ if ctrl && !shift && k == key::f && !in_drawing.get() => Action::Find,
            _ if ctrl && k == key::ISO_Left_Tab => Action::CycleTab(-1),
            _ if ctrl && k == key::Tab => Action::CycleTab(if shift { -1 } else { 1 }),
            _ if ctrl && alt && k == key::Left => Action::FocusSide(0),
            _ if ctrl && alt && k == key::Right => Action::FocusSide(1),
            // Also «|»: on Spanish layouts «\» needs AltGr and «|» does not.
            _ if ctrl && matches!(k, key::backslash | key::bar) => Action::ToggleSplit,
            _ if k == key::F5 => Action::Reload,
            _ if ctrl && k == key::r => Action::Reload,
            _ if ctrl && k == key::o => Action::PickVault,
            _ if ctrl && k == key::q => Action::Quit,
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
        proxy.send(UserEvent::Action(action));
        gtk::glib::Propagation::Stop
    });
}

/// Whether agents may read the document a view committed.
fn readable(vault: Option<&Vault>, document: &Document) -> Result<(), String> {
    if vault::agent_may_read(&document.borrow(), vault) {
        Ok(())
    } else {
        Err(tr("Solo se puede leer una página de la bóveda o un archivo suelto, no una web.", "Only a page in the vault or a loose file can be read, not a website.").into())
    }
}

/// Encodes a snapshot as PNG and writes it. Returns how many bytes went to the file.
fn save_png(surface: &gtk::cairo::Surface, path: &Path) -> Result<u64, String> {
    let mut bytes = Vec::new();
    surface.write_to_png(&mut bytes).map_err(|e| trf!("No se pudo codificar la captura: {e}", "Could not encode the snapshot: {e}"))?;
    fs::write(path, &bytes).map_err(|e| trf!("No se pudo escribir {}: {e}", "Could not write {}: {e}", path.display()))?;
    Ok(bytes.len() as u64)
}

fn empty_page(message: &str, pick: bool) -> String {
    let lang = crate::i18n::lang().code();
    let button = if pick {
        format!(r#"<button onclick="ipc.postMessage(JSON.stringify({{type:'pick_vault'}}))">{}</button>"#, tr("Abrir carpeta…", "Open folder…"))
    } else {
        String::new()
    };
    format!(
        r#"<!doctype html><html lang="{lang}"><meta charset="utf-8"><style>
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

/// The window's app: its tabs, sides and views. What every window shares
/// (settings, theme, web context) is in `common`. The interface is the frame,
/// drawn by `host`; each area's part of the app is in `app/<area>.rs`.
pub(super) struct App {
    /// The frame's webview and the containers of each side's page views,
    /// which follow the sides of `tabs`.
    host: Host,
    /// What the frame was last sent; see `app/frame.rs`.
    frame: frame::Sent,
    tabs: Tabs,
    /// The live pages' views, by view id.
    views: Vec<(u64, WebView)>,
    /// What each view committed, as WebKit reported it: agents may read only visor's pages.
    documents: HashMap<u64, Document>,
    /// Whether the focused view shows a drawing; see `connect_shortcuts`.
    in_drawing: Rc<Cell<bool>>,
    /// Whether the panel shows: not for a loose page started alone, and Ctrl+B flips it.
    sidebar_shown: bool,
    /// The last result of an update check, for the status bar.
    update_message: Option<String>,
    /// The last file the app wrote and a hash of what it wrote. A change on disk
    /// that still matches is the app's own; anything else came from outside.
    own_write: Option<OwnWrite>,
    shared: SharedRef,
    /// The window this app runs. Agents address it by id.
    win: WinId,
    proxy: Proxy,
    /// The process-wide state, shared with every window. How to borrow it is described in `ui/visor.rs`.
    common: CommonRef,
    watcher: Option<RecommendedWatcher>,
    /// One per folder of the loose pages in the tabs, without subfolders.
    loose_watchers: LooseWatchers,
    /// This window's sidebar width and divider share, saved with the window.
    /// The frame does not use them yet; they are kept for phase 1.
    sidebar_width: Option<i32>,
    split_ratio: Option<f64>,
    /// Started for a loose page: until a vault opens, the window is not remembered.
    loose: bool,
    /// Tabs, sides or size changed since the process last saved the windows.
    layout_changed: bool,
    /// Last, so the views above are dropped before their window.
    window: tao::window::Window,
}

impl App {
    /// Builds the window's widgets and its first, empty side. `common` is the
    /// process-wide state that the other windows share too.
    pub(super) fn new(
        target: &EventLoopWindowTarget<Routed>,
        proxy: Proxy,
        shared: SharedRef,
        common: CommonRef,
        state: &WindowState,
        loose: bool,
    ) -> App {
        let win = proxy.window();
        let (width, height) = state.size.map_or((1200.0, 820.0), |(w, h)| (f64::from(w), f64::from(h)));
        let window = WindowBuilder::new()
            .with_title("Visor")
            .with_inner_size(LogicalSize::new(width, height))
            .build(target)
            .expect("ventana");
        let in_drawing = Rc::new(Cell::new(false));
        connect_shortcuts(&window, proxy.clone(), in_drawing.clone());
        // The borrow covers Host::new, a GTK call that only connects handlers
        // sending through the proxy; none of them reads `common`.
        let host = {
            let mut c = common.borrow_mut();
            let (background, base) = (c.background, c.frame_base.clone());
            Host::new(&window, &mut c.context, background, proxy.clone(), &base).expect("marco")
        };
        let tabs = Tabs::new_from(crate::ui::windows::first_view_id(win));
        host.add_side(tabs.side(0).id());

        App {
            host,
            frame: frame::Sent::default(),
            tabs,
            views: vec![],
            documents: HashMap::new(),
            in_drawing,
            // Started for a page outside every vault: that page alone, without the
            // last vault and its tabs. See cli::start.
            sidebar_shown: !loose,
            update_message: None,
            own_write: None,
            shared,
            win,
            proxy,
            common,
            watcher: None,
            loose_watchers: LooseWatchers::default(),
            sidebar_width: state.sidebar_width,
            split_ratio: state.split_ratio,
            loose,
            layout_changed: false,
            window,
        }
    }

    pub(super) fn tao_id(&self) -> tao::window::WindowId {
        self.window.id()
    }

    /// Asks the compositor for the focus. `token` is the activation token of
    /// the launch that asked for this window, without which Wayland may refuse.
    pub(super) fn present(&self, token: Option<&str>) {
        if let Some(token) = token {
            self.window.gtk_window().set_startup_id(token);
        }
        self.window.set_focus();
    }

    /// What the settings keep of this window, to reopen it as it is.
    pub(super) fn window_state(&self) -> WindowState {
        let size = self.window.inner_size().to_logical::<f64>(self.window.scale_factor());
        WindowState {
            vault: self.vault().map(|v| v.path().to_path_buf()),
            workspace: self.tabs.workspace(),
            size: Some((size.width.round() as u32, size.height.round() as u32)),
            sidebar_width: self.sidebar_width,
            split_ratio: self.split_ratio,
        }
    }

    /// Whether the window is remembered: not while it shows only the loose
    /// page it was started for.
    pub(super) fn is_remembered(&self) -> bool {
        !self.loose || self.vault().is_some()
    }

    pub(super) fn resized(&mut self) {
        self.layout_changed = true;
    }

    /// Whether the window changed in a way the settings keep, since the last call.
    pub(super) fn take_layout_changed(&mut self) -> bool {
        std::mem::take(&mut self.layout_changed)
    }

    /// Another window changed a setting that every window shares: shows it here too.
    pub(super) fn settings_changed(&mut self, changed: &SettingsChange) {
        if changed.zoom {
            for (id, _) in &self.views {
                self.apply_zoom(*id);
            }
        }
        if changed.theme {
            self.restyle();
            self.announce_theme();
        }
        // Another window removed the vault this one shows.
        let removed = changed.vaults && self.vault().is_some_and(|v| self.cfg(|c| !c.vaults.iter().any(|p| p == v.path())));
        if removed {
            self.leave_removed_vault();
        } else if changed.vaults || changed.show_other {
            self.refresh_tree();
        }
        self.sync_frame();
    }

    /// The vault this window shows was removed from the list: shows the first
    /// one left, or none.
    fn leave_removed_vault(&mut self) {
        // The removed vault must not get its workspace saved back.
        self.shared.lock().unwrap().window_mut(self.win).vault = None;
        let next = self.cfg(|c| c.vaults.first().cloned());
        self.show_vault(next);
    }

    /// The installed binary could not replace this process.
    pub(super) fn restart_failed(&mut self, message: &str) {
        self.update_done(message, true);
        self.sync_frame();
    }

    /// Reads the settings. The closure runs while `common` is borrowed, so it
    /// must not call App or GTK.
    fn cfg<R>(&self, read: impl FnOnce(&Config) -> R) -> R {
        read(&self.common.borrow().config)
    }

    /// Changes the settings. The closure runs while `common` is borrowed, so it
    /// must not call App or GTK.
    fn cfg_mut<R>(&self, change: impl FnOnce(&mut Config) -> R) -> R {
        change(&mut self.common.borrow_mut().config)
    }

    /// The theme in use: a second handle to the one `common` holds.
    fn theme(&self) -> Option<Rc<Theme>> {
        self.common.borrow().theme.clone()
    }

    fn vault(&self) -> Option<Arc<Vault>> {
        self.shared.lock().unwrap().window(self.win).and_then(|w| w.vault.clone())
    }

    /// The URL prefix of the open vault's pages. With no vault open, the default base.
    fn vault_base(&self) -> String {
        self.vault().map_or_else(|| vault::NO_VAULT_BASE.to_string(), |v| v.base().to_string())
    }

    /// The vault path, or the loose absolute path, that a visor address names.
    fn rel_of(&self, url: &str) -> Option<String> {
        self.vault().and_then(|v| v.rel_from_url(url)).or_else(|| vault::loose_rel_from_url(url))
    }

    fn open_vault(&self) -> Result<Arc<Vault>, String> {
        self.vault().ok_or_else(|| tr("No hay ninguna bóveda abierta", "No vault is open").into())
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
        shared.focused_page(self.win).file.clone()
    }

    fn view_of(&self, view: u64) -> Option<&WebView> {
        self.views.iter().find(|(id, _)| *id == view).map(|(_, v)| v)
    }

    /// Opens a vault and restores its tabs. Without a vault the window shows one empty tab.
    pub(super) fn show_vault(&mut self, root: Option<PathBuf>) {
        self.show_vault_with(root, None);
    }

    /// Like `show_vault`, with the tabs to restore instead of the vault's last ones.
    pub(super) fn show_vault_with(&mut self, root: Option<PathBuf>, workspace: Option<Workspace>) {
        if let Some(Err(e)) = root.as_deref().map(Config::check_vault) {
            eprintln!("{e}");
            return;
        }
        let opened = match root.as_deref().map(|r| self.common.borrow_mut().vault(r, &self.shared)) {
            Some(Err(e)) => {
                eprintln!("{e}");
                return;
            }
            opened => opened.and_then(Result::ok),
        };
        if root.is_some() && self.vault().is_none() {
            // It was hidden for loose pages.
            self.sidebar_shown = true;
        }
        self.publish();
        {
            let mut shared = self.shared.lock().unwrap();
            let w = shared.window_mut(self.win);
            w.vault = opened;
            w.pages.clear();
        }
        self.watcher = None;
        if let Some(root) = &root {
            self.cfg_mut(|c| c.add(root));
            self.save_config_soon();
            self.watcher = watch(root, RecursiveMode::Recursive, 150, self.proxy.clone(), UserEvent::VaultChanged);
        }
        self.refresh_tree();

        let name = root.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
        self.window.set_title(&name.map_or_else(|| "Visor".to_string(), |n| format!("Visor — {n}")));
        let saved = workspace.filter(|_| root.is_some()).or_else(|| root.as_deref().and_then(|r| self.saved_workspace(r)));
        let effects = self.tabs.reset(saved, |t| root.as_deref().is_some_and(|r| r.join(t).is_file()));
        self.apply(effects);
        self.focus_changed();
    }

    /// The workspace saved for `root`, or one tab with the last file opened
    /// there, from before the workspaces.
    fn saved_workspace(&self, root: &Path) -> Option<Workspace> {
        self.cfg(|c| {
            c.workspaces.get(root).cloned().or_else(|| {
                let last = c.last.get(root)?.clone();
                Some(Workspace { panes: vec![PaneState { tabs: vec![last], active: 0 }], focused: 0 })
            })
        })
    }

    /// Records the sides and tabs for `visor status` and in the vault's workspace.
    fn publish(&mut self) {
        let focused = self.tabs.focused_view().unwrap_or(0);
        let root = {
            let mut shared = self.shared.lock().unwrap();
            let w = shared.window_mut(self.win);
            w.layout = self.tabs.layout();
            w.focused = focused;
            w.vault.as_ref().map(|v| v.path().to_path_buf())
        };
        self.watch_loose();
        self.layout_changed = true;
        let Some(root) = root else { return };
        let workspace = self.tabs.workspace();
        self.cfg_mut(|c| {
            c.workspaces.insert(root, workspace);
        });
        self.save_config_soon();
    }

    /// Does what the tabs model asks, in order. Returns the load of the first
    /// view it creates or points elsewhere: the one `open` waits for.
    fn apply(&mut self, effects: Vec<Effect>) -> Option<Loading> {
        let mut loading = None;
        for effect in effects {
            match effect {
                Effect::AddSide { id } => self.host.add_side(id),
                Effect::RemoveSide { side } => self.host.remove_side(side),
                Effect::SwapSides => self.host.swap_sides(),
                Effect::Create { side, view, target, fragment } => {
                    if self.create_view(side, view, &target, &fragment) {
                        loading.get_or_insert((view, 0));
                    } else {
                        self.tabs.abort_view(view);
                    }
                }
                Effect::Unload { view } => {
                    self.views.retain(|(id, _)| *id != view);
                    self.documents.remove(&view);
                    self.shared.lock().unwrap().window_mut(self.win).pages.remove(&view);
                }
                Effect::MoveView { view, from, to } => {
                    // Views move to the other side's container; WebKit keeps the page loaded.
                    if let Some(v) = self.view_of(view) {
                        let widget = v.webview();
                        self.host.views(from).remove(&widget);
                        self.host.views(to).pack_start(&widget, true, true, 0);
                    }
                }
                Effect::Navigate { view, target, fragment } => {
                    if let Some(v) = self.view_of(view) {
                        let before = self.shared.lock().unwrap().page_of(self.win, view).load_seq;
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
                            proxy.send(UserEvent::Position { view, json });
                        });
                    }
                }
                Effect::Show { side } => self.show_active(side),
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
        let vault_base = self.vault_base();
        // The borrow covers build_page, a GTK call. build_page only connects
        // handlers that send events through the proxy and reads neither App nor
        // `common` (see page_view.rs), so no handler can borrow `common` meanwhile.
        let built = {
            let mut c = self.common.borrow_mut();
            let background = c.background;
            build_page(&self.host.views(side), &mut c.context, background, self.proxy.clone(), &vault_base, id)
        };
        let view = match built {
            Ok((v, document)) => {
                self.documents.insert(id, document);
                v
            }
            Err(e) => {
                eprintln!("{}", trf!("No se pudo crear el webview: {e}", "Could not create the webview: {e}"));
                return false;
            }
        };
        view.webview().set_visible(false);
        self.style_view(&view);
        view.webview().set_zoom_level(self.cfg(|c| c.zoom.unwrap_or(1.0)));
        self.shared.lock().unwrap().window_mut(self.win).pages.insert(id, Page::default());
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

    fn load_into(&self, view: &WebView, target: &str, fragment: &str) {
        if target.is_empty() {
            let _ = view.load_html(&empty_page(self.empty_message(), self.vault().is_none()));
        } else if is_web(target) {
            let _ = view.load_url(target);
        } else if vault::is_loose(target) {
            if let Some(dir) = Path::new(target).parent() {
                let mut shared = self.shared.lock().unwrap();
                let loose = &mut shared.window_mut(self.win).loose;
                if !loose.iter().any(|d| d == dir) {
                    loose.push(dir.to_path_buf());
                }
            }
            let _ = view.load_url(&format!("{}{fragment}", vault::loose_url(target)));
        } else if let Some(vault) = self.vault() {
            let _ = view.load_url(&format!("{}{fragment}", vault.url(target)));
        } else {
            let _ = view.load_html(&empty_page(self.empty_message(), true));
        }
    }

    fn empty_message(&self) -> &'static str {
        match self.vault() {
            None => tr("Abre una carpeta con archivos HTML para empezar.", "Open a folder with HTML files to start."),
            Some(root) => {
                if !root.has_pages() {
                    tr(
                        "Esta carpeta todavía no tiene archivos HTML. Aparecerán aquí en cuanto se creen.",
                        "This folder has no HTML files yet. They will appear here as soon as they are created.",
                    )
                } else {
                    tr("Elige un archivo de la lista.", "Choose a file from the list.")
                }
            }
        }
    }

    /// Updates what depends on the focused side: the agents' view. The frame
    /// reads the rest after the event.
    fn focus_changed(&mut self) {
        self.publish();
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

    fn style_view(&self, view: &WebView) {
        if let Some(manager) = view.webview().user_content_manager() {
            manager.remove_all_style_sheets();
            if let Some(theme) = &self.theme() {
                manager.add_style_sheet(&UserStyleSheet::new(
                    &theme.page_css(),
                    UserContentInjectedFrames::AllFrames,
                    UserStyleLevel::User,
                    &[],
                    &[],
                ));
            }
        }
        if let Some(theme) = &self.theme() {
            let _ = view.set_background_color(theme.rgba());
        }
    }

    fn is_pdf_view(&self, id: u64) -> bool {
        self.shared.lock().unwrap().page_of(self.win, id).file.as_deref().is_some_and(|f| vault::is_pdf(Path::new(f)))
    }

    /// A PDF has the zoom of its own viewer, which draws the canvases at the
    /// size shown; WebKit's would stretch them blurry, toolbar and all.
    fn apply_zoom(&self, id: u64) {
        let Some(view) = self.view_of(id) else { return };
        let zoom = if self.is_pdf_view(id) { 1.0 } else { self.cfg(|c| c.zoom.unwrap_or(1.0)) };
        view.webview().set_zoom_level(zoom);
    }

    fn live_views(&self) -> impl Iterator<Item = &WebView> {
        self.views.iter().map(|(_, v)| v)
    }

    pub(super) fn apply_theme(&mut self) {
        self.common.borrow_mut().reload_theme();
        self.restyle();
    }

    /// Styles the frame and the views with the theme `common` holds.
    pub(super) fn restyle(&mut self) {
        for view in self.live_views() {
            self.style_view(view);
        }
        let background = self.common.borrow().background;
        self.host.set_background(background);
        self.sync_frame();
    }

    /// Tells the open pages of visor that the theme changed, once their
    /// `--visor-*` variables hold the new values. Only pages of visor get it:
    /// `Page::file` is set from the committed address, the one `vault::access`
    /// trusts, and is `None` for websites. Called on changes only, not at
    /// startup, when no page has been opened yet.
    pub(super) fn announce_theme(&self) {
        let Some(theme) = self.theme() else { return };
        let code = page_calls::theme_changed(theme.dark);
        let shared = self.shared.lock().unwrap();
        for (id, view) in &self.views {
            if shared.page_of(self.win, *id).file.is_some() {
                let _ = view.evaluate_script(&code);
            }
        }
    }

    /// Logs a resource of a visor page that did not load. Only visor's own
    /// addresses are written in full; any other site is named by its host,
    /// because its address or error text could carry instructions for agents.
    fn resource_failed(&self, view: u64, uri: &str, reason: LoadFailure) {
        // Before the lock: `vault_base` takes it too, and it is not reentrant.
        let vault_base = self.vault_base();
        let mut shared = self.shared.lock().unwrap();
        if shared.page_of(self.win, view).file.is_none() {
            return;
        }
        let why = match reason {
            LoadFailure::Network => tr("error de red", "network error").to_string(),
            LoadFailure::Status(status) => trf!("respondió {status}", "responded {status}"),
        };
        let own = [vault_base.as_str(), vault::loose_base()].into_iter().find(|base| !base.is_empty() && uri.starts_with(base));
        // visor's own scripts ask for things that are often missing: annot.js for
        // the page's annotation file, which exists once it has notes, and the
        // editor for where a `[[link]]` leads, which may be a note not yet written.
        if let (Some(base), LoadFailure::Status(404)) = (own, &reason)
            && (uri[base.len()..].starts_with(annot::DIR) || uri.contains("?wiki="))
        {
            return;
        }
        let text = if own.is_some() {
            trf!("No se pudo cargar {uri}: {why}", "Could not load {uri}: {why}")
        } else {
            trf!("No se pudo cargar un recurso de {} ({why})", "Could not load a resource from {} ({why})", host_of(uri))
        };
        push_console(&mut shared, self.win, view, "error", &text);
    }

    fn save_config_soon(&self) {
        self.common.borrow_mut().mark_config_dirty();
    }

    pub(super) fn save_layout(&mut self) {
        self.publish();
    }

    pub(super) fn handle(&mut self, event: UserEvent) {
        self.dispatch(event);
        self.sync_frame();
        self.in_drawing.set(self.focused_is_drawing());
    }

    /// Whether the focused view shows a drawing, from the address it committed.
    fn focused_is_drawing(&self) -> bool {
        let Some(id) = self.tabs.focused_view() else { return false };
        self.documents.get(&id).is_some_and(|document| {
            self.rel_of(&document.borrow()).is_some_and(|rel| vault::is_drawing(Path::new(&rel)))
        })
    }

    fn dispatch(&mut self, event: UserEvent) {
        match event {
            UserEvent::Frame(msg) => self.on_frame(msg),
            UserEvent::Page { view, url, body } => {
                if let Ok(msg) = serde_json::from_str(&body) {
                    self.on_page(view, &url, msg);
                }
            }
            UserEvent::Navigated { view, url } => self.navigated(view, url),
            UserEvent::PageLoaded(view) => {
                {
                    let mut shared = self.shared.lock().unwrap();
                    let Some(page) = shared.windows.get_mut(&self.win).and_then(|w| w.pages.get_mut(&view)) else { return };
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
            }
            UserEvent::Focused(view) => {
                if let Some((p, true)) = self.tabs.locate(view) {
                    self.focus_pane(p, false);
                }
            }
            UserEvent::Agent(request) => {
                match request {
                    AgentRequest::Open { path, raw, place, loose, reply } => {
                        let _ = reply.send(self.open(path, &raw, place, loose));
                    }
                    AgentRequest::OpenUrl { url, place, reply } => {
                        let _ = reply.send(Ok(self.open_in(url, place)));
                    }
                    AgentRequest::Close { target, reply } => {
                        let _ = reply.send(self.close_for_agent(target));
                    }
                    AgentRequest::Reload => self.act(Action::Reload),
                    AgentRequest::Eval { code, reply } => self.eval_for_agent(&code, reply),
                    AgentRequest::Screenshot { path, reply } => self.screenshot_for_agent(path, reply),
                    // Sent to the process, never to a window.
                    AgentRequest::NewWindow { reply, .. } => {
                        let _ = reply.send(Err(tr("Una ventana no abre otras.", "A window does not open others.").into()));
                    }
                }
            }
            UserEvent::Picked(Some(path)) => self.show_vault(Some(path)),
            UserEvent::Picked(None) => {}
            UserEvent::VaultChanged(changed) => self.vault_changed(changed),
            UserEvent::LooseChanged(changed) => self.loose_changed(changed),
            UserEvent::ThemeChanged => {
                self.apply_theme();
                self.announce_theme();
            }
            UserEvent::ResourceFailed { view, uri, reason } => self.resource_failed(view, &uri, reason),
            UserEvent::Fullscreen { view, on } => self.fullscreen(view, on),
            UserEvent::OpenTab { view, url, background } => self.open_tab(view, &url, background),
            UserEvent::Position { view, json } => {
                self.tabs.set_position(view, clean_position(&json));
            }
            UserEvent::MonoFont(font) => {
                self.common.borrow_mut().mono = font;
                self.apply_theme();
            }
            UserEvent::Action(action) => self.act(action),
            UserEvent::UpdateDone { message, installed } => self.update_done(&message, installed),
            // The process handles these; see `ui/visor.rs`.
            UserEvent::Quit | UserEvent::Restart { .. } | UserEvent::SaveConfig => {}
            UserEvent::RestartChecked(check) => match check {
                RestartCheck::Clear => self.restart(),
                RestartCheck::Unsaved => self.update_done(
                    tr("Hay notas sin guardar. Guárdalas antes de reiniciar.", "There are unsaved notes. Save them before restarting."),
                    true,
                ),
                RestartCheck::Unknown => self.update_done(
                    tr("No se pudo comprobar si hay notas sin guardar. No se reinicia.", "Could not check for unsaved notes. Not restarting."),
                    true,
                ),
            },
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Quit => self.proxy.to(0).send(UserEvent::Quit),
            Action::Reload => {
                if let Some(view) = self.view() {
                    let _ = view.reload();
                }
            }
            Action::Back => self.view().map_or((), |v| v.webview().go_back()),
            Action::Forward => self.view().map_or((), |v| v.webview().go_forward()),
            Action::PickVault => self.pick_vault(),
            Action::ToggleSidebar => self.sidebar_shown = !self.sidebar_shown,
            Action::FocusAddress => self.focus_address(),
            Action::AddressDone => {}
            Action::ToggleBar => {
                self.cfg_mut(|c| c.bar_shown = !c.bar_shown);
                self.save_config_soon();
            }
            Action::ZoomKey(step) => match self.tabs.focused_view().filter(|id| self.is_pdf_view(*id)) {
                Some(_) => self.eval(&page_calls::pdf_zoom(step)),
                None => self.act(Action::Zoom(step)),
            },
            Action::Zoom(step) => {
                let now = self.cfg(|c| c.zoom.unwrap_or(1.0));
                let zoom = match step {
                    0 => 1.0,
                    1 => ZOOM_STEPS.into_iter().find(|z| *z > now + 0.01).unwrap_or(now),
                    _ => ZOOM_STEPS.into_iter().rev().find(|z| *z < now - 0.01).unwrap_or(now),
                };
                self.cfg_mut(|c| c.zoom = Some(zoom).filter(|z| *z != 1.0));
                for (id, _) in &self.views {
                    self.apply_zoom(*id);
                }
                self.save_config_soon();
            }
            Action::SetTheme(choice) => {
                self.cfg_mut(|c| c.theme = choice);
                self.save_config_soon();
                self.apply_theme();
                self.announce_theme();
            }
            Action::SetAutoUpdate(on) => {
                self.cfg_mut(|c| c.auto_update = Some(on).filter(|on| !on));
                self.save_config_soon();
            }
            Action::SetSearchEngine(engine) => {
                self.cfg_mut(|c| c.search_engine = Some(engine).filter(|e| *e != Engine::ALL[0]).map(|e| e.id().to_string()));
                self.save_config_soon();
            }
            Action::SetLanguage(choice) => {
                self.language_pending(crate::i18n::would_use(choice.as_deref()) != crate::i18n::lang());
                self.cfg_mut(|c| c.language = choice);
                self.save_config_soon();
            }
            Action::SetShowOtherFiles(on) => {
                self.cfg_mut(|c| c.show_other_files = Some(on).filter(|on| !on));
                self.save_config_soon();
                self.refresh_tree();
            }
            Action::CheckUpdates => self.check_updates(),
            Action::Restart => self.ask_restart(),
            Action::Shortcuts => self.toggle_shortcuts(),
            Action::Find => self.find(),
            Action::FindNative => self.open_find(),
            Action::Settings => self.open_settings(),
            Action::LeaveAddress => {
                if let Some(view) = self.view() {
                    view.webview().grab_focus();
                }
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
                self.new_note(&dir);
            }
            Action::NewTab => {
                self.open_in(String::new(), Place::NewTab);
                self.focus_address();
            }
            Action::CloseTab(id) => {
                let effects = self.tabs.close(id);
                self.apply_and_publish(effects);
            }
            Action::CycleTab(step) => {
                let effects = self.tabs.cycle(step);
                self.apply_and_publish(effects);
            }
            Action::SelectNth(n) => {
                let effects = self.tabs.select_nth(n);
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
            Action::FocusSide(p) => {
                if p < self.tabs.sides().len() {
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
        let target = self.rel_of(address).unwrap_or_else(|| url.to_string());
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

    /// Runs a page function on the focused side, giving that page the keys.
    fn page_call(&self, code: &str) {
        if let Some(view) = self.view() {
            view.webview().grab_focus();
        }
        self.eval(code);
    }

    fn navigated(&mut self, view: u64, url: String) {
        let file = self.rel_of(&url);
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
            if let Some(page) = shared.windows.get_mut(&self.win).and_then(|w| w.pages.get_mut(&view)) {
                page.file = file.clone();
                page.url = (file.is_none() && is_web(&url)).then(|| url.clone());
                page.loaded = false;
                page.console.clear();
                page.orphans = None;
            }
        }
        self.apply_zoom(view);
        self.publish();
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
        let access = vault::access(url, self.vault().as_deref());
        let local = access != Access::None;
        let trusted = access == Access::Trusted;
        // A loose note may write its own file, and nothing else: `note_path` checks which.
        let note = trusted || access == Access::Local;
        let reply = |app: &App, code: String| {
            if let Some(v) = app.view_of(view) {
                let _ = v.evaluate_script(&code);
            }
        };
        match msg {
            PageMsg::PickVault if trusted || url.starts_with("about:") => self.pick_vault(),
            PageMsg::SaveNote { text } if note => {
                let result = self.save_note(url, &text);
                let (ok, error) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };
                reply(self, page_calls::note_saved(ok, &error));
            }
            PageMsg::SaveDrawing { text } if trusted => {
                let result = self.save_drawing(url, &text);
                let (ok, error) = match result {
                    Ok(()) => (true, String::new()),
                    Err(e) => (false, e),
                };
                reply(self, page_calls::drawing_saved(ok, &error));
            }
            PageMsg::SaveImage { file_id, data } if trusted => {
                let (ok, name, error) = match self.save_image(url, &file_id, &data) {
                    Ok(name) => (true, name, String::new()),
                    Err(e) => (false, String::new(), e),
                };
                reply(self, page_calls::image_saved(&file_id, ok, &name, &error));
            }
            PageMsg::ToggleTask { offset, checked } if note => {
                let error = self.toggle_task(url, offset, checked).err().unwrap_or_default();
                reply(self, page_calls::note_toggled(&error));
            }
            PageMsg::AnnotSave { annotation } if trusted => {
                match self.with_page_annotations(url, |root, page| annot::upsert(root, page, &annotation)) {
                    Ok(true) => {
                        let file = self.rel_of(url).unwrap_or_default();
                        let (id, text) = (annotation["id"].as_str().unwrap_or_default(), annotation["comment"].as_str().unwrap_or_default());
                        self.note_annotation(&file, id, "comment", text);
                    }
                    Ok(false) => {}
                    Err(e) => {
                        eprintln!("{}", trf!("No se pudo guardar la anotación: {e}", "Could not save the annotation: {e}"));
                        reply(self, page_calls::annot_save_failed(annotation["id"].as_str().unwrap_or_default(), &e));
                    }
                }
            }
            PageMsg::AnnotDelete { id } if trusted => {
                if let Err(e) = self.with_page_annotations(url, |root, page| annot::delete(root, page, &id)) {
                    eprintln!("{}", trf!("No se pudo borrar la anotación: {e}", "Could not delete the annotation: {e}"));
                    reply(self, page_calls::annot_delete_failed(&id, &e));
                }
            }
            PageMsg::AnnotReply { id, text } if trusted => {
                let file = self.rel_of(url).unwrap_or_default();
                match self.with_page_annotations(url, |root, page| annot::person_reply(root, page, &id, &text)) {
                    Ok(()) => self.note_annotation(&file, &id, "reply", &text),
                    // The page took the message as sent; it takes it back and shows why.
                    Err(e) => reply(self, page_calls::annot_reply_failed(&id, &text, &e)),
                }
            }
            PageMsg::AnnotStatus { orphans, total } if trusted => {
                if let Some(page) = self.shared.lock().unwrap().windows.get_mut(&self.win).and_then(|w| w.pages.get_mut(&view)) {
                    page.orphans = (total > 0).then_some(orphans);
                }
            }
            PageMsg::CreateNote { dir, name } if trusted => {
                if let Err(e) = self.create_note(&dir, vault_files::wiki_note_name(&name)) {
                    eprintln!("{}", trf!("No se pudo crear la nota: {e}", "Could not create the note: {e}"));
                    reply(self, page_calls::note_create_failed(&e));
                }
            }
            PageMsg::OpenPdfElsewhere if trusted => {
                // The file comes from the view's address, checked as a vault PDF, never from the message.
                let name = self.rel_of(url).unwrap_or_default();
                let file = self.open_vault().and_then(|v| vault::pdf::file_for(&v, url));
                match file {
                    Ok(file) => {
                        if !open_elsewhere(&file) {
                            self.notice(&files::no_application(&name), true);
                        }
                    }
                    Err(e) => self.notice(&e, true),
                }
            }
            PageMsg::Console { level, text } if local => {
                push_console(&mut self.shared.lock().unwrap(), self.win, view, &level, &text);
            }
            PageMsg::Event { data } if local => {
                let mut shared = self.shared.lock().unwrap();
                let file = shared.page_of(self.win, view).file.clone();
                push_event(&mut shared, json!({ "file": file, "data": data }), Some(self.win));
            }
            _ => {}
        }
    }

    /// Tells agents what the person wrote in an annotation. It carries
    /// `annotation` and no `data`: a page's own `visor.send` always has `data`,
    /// so it cannot pass as one of these.
    fn note_annotation(&self, file: &str, id: &str, action: &str, text: &str) {
        let mut shared = self.shared.lock().unwrap();
        push_event(&mut shared, json!({ "file": file, "annotation": { "id": id, "action": action, "text": text } }), Some(self.win));
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
    /// The note a page loaded from `url` may write: one of the vault, or a loose note's own file.
    fn note_path(&mut self, url: &str) -> Result<PathBuf, String> {
        if vault::access(url, self.vault().as_deref()) == Access::Local {
            return vault::loose_note(url, &self.shared.lock().unwrap().loose_folders());
        }
        Ok(self.open_vault()?.note(url)?.1)
    }

    fn save_note(&mut self, url: &str, text: &str) -> Result<(), String> {
        let path = self.note_path(url)?;
        self.own_write = Some(OwnWrite::new(&path, text));
        vault::write_atomic(&path, text).map_err(|e| e.to_string())
    }

    /// Writes a drawing. Only a drawing of the vault may be written, and only
    /// with text that is still a drawing, so a broken page cannot empty it.
    fn save_drawing(&mut self, url: &str, text: &str) -> Result<(), String> {
        let (_, path) = self.open_vault()?.drawing(url)?;
        if !vault::drawing::accepts(&path, text) {
            return Err(tr("El texto no es un dibujo: no se guarda.", "The text is not a drawing: not saved.").into());
        }
        self.own_write = Some(OwnWrite::new(&path, text));
        vault::write_atomic(&path, text).map_err(|e| e.to_string())
    }

    /// Saves an image pasted into the drawing at `url`, next to it. Rust picks the
    /// name and checks the bytes; the page only sends the image and its id.
    fn save_image(&mut self, url: &str, file_id: &str, data: &str) -> Result<String, String> {
        let valid_id = (1..=64).contains(&file_id.len()) && file_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid_id {
            return Err(tr("El identificador de la imagen no es válido.", "The image id is not valid.").into());
        }
        let (_, path) = self.open_vault()?.drawing(url)?;
        // Base64 spends 4 characters on every 3 bytes: a longer text is over the limit already.
        if data.len() > vault::drawing::MAX_IMAGE / 3 * 4 + 4 {
            return Err(tr("La imagen pesa más de 10 MB.", "The image is over 10 MB.").into());
        }
        let bytes = gtk::glib::base64_decode(data);
        let stamp = gtk::glib::DateTime::now_local()
            .and_then(|now| now.format("%Y-%m-%d %H%M%S"))
            .map_err(|_| tr("No se pudo leer la hora.", "Could not read the time.").to_string())?;
        vault::drawing::save_image(&path, &stamp, &bytes)
    }

    /// Ticks or unticks one task of the note at `url`. Rust re-reads the file and
    /// checks that a task really starts at `offset`, so the page cannot write anything else.
    fn toggle_task(&mut self, url: &str, offset: usize, checked: bool) -> Result<(), String> {
        let path = self.note_path(url)?;
        let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let text = crate::vault::markdown::toggle_task(&source, offset, checked).ok_or(tr("La tarea ya no está en la nota", "The task is no longer in the note"))?;
        self.save_note(url, &text)
    }

    fn create_folder(&mut self, dir: &str, name: &str) -> Result<(), String> {
        let (folder, name) = self.open_vault()?.new_in(dir, name)?;
        let path = folder.join(name);
        if path.exists() {
            return Err(trf!("Ya existe {name}", "{name} already exists"));
        }
        fs::create_dir(&path).map_err(|e| e.to_string())
    }

    /// Creates `name`.md in `dir` and opens it in the editor.
    fn create_note(&mut self, dir: &str, name: &str) -> Result<(), String> {
        let vault = self.open_vault()?;
        let (folder, name) = vault_files::note_target(&vault, dir, name)?;
        let file = if name.to_lowercase().ends_with(".md") { name.to_string() } else { format!("{name}.md") };
        let path = folder.join(&file);
        if path.exists() {
            return Err(trf!("Ya existe {file}", "{file} already exists"));
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
    fn open(&mut self, path: PathBuf, raw: &str, place: Place, loose: bool) -> Result<Option<Loading>, String> {
        let current = self.vault().map(|v| v.path().to_path_buf());
        let path = self.resolve_page(path, raw)?;
        if path.is_dir() {
            Config::check_vault(&path)?;
            self.show_vault(Some(path));
            return Ok(None);
        }
        let known = current.filter(|v| path.starts_with(v)).or_else(|| self.cfg(|c| c.vault_for(&path)));
        let target = match known {
            // Like a loose .html: a .md outside every vault, opened alone, with no folder added to the config.
            None if vault::is_html(&path) || vault::is_pdf(&path) || (loose && vault::is_markdown(&path)) => {
                // Turning its folder into a vault would list and watch all of it, maybe a whole home.
                if self.vault().is_none() {
                    self.sidebar_shown = false;
                }
                path.to_string_lossy().into_owned()
            }
            None if !vault::is_page(&path) => {
                return Err(tr("visor abre carpetas, páginas .html, notas .md y PDF.", "visor opens folders, .html pages, .md notes and PDFs.").into());
            }
            known => {
                let root = match known {
                    Some(root) => root,
                    None => {
                        let dir = path.parent().ok_or_else(|| trf!("Sin carpeta para {}", "No folder for {}", path.display()))?;
                        Config::check_folder_of_file(dir, dirs::home_dir().as_deref())?;
                        dir.to_path_buf()
                    }
                };
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
            .map_err(|_| trf!("No existe: {raw}", "Does not exist: {raw}"))
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
            self.cfg(|c| c.vault_for(p).is_some())
        });
        // The first name a tab shows wins; a deleted page is found by its lexical name.
        for name in &names {
            let (effects, closed) = self.tabs.close_showing(name);
            if closed > 0 {
                self.apply_and_publish(effects);
                return Ok(closed);
            }
        }
        Err(trf!("No hay ninguna pestaña abierta con {raw}.", "No tab is open with {raw}."))
    }

    /// The focused view, if agents may read its page, and what it committed. The
    /// decision uses the address WebKit committed, never the webview's own address.
    fn readable_view(&self) -> Result<(&WebView, Document), String> {
        let id = self.tabs.focused_view().ok_or(tr("No hay nada abierto.", "Nothing is open."))?;
        let (Some(view), Some(document)) = (self.view_of(id), self.documents.get(&id)) else {
            return Err(tr("No hay nada abierto.", "Nothing is open.").into());
        };
        readable(self.vault().as_deref(), document)?;
        Ok((view, document.clone()))
    }

    /// Runs the code in the focused page. WebKit answers on the main loop, so this
    /// returns at once and the reply is sent from the callback.
    fn eval_for_agent(&self, code: &str, reply: ValueReply) {
        let (view, document) = match self.readable_view() {
            Ok(found) => found,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let vault_base = self.vault_base();
        let script = page_calls::eval(code, [&vault_base, vault::loose_base()]);
        // run_javascript, not evaluate_javascript: the latter needs the v2_40 feature,
        // which visor does not enable (Cargo.toml asks for v2_38).
        let vault_now = self.vault();
        view.webview().run_javascript(&script, None::<&gtk::gio::Cancellable>, move |result| {
            // A website committed meanwhile would have answered: its words must not reach the agent.
            let answer = readable(vault_now.as_deref(), &document)
                .and_then(|()| result.map_err(|e| e.to_string()))
                .and_then(|r| r.js_value().map(|v| v.to_string()).ok_or_else(|| tr("La página no devolvió nada.", "The page returned nothing.").to_string()))
                .and_then(|raw| page_calls::eval_answer(&raw));
            let _ = reply.send(answer);
        });
    }

    /// Draws the visible part of the focused page into a PNG at `path`.
    fn screenshot_for_agent(&self, path: PathBuf, reply: SizeReply) {
        let (view, document) = match self.readable_view() {
            Ok(found) => found,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let vault_now = self.vault();
        view.webview().snapshot(SnapshotRegion::Visible, SnapshotOptions::NONE, None::<&gtk::gio::Cancellable>, move |result| {
            let answer = readable(vault_now.as_deref(), &document)
                .and_then(|()| result.map_err(|e| e.to_string()))
                .and_then(|surface| save_png(&surface, &path));
            let _ = reply.send(answer);
        });
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
        self.shared.lock().unwrap().window_mut(self.win).loose = dirs;
    }

    fn live_pages(&self) -> Vec<(u64, Option<String>, &str)> {
        self.tabs
            .live()
            .filter_map(|(id, target)| {
                let view = self.view_of(id)?;
                Some((id, self.rel_of(&view.url().unwrap_or_default()), target))
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
            self.refresh_tree();
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

/// Opens `file` in another application. Visor itself may be the system's
/// default for the file's type, and would only open it again, so it is skipped.
pub(super) fn open_elsewhere(file: &Path) -> bool {
    use gtk::gio;
    let (kind, _) = gio::content_type_guess(Some(file), &[]);
    let target = gio::File::for_path(file);
    gio::AppInfo::default_for_type(&kind, false)
        .into_iter()
        .chain(gio::AppInfo::recommended_for_type(&kind))
        .filter(|app| app.id().as_deref() != Some("visor.desktop"))
        .any(|app| app.launch(std::slice::from_ref(&target), None::<&gio::AppLaunchContext>).is_ok())
}
