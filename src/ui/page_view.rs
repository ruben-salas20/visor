//! The webview of one page: what it may navigate to, how it reports focus,
//! fullscreen and mouse buttons, and the scripts injected into it.

use crate::i18n::tr;
use crate::ui::action::{Action, LoadFailure, UserEvent};
use crate::vault;
use gtk::prelude::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use tao::event_loop::EventLoopProxy;
use webkit2gtk::{LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, NavigationType, NetworkError, PolicyDecisionExt, PolicyDecisionType, PolicyError, URIRequestExt, URIResponseExt, WebContextExt, WebResource, WebResourceExt, WebViewExt};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const BRIDGE: &str = include_str!("../page/bridge.js");
const ANNOT_QUOTE: &str = include_str!("../page/annot-quote.js");
const ANNOT: &str = include_str!("../page/annot.js");

/// The message without its key, if visor's own scripts sent it. visor's start
/// page, written by the app itself without scripts, may only ask for a folder.
fn accept(body: &str, key: &str, document: &str) -> Option<String> {
    let mut msg: Value = serde_json::from_str(body).ok()?;
    let fields = msg.as_object_mut()?;
    match fields.remove("key") {
        Some(Value::String(k)) if k == key => {}
        None if document.starts_with("about:") && fields.get("type") == Some(&json!("pick_vault")) => {}
        _ => return None,
    }
    Some(msg.to_string())
}

pub fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
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

/// Sends a failed resource of a page view to the app. Only web addresses
/// count: data:, blob: and about: have no server to fail.
fn report_failure(proxy: &EventLoopProxy<UserEvent>, view: u64, resource: &WebResource, reason: LoadFailure) {
    if let Some(uri) = resource.uri().filter(|uri| is_web(uri)) {
        let _ = proxy.send_event(UserEvent::ResourceFailed { view, uri: uri.to_string(), reason });
    }
}

/// The address of the document committed in a view, set by WebKit's signal itself.
pub type Document = Rc<RefCell<String>>;

