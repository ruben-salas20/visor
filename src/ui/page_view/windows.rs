//! A page view on WebView2, a child window of the winit window that the host
//! places in its side's hole. What wry does not give is done with WebView2's
//! COM interface, as experiments W3 and W4 proved it.

use super::{Document, accept, scripts};
use crate::i18n::tr;
use crate::platform::open_external;
use crate::trf;
use crate::ui::action::{LoadFailure, Proxy, UserEvent};
use crate::ui::host::Host;
use crate::ui::shell::Window;
use crate::vault::is_web;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, ICoreWebView2, ICoreWebView2_2, ICoreWebView2_28, ICoreWebView2Environment15, ICoreWebView2Find,
};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, CapturePreviewCompletedHandler, ContainsFullScreenElementChangedEventHandler,
    ContentLoadingEventHandler, ExecuteScriptCompletedHandler, FindMatchCountChangedEventHandler, FindStartCompletedHandler, FocusChangedEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    WebResourceResponseReceivedEventHandler, take_pwstr,
};
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
use windows_core::{BOOL, HSTRING, Interface, PWSTR};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewExtWindows};

/// A page's webview, with what WebView2 does beyond wry. wry's own methods
/// (load, evaluate, reload) are reached through it.
pub struct PageView {
    web: WebView,
    shown: Cell<bool>,
    /// The theme's script for new documents; see `set_style`.
    style: Rc<RefCell<Style>>,
}

#[derive(Default)]
struct Style {
    /// Goes up with each new style, so a late answer for an old one is undone.
    generation: u64,
    script: Option<String>,
}

impl Deref for PageView {
    type Target = WebView;
    fn deref(&self) -> &WebView {
        &self.web
    }
}

/// `js` only in the main frame. WebView2 runs document scripts in every frame
/// (wry's "main frame only" does not hold there), and the bridge's key must
/// stay out of the frames a page embeds. Document scripts run before any of
/// the page's, so `window.top` is still the browser's.
pub(in crate::ui) fn main_frame_only(js: &str) -> String {
    format!("if (window.top === window) {{\n{js}\n}}")
}

/// `js` once the document has its root element. WebView2 runs document
/// scripts before there is one, and WebKit after; the annotations observe it.
/// The root is inserted before the parser reaches any script of the page, and
/// the observer runs at the checkpoint before that script, so `js` still runs first.
fn once_rooted(js: &str) -> String {
    format!(
        "(() => {{\nconst run = () => {{\n{js}\n}};\nif (document.documentElement) {{ run(); return; }}\nnew MutationObserver((_, o) => {{ if (document.documentElement) {{ o.disconnect(); run(); }} }}).observe(document, {{ childList: true }});\n}})();"
    )
}

fn com_error(e: windows_core::Error) -> wry::Error {
    wry::Error::Io(std::io::Error::other(e.to_string()))
}

/// Follows the document each navigation of `com` commits: the address its
/// `NavigationStarting` gave, set at its `ContentLoading`, matched by
/// navigation id. Never `Source()`, which follows `history.pushState`. A
/// document whose start was not seen commits nothing, so nothing trusts it.
/// `committed` hears each commit, and `finished` the end of the last
/// navigation started: an older one that ends late (the empty page, while a
/// note already loads) is not the page an agent waits for.
pub(in crate::ui) fn track_commits(
    com: &ICoreWebView2,
    document: Document,
    committed: impl Fn(&str) + 'static,
    finished: impl Fn() + 'static,
) -> windows_core::Result<()> {
    let pending: Rc<RefCell<HashMap<u64, String>>> = Rc::default();
    let latest = Rc::new(Cell::new(0u64));
    let (starts, started) = (pending.clone(), latest.clone());
    let on_start = NavigationStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let (mut uri, mut id, mut cancelled) = (PWSTR::null(), 0u64, BOOL(0));
        // SAFETY: COM getters on the live event arguments; take_pwstr frees the text.
        unsafe {
            args.Uri(&mut uri)?;
            args.NavigationId(&mut id)?;
            args.Cancel(&mut cancelled)?;
        }
        let uri = take_pwstr(uri);
        // Refused by the navigation handler, which runs first: it never loads.
        if cancelled.as_bool() {
            return Ok(());
        }
        started.set(id);
        // A redirect keeps its id and overwrites its address.
        starts.borrow_mut().insert(id, uri);
        Ok(())
    }));
    let on_done = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
        let mut id = 0u64;
        if let Some(args) = args {
            // SAFETY: a COM getter on the live event arguments.
            unsafe { args.NavigationId(&mut id)? };
        }
        if id == latest.get() {
            finished();
        }
        Ok(())
    }));
    let on_loading = ContentLoadingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut id = 0u64;
        // SAFETY: a COM getter on the live event arguments.
        unsafe { args.NavigationId(&mut id)? };
        let mut pending = pending.borrow_mut();
        let url = pending.remove(&id).unwrap_or_default();
        // Older navigations will not commit any more.
        pending.retain(|other, _| *other > id);
        drop(pending);
        document.replace(url.clone());
        committed(&url);
        Ok(())
    }));
    let mut token = 0i64;
    // SAFETY: registers handlers on a live view; they live as long as it does.
    unsafe {
        com.add_NavigationStarting(&on_start, &mut token)?;
        com.add_ContentLoading(&on_loading, &mut token)?;
        com.add_NavigationCompleted(&on_done, &mut token)?;
    }
    Ok(())
}

