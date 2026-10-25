//! The window on Windows: every view is a WebView2 child window of the winit
//! window. Each side's page views sit in the hole the frame announces for that
//! side, and the frame's view lies over them all, transparent in the holes:
//! WebView2 blends sibling views (menus-encima prototype, removed from the tree; see git history).
//!
//! The frame's window has a region (`SetWindowRgn`): the zone `ui/cover.rs`
//! computes, the window minus the holes plus the boxes the frame draws over the
//! pages. The region clips both what the frame draws and where it takes the
//! pointer, so the rest of the pointer reaches the pages below.
//!
//! Unlike GTK, nothing lays the views out: the host places each one by hand,
//! in physical pixels, whenever the holes, the sides or the window size change.
//! Same methods as `host/linux.rs`.

use crate::i18n::tr;
use crate::ui::action::{Proxy, UserEvent};
use crate::ui::cover::{Px, Zone};
use crate::ui::page_view::{PageView, main_frame_only, track_commits};
use crate::ui::protocol::{Hole, Rect, accept_frame};
use crate::ui::shell::Window;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
use webview2_com::ProcessFailedEventHandler;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, HGDIOBJ, RGN_DIFF, RGN_OR, SetWindowRgn};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, HWND_TOP, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

const BRIDGE: &str = include_str!("../frame-bridge.js");

/// A page view as the host places it: its child window and its controller.
struct Placed {
    id: u64,
    hwnd: HWND,
    controller: ICoreWebView2Controller,
}

/// One side's page views.
struct Slot {
    id: u64,
    views: Vec<Placed>,
}

/// What the frame covers, and the zone last given to Windows.
#[derive(Default)]
struct Cover {
    boxes: Vec<Px>,
    dragging: bool,
    applied: Option<Zone>,
}

pub struct Host {
    frame: WebView,
    parent: HWND,
    slots: RefCell<Vec<Slot>>,
    holes: RefCell<HashMap<u64, Px>>,
    cover: RefCell<Cover>,
    /// The side whose page is fullscreen.
    fullscreen: Cell<Option<u64>>,
}

/// The winit window's own handle, the parent of every view.
fn hwnd_of(window: &winit::window::Window) -> wry::Result<HWND> {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => Ok(HWND(h.hwnd.get() as _)),
        _ => Err(wry::Error::Io(std::io::Error::other("HWND"))),
    }
}

