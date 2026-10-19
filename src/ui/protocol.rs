//! What the window says with its two kinds of webview: the frame (the
//! interface, a React app in `marco/`) and the pages. It knows no GTK, so it
//! compiles on every system.
//!
//! The frame only draws and asks. It asks with [`FrameMsg`]; Rust decides and
//! answers with [`ToFrame`]: whole snapshots of one area's state, which replace
//! the frame's copy. The TypeScript side is `marco/src/protocol.ts` and
//! `marco/src/areas/*/protocol.ts`; change both at once.
//!
//! Each area has its file here, for its messages and its snapshot:
//! `files` (A, the panel), `sides` (B, tabs and sides), `bar` (C, the address
//! row), `find` (D, the find bar) and `settings` (E, settings, shortcuts and
//! the status bar).

pub mod bar;
pub mod files;
pub mod find;
pub mod settings;
pub mod sides;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the frame asks for, tagged by `area` and then by `type`.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "area", rename_all = "snake_case")]
pub enum FrameMsg {
    Frame(Own),
    Files(crate::ui::action::SidebarMsg),
    Sides(sides::Msg),
    Bar(bar::Msg),
    Find(find::Msg),
    Settings(settings::Msg),
}

/// The frame's own messages, about the frame as a whole.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Own {
    /// The frame (re)started: it has no state and wants every snapshot.
    Ready,
    /// Where each side's pages go, in the frame's CSS pixels.
    Holes { holes: Vec<Hole> },
    /// What the frame draws over the pages (menus, dialogs), and whether it
    /// is dragging something. Sent only when it changes; see `ui/cover.rs`.
    Cover { boxes: Vec<Rect>, dragging: bool },
}

/// A rectangle of the frame, in its CSS pixels.
#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// The rectangle in whole pixels, or none if it is empty or not a number.
    pub fn pixels(&self) -> Option<(i32, i32, i32, i32)> {
        if [self.x, self.y, self.w, self.h].iter().any(|v| !v.is_finite()) || self.w < 0.5 || self.h < 0.5 {
            return None;
        }
        Some((self.x.round() as i32, self.y.round() as i32, self.w.round() as i32, self.h.round() as i32))
    }
}

/// The rectangle the frame leaves empty for the pages of one side.
#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
pub struct Hole {
    pub side: u64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Hole {
    /// The rectangle in whole pixels, never empty: GTK warns on a zero size.
    pub fn pixels(&self) -> (i32, i32, i32, i32) {
        let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
        (
            finite(self.x).round() as i32,
            finite(self.y).round() as i32,
            (finite(self.w).round() as i32).max(1),
            (finite(self.h).round() as i32).max(1),
        )
    }
}

/// What Rust sends the frame, tagged by `area`.
#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "area", rename_all = "snake_case")]
pub enum ToFrame {
    Theme(Colors),
    /// The vault's tree: large, so it goes apart from the rest of the panel.
    Tree(files::Tree),
    Files(files::State),
    Sides(sides::State),
    Bar(bar::State),
    Status(settings::Status),
}

impl ToFrame {
    /// Which copy in the frame this snapshot replaces.
    pub fn slot(&self) -> usize {
        match self {
            ToFrame::Theme(_) => 0,
            ToFrame::Tree(_) => 1,
            ToFrame::Files(_) => 2,
            ToFrame::Sides(_) => 3,
            ToFrame::Bar(_) => 4,
            ToFrame::Status(_) => 5,
        }
    }
}

/// How many kinds of snapshot there are; see [`ToFrame::slot`].
pub const SLOTS: usize = 6;

/// The five colors of visor's theme (`ui/theme.rs`). The frame mixes every
/// other color from them with `color-mix`.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Colors {
    pub dark: bool,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub muted: String,
    pub selection: String,
}

/// A frame message, if it carries the frame's key and the frame still shows
/// visor's interface. `document` is the address the frame's view committed,
/// never what the message says; `base` is the frame's origin and prefix.
pub fn accept_frame(body: &str, key: &str, document: &str, base: &str) -> Option<FrameMsg> {
    if base.is_empty() || !document.starts_with(base) {
        return None;
    }
    let mut msg: Value = serde_json::from_str(body).ok()?;
    let fields = msg.as_object_mut()?;
    match fields.remove("key") {
        Some(Value::String(k)) if k == key => {}
        _ => return None,
    }
    serde_json::from_value(msg).ok()
}

