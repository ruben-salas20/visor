//! The window on Linux: the frame's webview fills it, and each side's page
//! views sit on top, in the hole the frame announces for that side.
//!
//! The pages go in a `gtk::Fixed` laid over the frame. Each side has a box
//! there that holds its views; after the `Fixed` places its children, the
//! boxes are moved to their holes by hand. No size is requested: a requested
//! size is a minimum, and with one the window could not shrink below it (the
//! U1 prototype's narrowed pages and empty strip).
//!
//! While a file is dragged from the panel, each side's box also holds a drop
//! zone over its pages: the drag passes over the pages, never over the frame,
//! so only GTK can tell where it is. In fullscreen, one side's box takes the
//! whole window and the others hide.
//!
//! What the frame and Rust say is in `ui/protocol.rs`; nothing there depends on GTK.

use crate::i18n::tr;
use crate::ui::action::{Proxy, UserEvent};
use crate::ui::protocol::{Hole, accept_frame};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use webkit2gtk::{LoadEvent, WebViewExt};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const BRIDGE: &str = include_str!("frame-bridge.js");

/// The container of one side's page views, with its drop zone on top.
struct Slot {
    id: u64,
    root: gtk::Overlay,
    views: gtk::Box,
    drop: gtk::EventBox,
    /// Whether a drop takes the whole side (the window is split) or a half.
    whole: Rc<Cell<bool>>,
}

type Holes = Rc<RefCell<HashMap<u64, (i32, i32, i32, i32)>>>;

pub struct Host {
    frame: WebView,
    fixed: gtk::Fixed,
    slots: Rc<RefCell<Vec<Slot>>>,
    holes: Holes,
    /// The side whose page is fullscreen.
    fullscreen: Rc<Cell<Option<u64>>>,
    proxy: Proxy,
}

impl Host {
    /// Fills `window` with the frame, loaded from `base`, the frame's origin
    /// and secret prefix. The frame's orders reach the app through `proxy`.
    pub fn new(window: &tao::window::Window, context: &mut WebContext, background: (u8, u8, u8, u8), proxy: Proxy, base: &str) -> wry::Result<Host> {
        use tao::platform::unix::WindowExtUnix;
        let overlay = gtk::Overlay::new();
        let under = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let fixed = gtk::Fixed::new();
        overlay.add(&under);
        overlay.add_overlay(&fixed);
        // Clicks outside the pages reach the frame below.
        overlay.set_overlay_pass_through(&fixed, true);
        let vbox = window.default_vbox().ok_or_else(|| wry::Error::Io(std::io::Error::other("GTK")))?;
        vbox.pack_start(&overlay, true, true, 0);
        overlay.show_all();

        let slots: Rc<RefCell<Vec<Slot>>> = Rc::new(RefCell::new(Vec::new()));
        let holes: Holes = Rc::new(RefCell::new(HashMap::new()));
        let fullscreen = Rc::new(Cell::new(None));
        {
            let (slots, holes, fullscreen) = (slots.clone(), holes.clone(), fullscreen.clone());
            fixed.connect_size_allocate(move |_, area| place(&slots.borrow(), &holes.borrow(), fullscreen.get(), area));
        }

        // A key only the bridge script in the frame's main frame knows.
        let key = crate::http::secret().ok_or_else(|| wry::Error::Io(std::io::Error::other(tr("sin números aleatorios", "no random numbers"))))?;
        // The address the frame committed, set by WebKit; see `accept_frame`.
        let document = Rc::new(RefCell::new(String::new()));
        let (sender, frame_base) = (document.clone(), base.to_string());
        let allowed = base.to_string();
        let frame_proxy = proxy.clone();
        let frame = WebViewBuilder::new_with_web_context(context)
            .with_background_color(background)
            .with_initialization_script(BRIDGE.replace("__VISOR_FRAME_KEY__", &key))
            .with_url(format!("{base}index.html"))
            .with_ipc_handler(move |request: Request<String>| {
                if let Some(msg) = accept_frame(request.body(), &key, &sender.borrow(), &frame_base) {
                    frame_proxy.send(UserEvent::Frame(msg));
                }
            })
            // The frame never leaves its origin, nor opens windows.
            .with_navigation_handler(move |url| url.starts_with(&allowed))
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_devtools(true)
            .build_gtk(&under)?;
        let view = frame.webview();
        view.connect_load_changed(move |view, event| {
            if event == LoadEvent::Committed {
                document.replace(view.uri().map(|u| u.to_string()).unwrap_or_default());
            }
        });
        // WebKit's own menu (back, reload) means nothing in the frame.
        view.connect_context_menu(|_, _, _, _| true);
        // The frame has no state of its own: a new process asks for it again.
        view.connect_web_process_terminated(|view, _| view.reload());
        Ok(Host { frame, fixed, slots, holes, fullscreen, proxy })
    }

    /// Sends the frame a snapshot, a `protocol::ToFrame` as JSON.
    pub fn send(&self, json: &str) {
        let _ = self.frame.evaluate_script(&format!("window.__visorFrame.receive({json})"));
    }

    pub fn set_background(&self, background: (u8, u8, u8, u8)) {
        let _ = self.frame.set_background_color(background);
    }