/// Sends a failed resource of a page view to the app. Only web addresses
/// count: data:, blob: and about: have no server to fail.
fn report_failure(proxy: &Proxy, view: u64, uri: String, reason: LoadFailure) {
    if is_web(&uri) {
        proxy.send(UserEvent::ResourceFailed { view, uri, reason });
    }
}

/// A page view in side `side`. Its events carry `id`, so the app knows which
/// tab they come from.
#[allow(clippy::too_many_arguments)]
pub fn build_page(
    window: &Window,
    host: &Host,
    side: usize,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: Proxy,
    vault_base: &str,
    id: u64,
) -> wry::Result<(PageView, Document)> {
    // A key only visor's scripts in the main frame know, while any frame can post to the handler.
    let key = crate::http::secret().ok_or_else(|| wry::Error::Io(std::io::Error::other(tr("sin números aleatorios", "no random numbers"))))?;
    // The document that was loaded, set when it commits. The webview's own
    // address follows history.pushState and pending loads, so it cannot say
    // which page sent a message.
    let document = Rc::new(RefCell::new(String::new()));
    let (ipc, sender) = (proxy.clone(), document.clone());
    let new_window = proxy.clone();
    let [bridge, annot] = scripts(vault_base, &key);
    let web = WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_visible(false)
        .with_bounds(wry::Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(0, 0).into() })
        .with_initialization_script(main_frame_only(&bridge))
        .with_initialization_script(main_frame_only(&once_rooted(&annot)))
        .with_ipc_handler(move |request: Request<String>| {
            let url = sender.borrow().clone();
            if let Some(body) = accept(request.body(), &key, &url) {
                ipc.send(UserEvent::Page { view: id, url, body });
            }
        })
        // target="_blank", window.open and Ctrl+click open a tab beside this one.
        .with_new_window_req_handler(move |url, _| {
            if is_web(&url) {
                new_window.send(UserEvent::OpenTab { view: id, url, background: false });
            } else if url.starts_with("mailto:") {
                open_external(&url);
            }
            NewWindowResponse::Deny
        })
        // The vault and the web load; mail goes to the mail app, and any other
        // scheme is refused rather than handed to the system.
        .with_navigation_handler(|url| {
            if url.starts_with("mailto:") {
                open_external(&url);
                return false;
            }
            is_web(&url) || ["about:", "data:", "blob:"].iter().any(|s| url.starts_with(s))
        })
        .with_devtools(true)
        .build_as_child(window.winit())?;
    let com = web.webview();

    // Followed here, not by the page script, so websites are tracked too.
    let p = proxy.clone();
    let done = proxy.clone();
    track_commits(&com, document.clone(), move |url| p.send(UserEvent::Navigated { view: id, url: url.to_string() }), move || done.send(UserEvent::PageLoaded(id)))
        .map_err(com_error)?;
    let p = proxy.clone();
    let on_focus = FocusChangedEventHandler::create(Box::new(move |_, _| {
        p.send(UserEvent::Focused(id));
        Ok(())
    }));
    // WebView2 grows the element to the whole view and says so; the app makes
    // the window fullscreen and puts this side's views over it.
    let p = proxy.clone();
    let on_fullscreen = ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
        let mut on = BOOL(0);
        if let Some(sender) = sender {
            // SAFETY: a COM getter on the live view.
            unsafe { sender.ContainsFullScreenElement(&mut on)? };
        }
        p.send(UserEvent::Fullscreen { view: id, on: on.as_bool() });
        Ok(())
    }));
    // HTTP errors of the page's resources. The main document is left out (a
    // failed page shows its own error), and so is the favicon Chromium asks for
    // on its own. Network errors would need the DevTools protocol: not reported yet.
    let (p, main) = (proxy, document.clone());
    let on_response = WebResourceResponseReceivedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let (mut uri, mut status) = (PWSTR::null(), 0i32);
        // SAFETY: COM getters on the live event arguments; take_pwstr frees the text.
        unsafe {
            args.Request()?.Uri(&mut uri)?;
            args.Response()?.StatusCode(&mut status)?;
        }
        let uri = take_pwstr(uri);
        if status >= 400 && uri != *main.borrow() && !uri.ends_with("/favicon.ico") {
            report_failure(&p, id, uri, LoadFailure::Status(status as u32));
        }
        Ok(())
    }));
    let controller = web.controller();
    window.catch_shortcuts(&controller);
    let mut token = 0i64;
    // SAFETY: registers handlers on a live view; they live as long as it does.
    unsafe {
        com.add_ContainsFullScreenElementChanged(&on_fullscreen, &mut token).map_err(com_error)?;
        controller.add_GotFocus(&on_focus, &mut token).map_err(com_error)?;
        if let Ok(com2) = com.cast::<ICoreWebView2_2>() {
            com2.add_WebResourceResponseReceived(&on_response, &mut token).map_err(com_error)?;
        }
    }
    host.attach(side, id, &web);
    Ok((PageView { web, shown: Cell::new(false), style: Rc::default() }, document))
}

