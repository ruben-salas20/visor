//! The webview of one page: what it may navigate to, how it reports focus,
//! fullscreen and mouse buttons, and the scripts injected into it. What is the
//! same everywhere is here. Building the view, and what each engine does its
//! own way (WebKitGTK, WebView2), is in `page_view/<system>.rs`, behind
//! `build_page` and `PageView`.

use crate::i18n::tr;
use crate::vault;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

const BRIDGE: &str = include_str!("../page/bridge.js");
const ANNOT_QUOTE: &str = include_str!("../page/annot-quote.js");
const ANNOT: &str = include_str!("../page/annot.js");

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

/// The two scripts every page gets: the bridge, with the key only visor's
/// scripts know, and the annotations. One script for the annotations:
/// annot.js takes the quote helpers off window before any page script runs.
fn scripts(vault_base: &str, key: &str) -> [String; 2] {
    let lang = crate::i18n::lang().code();
    [
        BRIDGE
            .replace("__VISOR_BASE__", vault_base)
            .replace("__VISOR_LOOSE_BASE__", vault::loose_base())
            .replace("__VISOR_KEY__", key)
            .replace("__VISOR_LANG__", lang),
        format!("{ANNOT_QUOTE}\n{}", ANNOT.replace("__VISOR_BASE__", vault_base).replace("__VISOR_LANG__", lang)),
    ]
}

/// The message without its key, if visor's own scripts sent it. visor's start
/// page, written by the app itself without scripts, may only ask for a folder.
fn accept(body: &str, key: &str, document: &str) -> Option<String> {
    let mut msg: Value = serde_json::from_str(body).ok()?;
    let fields = msg.as_object_mut()?;
    match fields.remove("key") {
        Some(Value::String(k)) if k == key => {}
        None if document == EMPTY_PAGE && fields.get("type") == Some(&json!("pick_vault")) => {}
        _ => return None,
    }
    Some(msg.to_string())
}

/// The host of a website's address, the only part of it that reaches the
/// console. Path and query may carry anything, so «otro sitio» stands in for
/// any host that is not a plain name (IPv6, escapes, unicode, nothing at all).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = host.split(':').next().unwrap_or_default().to_ascii_lowercase();
    let plain = !host.is_empty() && host.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-');
    if plain { host } else { tr("otro sitio", "another site").to_string() }
}

/// The address of the document committed in a view, set by the engine's own
/// commit event: WebKit's `Committed`, WebView2's `ContentLoading` by navigation id.
pub type Document = Rc<RefCell<String>>;

/// A reading position as a page reported it, keeping only its numbers: the
/// page may be a website, and the value is later written into a script.
pub fn clean_position(json: &str) -> Option<String> {
    let value: Value = serde_json::from_str(json).ok()?;
    let y = value["y"].as_f64().filter(|y| y.is_finite())?;
    Some(match value["line"].as_u64() {
        Some(line) => json!({ "line": line, "y": y }).to_string(),
        None => json!({ "y": y }).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_website_is_named_by_its_host_only() {
        assert_eq!(host_of("https://cdn.jsdelivr.net/npm/x@1/a.css?v=2#f"), "cdn.jsdelivr.net");
        assert_eq!(host_of("http://localhost:59998/IGNORA/esto.css"), "localhost");
        assert_eq!(host_of("http://user:pass@Ejemplo.COM:8080"), "ejemplo.com");
        assert_eq!(host_of("https://a-b.example.org./x"), "a-b.example.org.");
    }

    #[test]
    fn anything_but_a_plain_host_becomes_otro_sitio() {
        assert_eq!(host_of("http://[::1]:8080/x.css"), "otro sitio");
        assert_eq!(host_of("http://%41.example/x"), "otro sitio");
        assert_eq!(host_of("http://ñandú.example/x"), "otro sitio");
        assert_eq!(host_of("http:///x.css"), "otro sitio");
        assert_eq!(host_of("http://user@/x"), "otro sitio");
        assert_eq!(host_of("http://exa_mple.com/"), "otro sitio");
        assert_eq!(host_of(""), "otro sitio");
    }

    #[test]
    fn only_messages_with_the_key_get_through() {
        let page = "http://127.0.0.1:1/s/nota.md";
        assert_eq!(accept(r#"{"type":"save_note","text":"a","key":"k"}"#, "k", page).as_deref(), Some(r#"{"text":"a","type":"save_note"}"#));
        assert_eq!(accept(r#"{"type":"save_note","text":"a"}"#, "k", page), None, "no key: another frame");
        assert_eq!(accept(r#"{"type":"save_note","text":"a","key":"x"}"#, "k", page), None);
        assert_eq!(accept(r#"{"type":"save_note","key":7}"#, "k", page), None);
        assert_eq!(accept("no es JSON", "k", page), None);
        assert_eq!(accept(r#"{"type":"pick_vault"}"#, "k", EMPTY_PAGE).as_deref(), Some(r#"{"type":"pick_vault"}"#));
        assert_eq!(accept(r#"{"type":"pick_vault"}"#, "k", page), None, "only the start page goes without a key");
        assert_eq!(accept(r#"{"type":"pick_vault"}"#, "k", "about:srcdoc"), None);
        assert_eq!(accept(r#"{"type":"pick_vault"}"#, "k", "data:text/html,<button>"), None);
        assert_eq!(accept(r#"{"type":"create_note","dir":"","name":"x"}"#, "k", EMPTY_PAGE), None);
    }

    #[test]
    fn position_keeps_only_numbers() {
        assert_eq!(clean_position(r#"{"y": 120.5}"#).as_deref(), Some(r#"{"y":120.5}"#));
        assert_eq!(clean_position(r#"{"line": 4, "y": 10}"#).as_deref(), Some(r#"{"line":4,"y":10.0}"#));
        // Extra fields from a website never reach the script that restores the position.
        assert_eq!(clean_position(r#"{"y": 1, "x": "alert(1)"}"#).as_deref(), Some(r#"{"y":1.0}"#));
        assert_eq!(clean_position(r#"{"line": "1); alert(1", "y": 2}"#).as_deref(), Some(r#"{"y":2.0}"#));
    }

    #[test]
    fn position_without_a_number_is_dropped() {
        assert_eq!(clean_position(r#"{"y": "3"}"#), None);
        assert_eq!(clean_position(r#"{"line": 3}"#), None);
        assert_eq!(clean_position("no es json"), None);
        assert_eq!(clean_position("null"), None);
    }
}