/// Messages from vault pages. Websites never get to send these. Tagged by
/// `type` and never by `area`, so no frame message passes for one of them.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PageMsg {
    Console { level: String, text: String },
    Event { data: Value },
    PickVault,
    /// From a note's editor: replaces that note's text.
    SaveNote { text: String },
    /// From a drawing's editor: replaces the whole file, in the form it had.
    SaveDrawing { text: String },
    /// From a drawing's editor: an image pasted or inserted in it, as base64.
    SaveImage { file_id: String, data: String },
    /// From a checkbox in a rendered note: the task whose `[ ]` starts at `offset`.
    ToggleTask { offset: usize, checked: bool },
    /// From a `[[link]]` to a note that does not exist yet.
    CreateNote { dir: String, name: String },
    /// From the PDF viewer's «Abrir con otra aplicación»: the PDF the view shows.
    OpenPdfElsewhere,
    AnnotSave { annotation: Value },
    AnnotDelete { id: String },
    /// The person's reply in the conversation of one of the page's annotations.
    AnnotReply { id: String, text: String },
    /// Which annotations the page could not place.
    AnnotStatus { orphans: Vec<String>, total: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://127.0.0.255:4000/abc/";
    const FRAME: &str = "http://127.0.0.255:4000/abc/index.html";

    #[test]
    fn the_frame_speaks_with_its_key() {
        let msg = accept_frame(r#"{"area":"sides","type":"select","tab":7,"key":"k"}"#, "k", FRAME, BASE);
        assert_eq!(msg, Some(FrameMsg::Sides(sides::Msg::Select { tab: 7 })));
        let ready = accept_frame(r#"{"area":"frame","type":"ready","key":"k"}"#, "k", FRAME, BASE);
        assert_eq!(ready, Some(FrameMsg::Frame(Own::Ready)));
    }

    #[test]
    fn a_frame_message_without_the_frame_key_is_refused() {
        let body = r#"{"area":"sides","type":"close","tab":7}"#;
        assert_eq!(accept_frame(body, "k", FRAME, BASE), None, "no key");
        let with_page_key = r#"{"area":"sides","type":"close","tab":7,"key":"clave-de-la-pagina"}"#;
        assert_eq!(accept_frame(with_page_key, "k", FRAME, BASE), None, "a page's key");
        assert_eq!(accept_frame(r#"{"area":"sides","type":"close","tab":7,"key":7}"#, "k", FRAME, BASE), None);
        assert_eq!(accept_frame("no es JSON", "k", FRAME, BASE), None);
    }

    #[test]
    fn the_frame_speaks_only_from_its_own_origin() {
        let body = r#"{"area":"files","type":"open","path":"a.md","tab":false,"beside":false,"key":"k"}"#;
        assert!(accept_frame(body, "k", FRAME, BASE).is_some());
        // The frame's view went elsewhere: a vault page, a loose page, a website, nothing.
        for document in ["http://127.0.0.1:4000/abc/index.html", "http://127.0.0.2:4000/x/", "https://ejemplo.com/", "about:blank", ""] {
            assert_eq!(accept_frame(body, "k", document, BASE), None, "{document}");
        }
        assert_eq!(accept_frame(body, "k", FRAME, ""), None, "no frame origin");
    }

    #[test]
    fn a_page_message_shaped_as_a_frame_order_goes_nowhere() {
        // What a page can post, with its own key already removed by page_view::accept.
        for body in [
            r#"{"area":"sides","type":"close","tab":1}"#,
            r#"{"area":"files","type":"trash","path":"a.md"}"#,
            r#"{"area":"frame","type":"holes","holes":[]}"#,
            r#"{"type":"select","tab":1}"#,
            r#"{"type":"trash","path":"a.md"}"#,
        ] {
            assert!(serde_json::from_str::<PageMsg>(body).is_err(), "{body}");
        }
        // A page message never passes for a frame one, even with the frame key.
        assert_eq!(accept_frame(r#"{"type":"save_note","text":"x","key":"k"}"#, "k", FRAME, BASE), None);
        assert_eq!(accept_frame(r#"{"type":"pick_vault","key":"k"}"#, "k", FRAME, BASE), None);
    }

    #[test]
    fn unknown_orders_are_refused() {
        assert_eq!(accept_frame(r#"{"area":"sides","type":"quit","key":"k"}"#, "k", FRAME, BASE), None);
        assert_eq!(accept_frame(r#"{"area":"agent","type":"eval","key":"k"}"#, "k", FRAME, BASE), None);
        assert_eq!(accept_frame(r#"{"area":"sides","type":"select","tab":"7","key":"k"}"#, "k", FRAME, BASE), None);
    }

    #[test]
    fn a_hole_is_whole_pixels_and_never_empty() {
        let hole = Hole { side: 1, x: 264.4, y: 56.6, w: 0.2, h: f64::NAN };
        assert_eq!(hole.pixels(), (264, 57, 1, 1));
    }

    #[test]
    fn the_frame_declares_what_covers_the_pages() {
        let body = r#"{"area":"frame","type":"cover","boxes":[{"x":220.4,"y":44,"w":240,"h":200}],"dragging":false,"key":"k"}"#;
        let Some(FrameMsg::Frame(Own::Cover { boxes, dragging })) = accept_frame(body, "k", FRAME, BASE) else { panic!("refused") };
        assert!(!dragging);
        assert_eq!(boxes[0].pixels(), Some((220, 44, 240, 200)));
        assert_eq!(Rect { x: 0.0, y: 0.0, w: 0.2, h: 9.0 }.pixels(), None);
        assert_eq!(Rect { x: f64::NAN, y: 0.0, w: 9.0, h: 9.0 }.pixels(), None);
        // Without the frame's key, or from a page, a cover goes nowhere.
        assert_eq!(accept_frame(r#"{"area":"frame","type":"cover","boxes":[],"dragging":true}"#, "k", FRAME, BASE), None);
        assert!(serde_json::from_str::<PageMsg>(r#"{"area":"frame","type":"cover","boxes":[],"dragging":true}"#).is_err());
    }

    #[test]
    fn snapshots_say_their_area() {
        let colors = Colors { dark: true, bg: "#000".into(), fg: "#fff".into(), accent: "#f80".into(), muted: "#777".into(), selection: "#320".into() };
        let json = serde_json::to_value(ToFrame::Theme(colors)).unwrap();
        assert_eq!(json["area"], "theme");
        assert_eq!(json["bg"], "#000");
    }
}