impl PageView {
    /// Shows or hides the view; a hidden one stays loaded.
    pub fn set_shown(&self, shown: bool) {
        self.shown.set(shown);
        let _ = self.web.set_visible(shown);
    }

    pub fn is_shown(&self) -> bool {
        self.shown.get()
    }

    pub fn grab_focus(&self) {
        let _ = self.web.focus();
    }

    pub fn set_zoom(&self, zoom: f64) {
        let _ = self.web.zoom(zoom);
    }

    pub fn title(&self) -> Option<String> {
        let mut title = PWSTR::null();
        // SAFETY: a COM getter on the live view; take_pwstr frees the text.
        unsafe { self.web.webview().DocumentTitle(&mut title) }.ok()?;
        Some(take_pwstr(title)).filter(|t| !t.is_empty())
    }

    pub fn back(&self) {
        let _ = self.web.go_back();
    }

    pub fn forward(&self) {
        let _ = self.web.go_forward();
    }

    /// Puts `css` in every document this view loads from now on, and in the
    /// one it shows; or takes the theme's style out (`None`). WebView2 has no
    /// user style sheets: a `<style>` goes in at the page's level, so a page's
    /// own `!important` wins over the theme (W4), unlike in WebKit.
    pub fn set_style(&self, css: Option<&str>) {
        let com = self.web.webview();
        let generation = {
            let mut style = self.style.borrow_mut();
            style.generation += 1;
            if let Some(old) = style.script.take() {
                // SAFETY: a plain COM call on the live view.
                let _ = unsafe { com.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(old)) };
            }
            style.generation
        };
        let script = style_script(css.unwrap_or(""));
        let _ = self.web.evaluate_script(&script);
        if css.is_none() {
            return;
        }
        let (state, view) = (self.style.clone(), com.clone());
        let added = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |result, id| {
            if result.is_err() {
                return Ok(());
            }
            let mut style = state.borrow_mut();
            if style.generation == generation {
                style.script = Some(id);
            } else {
                // SAFETY: a plain COM call on the live view.
                let _ = unsafe { view.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id)) };
            }
            Ok(())
        }));
        // SAFETY: a plain COM call on the live view; the handler owns what it needs.
        let _ = unsafe { com.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(script), &added) };
    }

    /// Runs `script` in the page and gives its value as text. WebView2 answers
    /// on the loop, so this returns at once.
    pub fn run_script(&self, script: &str, done: impl FnOnce(Result<String, String>) + 'static) {
        // Straight to COM: wry's callback must be Send, and `done` holds the view's document.
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
            // WebView2 gives the value as JSON: the script's text is a JSON string.
            let nothing = || tr("La página no devolvió nada.", "The page returned nothing.").to_string();
            done(result.map_err(|e| e.to_string()).and_then(|()| serde_json::from_str::<String>(&json).map_err(|_| nothing())));
            Ok(())
        }));
        // SAFETY: a plain COM call on the live view; the handler owns what it needs.
        if let Err(e) = unsafe { self.web.webview().ExecuteScript(&HSTRING::from(script), &handler) } {
            eprintln!("{e}");
        }
    }

    /// Draws the visible part of the page, as PNG.
    pub fn snapshot(&self, done: impl FnOnce(Result<Vec<u8>, String>) + 'static) {
        // SAFETY: a new stream that this function owns, growing as WebView2 writes.
        let stream: IStream = match unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) } {
            Ok(stream) => stream,
            Err(e) => return done(Err(e.to_string())),
        };
        let written = stream.clone();
        let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
            done(result.map_err(|e| e.to_string()).and_then(|()| read_stream(&written)));
            Ok(())
        }));
        // SAFETY: a plain COM call on the live view; the handler owns the stream.
        if let Err(e) = unsafe { self.web.webview().CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler) } {
            eprintln!("{}", trf!("No se pudo capturar la página: {e}", "Could not capture the page: {e}"));
        }
    }

    /// WebView2's search in this page, for websites. Its counts reach the app
    /// as `FindFound` from `view`, up to `max`. None with a runtime too old for it.
    pub fn finder(&self, proxy: &Proxy, view: u64, max: u32) -> Option<(Finder, FindSignals)> {
        let com: ICoreWebView2_28 = self.web.webview().cast().ok()?;
        let environment: ICoreWebView2Environment15 = self.web.environment().cast().ok()?;
        // SAFETY: plain COM calls on the live view; the handler owns what it needs.
        unsafe {
            let find = com.Find().ok()?;
            let p = proxy.clone();
            let counted = FindMatchCountChangedEventHandler::create(Box::new(move |sender, _| {
                let mut count = 0;
                if let Some(sender) = sender {
                    sender.MatchCount(&mut count)?;
                }
                found(&p, view, max, count);
                Ok(())
            }));
            let mut token = 0i64;
            find.add_MatchCountChanged(&counted, &mut token).ok()?;
            Some((Finder { find: find.clone(), environment, proxy: proxy.clone(), view, max }, FindSignals { find, token }))
        }
    }
}

