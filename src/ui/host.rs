//! The window on Linux: each side's page views below, in the hole the frame
//! announces for that side, and the frame's webview over them all.
//!
//! The frame is transparent in the holes, so the pages show through. Its input
//! shape is the zone `ui/cover.rs` computes: the window minus the holes, plus
//! the boxes the frame draws over the pages (its menus). The rest of the
//! pointer goes to the pages. A click on a page takes the focus from the frame,
//! whose `blur` closes its menus.
//!
//! The pages go in a `gtk::Fixed`. Each side has a box there that holds its
//! views; after the `Fixed` places its children, the boxes are moved to their
//! holes by hand. No size is requested: a requested size is a minimum, and
//! with one the window could not shrink below it (the U1 prototype's narrowed
//! pages and empty strip). In fullscreen, one side's box takes the whole
//! window, and the frame and the other sides hide.
//!
//! What the frame and Rust say is in `ui/protocol.rs`; nothing there depends on GTK.

use crate::i18n::tr;
use crate::ui::action::{Proxy, UserEvent};
use crate::ui::cover::{Px, Zone};
use crate::ui::protocol::{Hole, Rect, accept_frame};
use gtk::cairo;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use webkit2gtk::{LoadEvent, WebViewExt};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const BRIDGE: &str = include_str!("frame-bridge.js");

/// The container of one side's page views.
struct Slot {
    id: u64,
    views: gtk::Box,
}

type Holes = Rc<RefCell<HashMap<u64, Px>>>;

/// What the frame covers, and the zone last given to GTK.
#[derive(Default)]
struct Cover {
    boxes: Vec<Px>,
    dragging: bool,
    applied: Option<Zone>,
}

pub struct Host {
    frame: WebView,
    above: gtk::Box,
    fixed: gtk::Fixed,
    slots: Rc<RefCell<Vec<Slot>>>,
    holes: Holes,
    cover: RefCell<Cover>,
    /// The side whose page is fullscreen.
    fullscreen: Rc<Cell<Option<u64>>>,
    /// Painted under the pages, where a hole has none.
    background: Rc<Cell<(u8, u8, u8, u8)>>,
}

