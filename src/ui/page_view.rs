//! The webview of one page: what it may navigate to, how it reports focus,
//! fullscreen and mouse buttons, and the scripts injected into it.

use crate::ui::action::{Action, UserEvent};
use crate::vault;
use gtk::prelude::*;
use serde_json::{Value, json};
use std::process::Command;
use tao::event_loop::EventLoopProxy;
use webkit2gtk::{LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, NavigationType, PolicyDecisionExt, PolicyDecisionType, URIRequestExt, WebViewExt};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const BRIDGE: &str = include_str!("../page/bridge.js");
const ANNOT: &str = include_str!("../page/annot.js");

pub fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// A page view inside `container`. Its events carry `id`, so the app knows
/// which tab they come from.
pub fn build_page(
    container: &gtk::Box,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: EventLoopProxy<UserEvent>,
    id: u64,
) -> wry::Result<WebView> {
    let ipc = proxy.clone();
    let new_window = proxy.clone();
    let clicks = proxy.clone();
    let page = WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_initialization_script(BRIDGE.replace("__VISOR_BASE__", vault::base()))
        .with_initialization_script(ANNOT.replace("__VISOR_BASE__", vault::base()))
        .with_ipc_handler(move |request: Request<String>| {
            let url = request.uri().to_string();
            let _ = ipc.send_event(UserEvent::Page { view: id, url, body: request.into_body() });
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
            LoadEvent::Committed => UserEvent::Navigated { view: id, url: view.uri().map(|u| u.to_string()).unwrap_or_default() },
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
    Ok(page)
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

#[cfg(test)]
mod tests {
    use super::*;

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