impl Host {
    /// Fills `window` with the frame, loaded from `base`, the frame's origin
    /// and secret prefix. The frame's orders reach the app through `proxy`.
    pub fn new(window: &Window, context: &mut WebContext, _background: (u8, u8, u8, u8), proxy: Proxy, base: &str) -> wry::Result<Host> {
        let shell = window;
        let window = window.winit();
        let parent = hwnd_of(window)?;
        // A key only the bridge script in the frame's main frame knows.
        let key = crate::http::secret().ok_or_else(|| wry::Error::Io(std::io::Error::other(tr("sin números aleatorios", "no random numbers"))))?;
        // The address the frame committed, set by WebView2's own events; see `accept_frame`.
        let document = Rc::new(RefCell::new(String::new()));
        let (sender, frame_base) = (document.clone(), base.to_string());
        let allowed = base.to_string();
        let size = window.inner_size();
        let frame = WebViewBuilder::new_with_web_context(context)
            .with_transparent(true)
            .with_initialization_script(main_frame_only(&BRIDGE.replace("__VISOR_FRAME_KEY__", &key)))
            .with_url(format!("{base}index.html"))
            .with_bounds(wry::Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(size.width, size.height).into() })
            .with_ipc_handler(move |request: Request<String>| {
                if let Some(msg) = accept_frame(request.body(), &key, &sender.borrow(), &frame_base) {
                    proxy.send(UserEvent::Frame(msg));
                }
            })
            // The frame never leaves its origin, nor opens windows.
            .with_navigation_handler(move |url| url.starts_with(&allowed))
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            // As in the page views: only visor's shortcuts act.
            .with_browser_accelerator_keys(false)
            .with_devtools(true)
            .build_as_child(window)?;
        shell.catch_shortcuts(&frame.controller());
        let com = frame.webview();
        track_commits(&com, document, Rc::default(), None, |_| {}, || {}).map_err(|e| wry::Error::Io(std::io::Error::other(e.to_string())))?;
        // The frame has no state of its own: a new process asks for it again.
        let reload = ProcessFailedEventHandler::create(Box::new(|sender, _| {
            if let Some(view) = sender {
                // SAFETY: a plain COM call on the view that failed.
                let _ = unsafe { view.Reload() };
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: plain COM calls on a live view; the handler lives as long as it does.
        unsafe {
            // WebView2's own menu (back, reload) means nothing in the frame.
            if let Ok(settings) = com.Settings() {
                let _ = settings.SetAreDefaultContextMenusEnabled(false);
            }
            let _ = com.add_ProcessFailed(&reload, &mut token);
        }
        Ok(Host { frame, parent, slots: RefCell::default(), holes: RefCell::default(), cover: RefCell::default(), fullscreen: Cell::new(None) })
    }

    /// Sends the frame a snapshot, a `protocol::ToFrame` as JSON.
    pub fn send(&self, json: &str) {
        let _ = self.frame.evaluate_script(&format!("window.__visorFrame.receive({json})"));
    }

    /// Gives the keyboard focus to the frame, so its fields take the typing.
    pub fn focus_frame(&self) {
        let _ = self.frame.focus();
    }

    // Each page view paints its own background; with a page in every hole,
    // the window's own never shows.
    pub fn set_background(&self, _background: (u8, u8, u8, u8)) {}

    /// Adds the views of a new side, on the right. They show once the frame
    /// says where they go.
    pub fn add_side(&self, id: u64) {
        self.slots.borrow_mut().push(Slot { id, views: Vec::new() });
    }

    /// Drops side `p`, with whatever views it still has.
    pub fn remove_side(&self, p: usize) {
        let slot = self.slots.borrow_mut().remove(p);
        self.holes.borrow_mut().remove(&slot.id);
        self.shape();
    }

    pub fn swap_sides(&self) {
        self.slots.borrow_mut().swap(0, 1);
    }

    /// Takes a page view just built into side `p`, and keeps the frame above it.
    pub(in crate::ui) fn attach(&self, p: usize, id: u64, view: &WebView) {
        let placed = Placed { id, hwnd: view.hwnd(), controller: view.controller() };
        let side = self.slots.borrow()[p].id;
        self.place(&placed, side);
        self.slots.borrow_mut()[p].views.push(placed);
        self.raise_frame();
    }

    /// Moves a page view to side `to`; the page stays loaded.
    pub fn move_view(&self, _view: &PageView, id: u64, from: usize, to: usize) {
        let mut slots = self.slots.borrow_mut();
        let Some(i) = slots[from].views.iter().position(|v| v.id == id) else { return };
        let placed = slots[from].views.remove(i);
        let side = slots[to].id;
        self.place(&placed, side);
        slots[to].views.push(placed);
    }

    /// A page view was dropped: its window went with it.
    pub fn forget_view(&self, id: u64) {
        for slot in self.slots.borrow_mut().iter_mut() {
            slot.views.retain(|v| v.id != id);
        }
    }

    /// The window changed size: the frame takes the whole of it again.
    pub fn resized(&self) {
        let (w, h) = self.client_size();
        let _ = self.frame.set_bounds(wry::Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(w, h).into() });
        self.relayout();
        self.cover.borrow_mut().applied = None;
        self.shape();
    }

    /// Puts each side's pages in its hole, now and on every later change.
    pub fn set_holes(&self, holes: &[Hole]) {
        {
            let mut known = self.holes.borrow_mut();
            known.clear();
            known.extend(holes.iter().map(|h| (h.side, h.pixels())));
        }
        self.relayout();
        self.shape();
    }

    /// What the frame draws over the pages, and whether it drags something.
    pub fn set_cover(&self, boxes: &[Rect], dragging: bool) {
        {
            let mut cover = self.cover.borrow_mut();
            cover.boxes = boxes.iter().filter_map(Rect::pixels).collect();
            cover.dragging = dragging;
        }
        self.shape();
    }

    /// Lets side `side`'s pages cover the window, or (`None`) puts every side back in its hole.
    pub fn set_fullscreen(&self, side: Option<u64>) {
        self.fullscreen.set(side);
        let _ = self.frame.set_visible(side.is_none());
        self.cover.borrow_mut().applied = None;
        self.relayout();
        self.shape();
    }

    fn relayout(&self) {
        for slot in self.slots.borrow().iter() {
            for placed in &slot.views {
                self.place(placed, slot.id);
            }
        }
    }

    /// Puts one page view where its side goes: its hole, the whole window in
    /// fullscreen, or nowhere (an empty rectangle) while the side is not shown.
    fn place(&self, placed: &Placed, side: u64) {
        let rect = match self.fullscreen.get() {
            Some(f) if f == side => {
                let (w, h) = self.client_size();
                (0, 0, w, h)
            }
            Some(_) => (0, 0, 0, 0),
            None => match self.holes.borrow().get(&side) {
                Some(&hole) => self.physical(hole),
                None => (0, 0, 0, 0),
            },
        };
        let (x, y, w, h) = rect;
        // As wry's own set_bounds: the controller fills its child window, which moves.
        // SAFETY: plain calls on windows and a controller that live as long as the view.
        unsafe {
            let _ = placed.controller.SetBounds(RECT { left: 0, top: 0, right: w, bottom: h });
            let _ = SetWindowPos(placed.hwnd, None, x, y, w, h, SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOZORDER);
        }
    }

    /// The frame's window over every page view's.
    fn raise_frame(&self) {
        // SAFETY: a plain call on the frame's live window.
        unsafe {
            let _ = SetWindowPos(self.frame.hwnd(), Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// The window's inner size, in physical pixels.
    fn client_size(&self) -> (i32, i32) {
        let mut rect = RECT::default();
        // SAFETY: GetClientRect only writes the rectangle it is given.
        let _ = unsafe { GetClientRect(self.parent, &mut rect) };
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    /// The frame speaks in CSS pixels; windows are placed in physical ones.
    fn physical(&self, (x, y, w, h): Px) -> Px {
        // SAFETY: a plain call on the live parent window.
        let dpi = unsafe { GetDpiForWindow(self.parent) };
        let scale = if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 };
        let f = |v: i32| (f64::from(v) * scale).round() as i32;
        (f(x), f(y), f(w), f(h))
    }

    /// Gives the frame's window the zone as its region, if it changed.
    fn shape(&self) {
        if self.fullscreen.get().is_some() {
            return;
        }
        let holes: Vec<Px> = self.holes.borrow().values().copied().collect();
        let mut cover = self.cover.borrow_mut();
        let zone = Zone::new(&holes, &cover.boxes, cover.dragging);
        if cover.applied.as_ref() == Some(&zone) {
            return;
        }
        let rect = |px: Px| {
            let (x, y, w, h) = self.physical(px);
            // SAFETY: creates a new region that this function owns.
            unsafe { CreateRectRgn(x, y, x + w, y + h) }
        };
        // SAFETY: every region made here is either combined and deleted, or
        // handed to SetWindowRgn, which owns it from then on.
        unsafe {
            let region = rect(zone.base);
            for &hole in &zone.holes {
                let r = rect(hole);
                CombineRgn(Some(region), Some(region), Some(r), RGN_DIFF);
                let _ = DeleteObject(HGDIOBJ(r.0));
            }
            for &b in &zone.boxes {
                let r = rect(b);
                CombineRgn(Some(region), Some(region), Some(r), RGN_OR);
                let _ = DeleteObject(HGDIOBJ(r.0));
            }
            SetWindowRgn(self.frame.hwnd(), Some(region), true);
        }
        cover.applied = Some(zone);
    }
}