impl Host {
    /// Fills `window` with the frame, loaded from `base`, the frame's origin
    /// and secret prefix. The frame's orders reach the app through `proxy`.
    pub fn new(window: &tao::window::Window, context: &mut WebContext, background: (u8, u8, u8, u8), proxy: Proxy, base: &str) -> wry::Result<Host> {
        use tao::platform::unix::WindowExtUnix;
        let overlay = gtk::Overlay::new();
        let fixed = gtk::Fixed::new();
        let above = gtk::Box::new(gtk::Orientation::Vertical, 0);
        overlay.add(&fixed);
        overlay.add_overlay(&above);
        let vbox = window.default_vbox().ok_or_else(|| wry::Error::Io(std::io::Error::other("GTK")))?;
        vbox.pack_start(&overlay, true, true, 0);
        overlay.show_all();

        let background = Rc::new(Cell::new(background));
        {
            let background = background.clone();
            fixed.connect_draw(move |fixed, cr| {
                let (r, g, b, _) = background.get();
                cr.set_source_rgb(f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
                cr.rectangle(0.0, 0.0, f64::from(fixed.allocated_width()), f64::from(fixed.allocated_height()));
                let _ = cr.fill();
                gtk::glib::Propagation::Proceed
            });
        }
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
        let frame = WebViewBuilder::new_with_web_context(context)
            .with_transparent(true)
            .with_initialization_script(BRIDGE.replace("__VISOR_FRAME_KEY__", &key))
            .with_url(format!("{base}index.html"))
            .with_ipc_handler(move |request: Request<String>| {
                if let Some(msg) = accept_frame(request.body(), &key, &sender.borrow(), &frame_base) {
                    proxy.send(UserEvent::Frame(msg));
                }
            })
            // The frame never leaves its origin, nor opens windows.
            .with_navigation_handler(move |url| url.starts_with(&allowed))
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_devtools(true)
            .build_gtk(&above)?;
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
        Ok(Host { frame, above, fixed, slots, holes, cover: RefCell::default(), fullscreen, background })
    }

    /// Sends the frame a snapshot, a `protocol::ToFrame` as JSON.
    pub fn send(&self, json: &str) {
        let _ = self.frame.evaluate_script(&format!("window.__visorFrame.receive({json})"));
    }

    /// Gives the keyboard focus to the frame, so its fields take the typing.
    pub fn focus_frame(&self) {
        self.frame.webview().grab_focus();
    }

    pub fn set_background(&self, background: (u8, u8, u8, u8)) {
        self.background.set(background);
        self.fixed.queue_draw();
    }

    /// Adds the container of a new side, on the right. It shows once the frame
    /// says where it goes.
    pub fn add_side(&self, id: u64) {
        let views = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // A page view realized after the frame gets its window stacked over
        // the frame's: once it is shown, put the frame back on top.
        let above = self.above.clone();
        views.connect_add(move |_, _| {
            let above = above.clone();
            gtk::glib::idle_add_local_once(move || raise(&above));
        });
        self.fixed.put(&views, 0, 0);
        let slot = Slot { id, views };
        slot.views.set_visible(self.shown(id));
        self.slots.borrow_mut().push(slot);
    }

    /// Whether side `id`'s pages show: it has a hole, and no other side is fullscreen.
    fn shown(&self, id: u64) -> bool {
        self.holes.borrow().contains_key(&id) && self.fullscreen.get().is_none_or(|f| f == id)
    }

    /// Drops side `p`'s container, with whatever views it still has.
    pub fn remove_side(&self, p: usize) {
        let slot = self.slots.borrow_mut().remove(p);
        self.fixed.remove(&slot.views);
        self.holes.borrow_mut().remove(&slot.id);
        self.shape();
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
        self.above.set_visible(side.is_none());
        // GTK may give the frame new windows when it shows again.
        self.cover.borrow_mut().applied = None;
        self.relayout();
        self.shape();
    }

    fn relayout(&self) {
        for slot in self.slots.borrow().iter() {
            slot.views.set_visible(self.shown(slot.id));
        }
        place(&self.slots.borrow(), &self.holes.borrow(), self.fullscreen.get(), &self.fixed.allocation());
    }

    /// Gives the frame's windows the input zone, if it changed.
    fn shape(&self) {
        let view = self.frame.webview();
        if !view.is_realized() || self.fullscreen.get().is_some() {
            return;
        }
        let holes: Vec<Px> = self.holes.borrow().values().copied().collect();
        let mut cover = self.cover.borrow_mut();
        let zone = Zone::new(&holes, &cover.boxes, cover.dragging);
        if cover.applied.as_ref() == Some(&zone) {
            return;
        }
        let rect = |(x, y, w, h): Px| cairo::RectangleInt::new(x, y, w, h);
        let region = cairo::Region::create_rectangle(&rect(zone.base));
        for &hole in &zone.holes {
            let _ = region.subtract_rectangle(&rect(hole));
        }
        for &b in &zone.boxes {
            let _ = region.union_rectangle(&rect(b));
        }
        // GtkOverlay gives the frame's box a window of its own, and GDK sends
        // a click to the deepest window whose shape holds it: shape every
        // window from the view up to the toplevel, or one of them takes it.
        let top = view.toplevel().and_then(|t| t.window());
        let (mut dx, mut dy) = (0, 0);
        let mut window = view.window();
        while let Some(w) = window {
            if Some(&w) == top.as_ref() {
                break;
            }
            w.input_shape_combine_region(&region, dx, dy);
            let (x, y) = w.position();
            dx += x;
            dy += y;
            window = w.parent();
        }
        cover.applied = Some(zone);
    }
}

/// Puts the frame's windows over the pages'.
fn raise(above: &gtk::Box) {
    let top = above.toplevel().and_then(|t| t.window());
    let mut window = above.window();
    while let Some(w) = window {
        if Some(&w) == top.as_ref() {
            break;
        }
        w.raise();
        window = w.parent();
    }
}

/// Moves each container to its hole. `area` is the `Fixed`'s allocation, in
/// the coordinates its children are allocated in.
/// The side in `fullscreen` takes the whole area.
fn place(slots: &[Slot], holes: &HashMap<u64, Px>, fullscreen: Option<u64>, area: &gtk::Allocation) {
    for slot in slots {
        if fullscreen == Some(slot.id) {
            slot.views.size_allocate(area);
            continue;
        }
        let Some(&(x, y, w, h)) = holes.get(&slot.id) else { continue };
        slot.views.size_allocate(&gtk::Allocation::new(area.x() + x, area.y() + y, w, h));
    }
}