    /// Adds the container of a new side, on the right. It shows once the frame
    /// says where it goes.
    pub fn add_side(&self, id: u64) {
        let root = gtk::Overlay::new();
        let views = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add(&views);
        views.show();
        let (drop, whole) = drop_zone(id, &self.proxy);
        root.add_overlay(&drop);
        self.fixed.put(&root, 0, 0);
        let slot = Slot { id, root, views, drop, whole };
        slot.root.set_visible(self.shown(id));
        self.slots.borrow_mut().push(slot);
    }

    /// Whether side `id`'s pages show: it has a hole, and no other side is fullscreen.
    fn shown(&self, id: u64) -> bool {
        self.holes.borrow().contains_key(&id) && self.fullscreen.get().is_none_or(|f| f == id)
    }

    /// Drops side `p`'s container, with whatever views it still has.
    pub fn remove_side(&self, p: usize) {
        let slot = self.slots.borrow_mut().remove(p);
        self.fixed.remove(&slot.root);
        self.holes.borrow_mut().remove(&slot.id);
    }

    pub fn swap_sides(&self) {
        self.slots.borrow_mut().swap(0, 1);
    }

    /// Where side `p`'s page views go. A handle to the box, which GTK keeps.
    pub fn views(&self, p: usize) -> gtk::Box {
        self.slots.borrow()[p].views.clone()
    }

    /// Puts each side's pages in its hole, now and on every later layout.
    pub fn set_holes(&self, holes: &[Hole]) {
        {
            let mut known = self.holes.borrow_mut();
            known.clear();
            known.extend(holes.iter().map(|h| (h.side, h.pixels())));
        }
        self.relayout();
    }

    /// Lets side `side`'s pages cover the window, or (`None`) puts every side back in its hole.
    pub fn set_fullscreen(&self, side: Option<u64>) {
        self.fullscreen.set(side);
        self.relayout();
    }

    fn relayout(&self) {
        for slot in self.slots.borrow().iter() {
            slot.root.set_visible(self.shown(slot.id));
        }
        place(&self.slots.borrow(), &self.holes.borrow(), self.fullscreen.get(), &self.fixed.allocation());
    }

    /// Shows or hides the drop zones over the pages. `whole`: a drop takes the
    /// side it falls on, not a half.
    pub fn show_drop(&self, on: bool, whole: bool) {
        for slot in self.slots.borrow().iter() {
            slot.whole.set(whole);
            slot.drop.set_visible(on);
        }
    }
}

/// Moves each container to its hole. `area` is the `Fixed`'s allocation, in
/// the coordinates its children are allocated in.
/// The side in `fullscreen` takes the whole area.
fn place(slots: &[Slot], holes: &HashMap<u64, (i32, i32, i32, i32)>, fullscreen: Option<u64>, area: &gtk::Allocation) {
    for slot in slots {
        if fullscreen == Some(slot.id) {
            slot.root.size_allocate(area);
            continue;
        }
        let Some(&(x, y, w, h)) = holes.get(&slot.id) else { continue };
        slot.root.size_allocate(&gtk::Allocation::new(area.x() + x, area.y() + y, w, h));
    }
}

/// The drop zone over a side's pages. It tells the app which zone is under
/// the pointer, only when it changes: 0 left half, 1 right half, 2 the whole
/// side (the window is split), or none once the pointer leaves.
fn drop_zone(side: u64, proxy: &Proxy) -> (gtk::EventBox, Rc<Cell<bool>>) {
    let zone = gtk::EventBox::new();
    zone.set_visible_window(false);
    zone.set_no_show_all(true);
    let whole = Rc::new(Cell::new(false));
    let zone_at = {
        let whole = whole.clone();
        move |zone: &gtk::EventBox, x: i32| if whole.get() { 2 } else { u8::from(x > zone.allocated_width() / 2) }
    };
    let last: Rc<Cell<Option<u8>>> = Rc::new(Cell::new(None));
    let hover = {
        let (proxy, last) = (proxy.clone(), last.clone());
        move |zone: Option<u8>| {
            if last.replace(zone) != zone {
                proxy.send(UserEvent::DropHover { side, zone });
            }
        }
    };
    // No target list: the drag comes from the frame's tree, and the app
    // already knows which file it carries.
    zone.drag_dest_set(gtk::DestDefaults::empty(), &[], gtk::gdk::DragAction::MOVE | gtk::gdk::DragAction::COPY);
    let (h, at) = (hover.clone(), zone_at.clone());
    zone.connect_drag_motion(move |zone, context, x, _, time| {
        h(Some(at(zone, x)));
        // Whatever the frame's tree allows: the file is opened, never moved.
        let allowed = context.actions();
        let action = if allowed.contains(gtk::gdk::DragAction::COPY) { gtk::gdk::DragAction::COPY } else { context.suggested_action() };
        context.drag_status(action, time);
        true
    });
    zone.connect_drag_leave(move |_, _, _| hover(None));
    let proxy = proxy.clone();
    zone.connect_drag_drop(move |zone, context, x, _, time| {
        last.set(None);
        let half = match zone_at(zone, x) {
            2 => None,
            h => Some(h as usize),
        };
        proxy.send(UserEvent::Dropped { side, half });
        context.drag_finish(true, false, time);
        true
    });
    (zone, whole)
}