/// A page view inside `container`. Its events carry `id`, so the app knows
/// which tab they come from.
pub fn build_page(
    container: &gtk::Box,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: EventLoopProxy<UserEvent>,
    vault_base: &str,
    id: u64,
) -> wry::Result<(WebView, Document)> {
    // A key only visor's scripts in the main frame know: wry injects them there
    // alone, while any frame can post to the handler.
    let key = crate::http::secret().ok_or_else(|| wry::Error::Io(std::io::Error::other(tr("sin números aleatorios", "no random numbers"))))?;
    // The document that was loaded, set when it commits. The webview's own
    // address follows history.pushState and pending loads, so it cannot say
    // which page sent a message.
    let document = Rc::new(RefCell::new(String::new()));
    let committed = document.clone();
    let (ipc, sender) = (proxy.clone(), document.clone());
    let new_window = proxy.clone();
    let clicks = proxy.clone();
    let page = WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_initialization_script(
            BRIDGE
                .replace("__VISOR_BASE__", vault_base)
                .replace("__VISOR_LOOSE_BASE__", vault::loose_base())
                .replace("__VISOR_KEY__", &key)
                .replace("__VISOR_LANG__", crate::i18n::lang().code()),
        )
        // One script: annot.js takes the quote helpers off window before any page script runs.
        .with_initialization_script(format!(
            "{ANNOT_QUOTE}\n{}",
            ANNOT.replace("__VISOR_BASE__", vault_base).replace("__VISOR_LANG__", crate::i18n::lang().code())
        ))
        .with_ipc_handler(move |request: Request<String>| {
            let url = sender.borrow().clone();
            if let Some(body) = accept(request.body(), &key, &url) {
                let _ = ipc.send_event(UserEvent::Page { view: id, url, body });
            }
        })
        // target="_blank" and window.open open a tab beside this one.
        .with_new_window_req_handler(move |url, _| {
            if is_web(&url) {
                let _ = new_window.send_event(UserEvent::OpenTab { view: id, url, background: false });
            } else if url.starts_with("mailto:") {
                open_external(&url);
            }
            NewWindowResponse::Deny
        })
        .with_devtools(true)
        .build_gtk(container)?;
    let view = page.webview();
    // WebKit's PDF viewer takes its language from the context and knows es-ES but
    // not every region (es-CO falls back to English): visor's language, with a region
    // it knows. Set before the view loads anything; the context is shared, so this repeats a no-op.
    if let Some(context) = view.context() {
        let language = match crate::i18n::lang() {
            crate::i18n::Lang::Es => "es-ES",
            crate::i18n::Lang::En => "en-US",
        };
        context.set_preferred_languages(&[language]);
    }

    // Runs before WebKit's own handler and may change the event it gets.
    view.connect_scroll_event(|widget, event| {
        if event.direction() != gtk::gdk::ScrollDirection::Smooth {
            return gtk::glib::Propagation::Proceed;
        }
        if event.source_device().map(|device| device.source()) != Some(gtk::gdk::InputSource::Mouse) {
            return gtk::glib::Propagation::Proceed;
        }
        let (dx, dy) = event.delta();
        let dx = whole_pixel_delta(dx, widget.allocated_width());
        let dy = whole_pixel_delta(dy, widget.allocated_height());
        // SAFETY: GTK keeps the event alive and writable for the whole signal
        // emission; only its two delta fields are changed, and nothing else reads them meanwhile.
        unsafe {
            let raw = event.as_ptr() as *mut gtk::gdk::ffi::GdkEventScroll;
            (*raw).delta_x = dx;
            (*raw).delta_y = dy;
        }
        gtk::glib::Propagation::Proceed
    });

    // WebKit reports the failures of every frame's resources, external iframes
    // included, so the app filters them. The main document is left out: a failed
    // page shows its own error. The signals are connected per resource, and
    // successful ones send nothing.
    let reports = proxy.clone();
    view.connect_resource_load_started(move |view, resource, _| {
        if view.main_resource().as_ref() == Some(resource) {
            return;
        }
        let failed = reports.clone();
        resource.connect_failed(move |resource, error| {
            // Navigating away cancels the loads still running; that is not a failure.
            if error.matches(NetworkError::Cancelled) || error.matches(PolicyError::FrameLoadInterruptedByPolicyChange) {
                return;
            }
            report_failure(&failed, id, resource, LoadFailure::Network);
        });
        let finished = reports.clone();
        resource.connect_finished(move |resource| {
            let status = resource.response().map_or(0, |response| response.status_code());
            if status >= 400 {
                report_failure(&finished, id, resource, LoadFailure::Status(status));
            }
        });
    });

    // Clicked links may go to the vault or the web. Mail goes to the mail app,
    // and any other scheme is refused rather than handed to the system.
    // Only clicks are checked, so embedded iframes still load.
    view.connect_decide_policy(move |_, decision, kind| {
        if kind != PolicyDecisionType::NavigationAction && kind != PolicyDecisionType::NewWindowAction {
            return false;
        }
        let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else { return false };
        let Some(action) = nav.navigation_action() else { return false };
        if action.navigation_type() != NavigationType::LinkClicked {
            return false;
        }
        let uri = action.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
        if is_web(&uri) {
            // Ctrl+click and middle click open a tab in the background, as in a browser.
            let control = action.modifiers() & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;
            if !(control || action.mouse_button() == 2) {
                return false;
            }
            let _ = clicks.send_event(UserEvent::OpenTab { view: id, url: uri, background: true });
            decision.ignore();
            return true;
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
            LoadEvent::Committed => {
                let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
                document.replace(url.clone());
                UserEvent::Navigated { view: id, url }
            }
            LoadEvent::Finished => UserEvent::PageLoaded(id),
            _ => return,
        };
        let _ = p.send_event(event);
    });

    let p = proxy.clone();
    view.connect_focus_in_event(move |_, _| {
        let _ = p.send_event(UserEvent::Focused(id));
        gtk::glib::Propagation::Proceed
    });

    // WebKit makes the whole window fullscreen; the app hides its own chrome
    // so only the element (a video, usually) remains. Returning false lets
    // WebKit go on with its default handling.
    let p = proxy.clone();
    view.connect_enter_fullscreen(move |_| {
        let _ = p.send_event(UserEvent::Fullscreen { view: id, on: true });
        false
    });
    let p = proxy.clone();
    view.connect_leave_fullscreen(move |_| {
        let _ = p.send_event(UserEvent::Fullscreen { view: id, on: false });
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
    Ok((page, committed))
}

fn open_external(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

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

/// The wheel delta that makes WebKit scroll a whole number of pixels. WebKit
/// scrolls by `delta * step`, where `step = (size^(2/3))` truncated to an
/// integer and `size` is the view's height (or width). An odd step turns a
/// 1.5 step into .5 px, and the page is then drawn between pixels: blurry text.
fn whole_pixel_delta(delta: f64, size: i32) -> f64 {
    if size <= 0 {
        return delta;
    }
    let step = (size as f64).powf(2.0 / 3.0) as i32;
    if step == 0 {
        return delta;
    }
    (delta * step as f64).round() / step as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_step_lands_on_whole_pixels() {
        // Height 970 gives step 97: 1.5 * 97 = 145.5 is rounded to 146.
        let delta = whole_pixel_delta(1.5, 970);
        assert!(((delta * 97.0) - 146.0).abs() < 1e-9, "{delta}");
        // Height 1009 gives step 100: 1.5 already lands on whole pixels.
        assert_eq!(whole_pixel_delta(1.5, 1009), 1.5);
        // Height 971 gives step 98.
        assert_eq!(whole_pixel_delta(-1.5, 971), -1.5);
        // No size yet: the delta is left as it was.
        assert_eq!(whole_pixel_delta(1.5, 0), 1.5);
        assert_eq!(whole_pixel_delta(-1.5, -3), -1.5);
    }

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
        assert_eq!(accept(r#"{"type":"pick_vault"}"#, "k", "about:blank").as_deref(), Some(r#"{"type":"pick_vault"}"#));
        assert_eq!(accept(r#"{"type":"pick_vault"}"#, "k", page), None, "only the start page goes without a key");
        assert_eq!(accept(r#"{"type":"create_note","dir":"","name":"x"}"#, "k", "about:blank"), None);
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
