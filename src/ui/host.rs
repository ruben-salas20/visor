//! The window on Linux: the frame's webview fills it, and each side's page
//! views sit on top, in the hole the frame announces for that side.
//!
//! The pages go in a `gtk::Fixed` laid over the frame. Each side has a box
//! there that holds its views; after the `Fixed` places its children, the
//! boxes are moved to their holes by hand. No size is requested: a requested
//! size is a minimum, and with one the window could not shrink below it (the
//! U1 prototype's narrowed pages and empty strip).
//!
//! What the frame and Rust say is in `ui/protocol.rs`; nothing there depends on GTK.

use crate::i18n::tr;
use crate::ui::action::{Proxy, UserEvent};
use crate::ui::protocol::{Hole, accept_frame};
use gtk::prelude::*;
use std::cell::RefCell;
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

type Holes = Rc<RefCell<HashMap<u64, (i32, i32, i32, i32)>>>;

pub struct Host {
    frame: WebView,
    fixed: gtk::Fixed,
    slots: Rc<RefCell<Vec<Slot>>>,
    holes: Holes,
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
        {
            let (slots, holes) = (slots.clone(), holes.clone());
            fixed.connect_size_allocate(move |_, area| place(&slots.borrow(), &holes.borrow(), area));
        }

        // A key only the bridge script in the frame's main frame knows.
        let key = crate::http::secret().ok_or_else(|| wry::Error::Io(std::io::Error::other(tr("sin números aleatorios", "no random numbers"))))?;
        // The address the frame committed, set by WebKit; see `accept_frame`.
        let document = Rc::new(RefCell::new(String::new()));
        let (sender, frame_base) = (document.clone(), base.to_string());
        let allowed = base.to_string();
        let frame = WebViewBuilder::new_with_web_context(context)
            .with_background_color(background)
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
        Ok(Host { frame, fixed, slots, holes })
    }

    /// Sends the frame a snapshot, a `protocol::ToFrame` as JSON.
    pub fn send(&self, json: &str) {
        let _ = self.frame.evaluate_script(&format!("window.__visorFrame.receive({json})"));
    }

    /// Gives the frame the keyboard, so a field in it can take what is typed.
    pub fn focus(&self) {
        self.frame.webview().grab_focus();
    }

    pub fn set_background(&self, background: (u8, u8, u8, u8)) {
        let _ = self.frame.set_background_color(background);
    }

    /// Adds the container of a new side, on the right. It shows once the frame
    /// says where it goes.
    pub fn add_side(&self, id: u64) {
        let views = gtk::Box::new(gtk::Orientation::Vertical, 0);
        self.fixed.put(&views, 0, 0);
        if self.holes.borrow().contains_key(&id) {
            views.show();
        }
        self.slots.borrow_mut().push(Slot { id, views });
    }

    /// Drops side `p`'s container, with whatever views it still has.
    pub fn remove_side(&self, p: usize) {
        let slot = self.slots.borrow_mut().remove(p);
        self.fixed.remove(&slot.views);
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
        for slot in self.slots.borrow().iter() {
            slot.views.set_visible(self.holes.borrow().contains_key(&slot.id));
        }
        place(&self.slots.borrow(), &self.holes.borrow(), &self.fixed.allocation());
    }
}

/// Moves each container to its hole. `area` is the `Fixed`'s allocation, in
/// the coordinates its children are allocated in.
fn place(slots: &[Slot], holes: &HashMap<u64, (i32, i32, i32, i32)>, area: &gtk::Allocation) {
    for slot in slots {
        let Some(&(x, y, w, h)) = holes.get(&slot.id) else { continue };
        slot.views.size_allocate(&gtk::Allocation::new(area.x() + x, area.y() + y, w, h));
    }
}
