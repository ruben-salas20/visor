//! A page view on WebKitGTK, inside its side's box in the host.

use super::{Document, accept, scripts};
use crate::i18n::tr;
use crate::platform::open_external;
use crate::trf;
use crate::ui::action::{Action, LoadFailure, Proxy, UserEvent};
use crate::ui::host::Host;
use crate::ui::shell::Window;
use crate::vault::is_web;
use gtk::glib::SignalHandlerId;
use gtk::prelude::*;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use webkit2gtk::{
    FindController, FindControllerExt, FindOptions, LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, NavigationType, NetworkError,
    PolicyDecisionExt, PolicyDecisionType, PolicyError, SnapshotOptions, SnapshotRegion, URIRequestExt, URIResponseExt, UserContentInjectedFrames,
    UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebContextExt, WebResource, WebResourceExt, WebViewExt,
};
use wry::http::Request;
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

/// The document visor's empty page commits as: WebKit gives `load_html` no other address.
pub const EMPTY_PAGE: &str = "about:blank";

/// A page's webview, with what WebKitGTK does beyond wry. wry's own methods
/// (load, evaluate, reload) are reached through it.
pub struct PageView(WebView);

impl Deref for PageView {
    type Target = WebView;
    fn deref(&self) -> &WebView {
        &self.0
    }
}

/// Sends a failed resource of a page view to the app. Only web addresses
/// count: data:, blob: and about: have no server to fail.
fn report_failure(proxy: &Proxy, view: u64, resource: &WebResource, reason: LoadFailure) {
    if let Some(uri) = resource.uri().filter(|uri| is_web(uri)) {
        proxy.send(UserEvent::ResourceFailed { view, uri: uri.to_string(), reason });
    }
}

/// A page view in side `side`'s box. Its events carry `id`, so the app knows
/// which tab they come from.
#[allow(clippy::too_many_arguments)]
pub fn build_page(
    _window: &Window,
    host: &Host,
    side: usize,
    context: &mut WebContext,
    background: (u8, u8, u8, u8),
    proxy: Proxy,
    vault_base: &str,
    id: u64,
) -> wry::Result<(PageView, Document)> {
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
    let [bridge, annot] = scripts(vault_base, &key);
    let page = WebViewBuilder::new_with_web_context(context)
        .with_background_color(background)
        .with_initialization_script(bridge)
        .with_initialization_script(annot)
        .with_ipc_handler(move |request: Request<String>| {
            let url = sender.borrow().clone();
            if let Some(body) = accept(request.body(), &key, &url) {
                ipc.send(UserEvent::Page { view: id, url, body });
            }
        })
        // target="_blank" and window.open open a tab beside this one.
        .with_new_window_req_handler(move |url, _| {
            if is_web(&url) {
                new_window.send(UserEvent::OpenTab { view: id, url, background: false });
            } else if url.starts_with("mailto:") {
                open_external(&url);
            }
            NewWindowResponse::Deny
        })
        .with_devtools(true)
        .build_gtk(&host.views(side))?;
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
            clicks.send(UserEvent::OpenTab { view: id, url: uri, background: true });
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
        p.send(event);
    });

    let p = proxy.clone();
    view.connect_focus_in_event(move |_, _| {
        p.send(UserEvent::Focused(id));
        gtk::glib::Propagation::Proceed
    });

    // WebKit makes the whole window fullscreen; the app hides its own chrome
    // so only the element (a video, usually) remains. Returning false lets
    // WebKit go on with its default handling.
    let p = proxy.clone();
    view.connect_enter_fullscreen(move |_| {
        p.send(UserEvent::Fullscreen { view: id, on: true });
        false
    });
    let p = proxy.clone();
    view.connect_leave_fullscreen(move |_| {
        p.send(UserEvent::Fullscreen { view: id, on: false });
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
        p.send(UserEvent::Action(action));
        gtk::glib::Propagation::Stop
    });
    Ok((PageView(page), committed))
}

impl PageView {
    /// Shows or hides the view; a hidden one stays loaded.
    pub fn set_shown(&self, shown: bool) {
        self.0.webview().set_visible(shown);
    }

    pub fn is_shown(&self) -> bool {
        self.0.webview().is_visible()
    }

    pub fn grab_focus(&self) {
        self.0.webview().grab_focus();
    }

    pub fn set_zoom(&self, zoom: f64) {
        self.0.webview().set_zoom_level(zoom);
    }

    pub fn title(&self) -> Option<String> {
        self.0.webview().title().map(|t| t.to_string())
    }

    pub fn back(&self) {
        self.0.webview().go_back();
    }