/// The script that puts the theme's style in a document, or empties it.
/// Put again at DOMContentLoaded so it comes after the page's own styles.
fn style_script(css: &str) -> String {
    let css = serde_json::to_string(css).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ const css = {css}; const put = () => {{ const root = document.head || document.documentElement; if (!root) return; let s = document.getElementById('__visor_theme'); if (!s) {{ s = document.createElement('style'); s.id = '__visor_theme'; }} s.textContent = css; root.appendChild(s); }}; put(); document.addEventListener('DOMContentLoaded', put, {{ once: true }}); }})();"
    )
}

fn read_stream(stream: &IStream) -> Result<Vec<u8>, String> {
    // SAFETY: reads a stream this module created, into a buffer of its size.
    unsafe {
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).map_err(|e| e.to_string())?;
        stream.Seek(0, STREAM_SEEK_SET, None).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; stat.cbSize as usize];
        let mut read = 0u32;
        stream.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut read)).ok().map_err(|e| e.to_string())?;
        buf.truncate(read as usize);
        Ok(buf)
    }
}

/// Runs WebView2's search in one page, without its own find dialog.
#[derive(Clone)]
pub struct Finder {
    find: ICoreWebView2Find,
    environment: ICoreWebView2Environment15,
    proxy: Proxy,
    view: u64,
    max: u32,
}

/// What `MatchCount` says, as the app counts it.
fn found(proxy: &Proxy, view: u64, max: u32, count: i32) {
    let n = count.max(0) as u32;
    proxy.send(UserEvent::FindFound { view, total: n.min(max), more: n > max });
}

impl Finder {
    pub fn search(&self, text: &str, match_case: bool, whole_word: bool) {
        // SAFETY: plain COM calls on live objects; the handler owns what it needs.
        unsafe {
            let Ok(options) = self.environment.CreateFindOptions() else { return };
            let _ = options.SetFindTerm(&HSTRING::from(text));
            let _ = options.SetIsCaseSensitive(match_case);
            let _ = options.SetShouldMatchWord(whole_word);
            let _ = options.SetSuppressDefaultFindDialog(true);
            let _ = options.SetShouldHighlightAllMatches(true);
            let (find, proxy, view, max) = (self.find.clone(), self.proxy.clone(), self.view, self.max);
            // A search with no match changes no count: the answer comes from here too.
            let started = FindStartCompletedHandler::create(Box::new(move |_| {
                let mut count = 0;
                if find.MatchCount(&mut count).is_ok() {
                    found(&proxy, view, max, count);
                }
                Ok(())
            }));
            let _ = self.find.Start(&options, &started);
        }
    }

    pub fn next(&self) {
        // SAFETY: a plain COM call on a live object.
        let _ = unsafe { self.find.FindNext() };
    }

    pub fn previous(&self) {
        // SAFETY: a plain COM call on a live object.
        let _ = unsafe { self.find.FindPrevious() };
    }

    /// Takes the highlights off.
    pub fn clear(&self) {
        // SAFETY: a plain COM call on a live object.
        let _ = unsafe { self.find.Stop() };
    }
}

/// The search's count handler, until the search ends.
pub struct FindSignals {
    find: ICoreWebView2Find,
    token: i64,
}

impl FindSignals {
    /// Stops the search and its handler.
    pub fn end(self) {
        // SAFETY: plain COM calls on a live object, with the token it gave.
        unsafe {
            let _ = self.find.Stop();
            let _ = self.find.remove_MatchCountChanged(self.token);
        }
    }
}