    pub fn forward(&self) {
        self.0.webview().go_forward();
    }

    /// Puts `css` in every frame of every page this view loads, at the user's
    /// level, or takes the theme's style out (`None`).
    pub fn set_style(&self, css: Option<&str>) {
        if let Some(manager) = self.0.webview().user_content_manager() {
            manager.remove_all_style_sheets();
            if let Some(css) = css {
                manager.add_style_sheet(&UserStyleSheet::new(css, UserContentInjectedFrames::AllFrames, UserStyleLevel::User, &[], &[]));
            }
        }
    }

    /// Runs `script` in the page and gives its value as text. WebKit answers on
    /// the main loop, so this returns at once.
    pub fn run_script(&self, script: &str, done: impl FnOnce(Result<String, String>) + 'static) {
        // run_javascript, not evaluate_javascript: the latter needs the v2_40 feature,
        // which visor does not enable (Cargo.toml asks for v2_38).
        self.0.webview().run_javascript(script, None::<&gtk::gio::Cancellable>, move |result| {
            done(
                result
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.js_value().map(|v| v.to_string()).ok_or_else(|| tr("La página no devolvió nada.", "The page returned nothing.").to_string())),
            );
        });
    }

    /// Draws the visible part of the page, as PNG.
    pub fn snapshot(&self, done: impl FnOnce(Result<Vec<u8>, String>) + 'static) {
        self.0.webview().snapshot(SnapshotRegion::Visible, SnapshotOptions::NONE, None::<&gtk::gio::Cancellable>, move |result| {
            done(result.map_err(|e| e.to_string()).and_then(|surface| png(&surface)));
        });
    }

    /// WebKit's search in this page, for websites. Its counts reach the app as
    /// `FindFound` from `view`, up to `max`.
    pub fn finder(&self, proxy: &Proxy, view: u64, max: u32) -> Option<(Finder, FindSignals)> {
        let c = self.0.webview().find_controller()?;
        let mut handlers = Vec::new();
        let p = proxy.clone();
        handlers.push(c.connect_found_text(move |_, n| {
            p.send(UserEvent::FindFound { view, total: n.min(max), more: n > max });
        }));
        let p = proxy.clone();
        handlers.push(c.connect_failed_to_find_text(move |_| {
            p.send(UserEvent::FindFound { view, total: 0, more: false });
        }));
        Some((Finder { controller: c.clone(), max }, FindSignals { controller: c, handlers }))
    }
}

/// Runs WebKit's search in one page.
#[derive(Clone)]
pub struct Finder {
    controller: FindController,
    max: u32,
}

impl Finder {
    pub fn search(&self, text: &str, match_case: bool, whole_word: bool) {
        self.controller.search(text, options(match_case, whole_word).bits(), self.max);
    }

    pub fn next(&self) {
        self.controller.search_next();
    }

    pub fn previous(&self) {
        self.controller.search_previous();
    }

    /// Takes the highlights off.
    pub fn clear(&self) {
        self.controller.search_finish();
    }
}

/// The search's signals, until the search ends.
pub struct FindSignals {
    controller: FindController,
    handlers: Vec<SignalHandlerId>,
}

impl FindSignals {
    /// Stops the search and its signals.
    pub fn end(self) {
        self.controller.search_finish();
        for id in self.handlers {
            self.controller.disconnect(id);
        }
    }
}

/// The options for the two checkboxes. WebKitGTK 2.0 has no flag for the end
/// of a word, so "whole word" only checks the start.
fn options(match_case: bool, whole_word: bool) -> FindOptions {
    let mut options = FindOptions::WRAP_AROUND;
    if !match_case {
        options |= FindOptions::CASE_INSENSITIVE;
    }
    if whole_word {
        options |= FindOptions::AT_WORD_STARTS;
    }
    options
}

/// Encodes a snapshot as PNG.
fn png(surface: &gtk::cairo::Surface) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    surface.write_to_png(&mut bytes).map_err(|e| trf!("No se pudo codificar la captura: {e}", "Could not encode the snapshot: {e}"))?;
    Ok(bytes)
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
    fn the_checkboxes_set_the_webkit_options() {
        let both = options(true, true);
        assert!(!both.contains(FindOptions::CASE_INSENSITIVE));
        assert!(both.contains(FindOptions::AT_WORD_STARTS));
        let neither = options(false, false);
        assert!(neither.contains(FindOptions::CASE_INSENSITIVE));
        assert!(!neither.contains(FindOptions::AT_WORD_STARTS));
        assert!(neither.contains(FindOptions::WRAP_AROUND));
    }
}
