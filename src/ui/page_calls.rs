//! The calls Rust makes into pages, as the JavaScript to run. Each one
//! names a function of `note.js`, `editor.js` or `annot.js` (or `bridge.js`),
//! so that a rename there shows up in one place here. Values go through
//! serde_json, so a page's text cannot break out of its string.

use serde_json::json;

/// Tells the editor whether its save worked.
pub fn note_saved(ok: bool, error: &str) -> String {
    format!("window.__visorNote && window.__visorNote.saved({ok}, {})", json!(error))
}

/// Tells a rendered note whether the task it ticked was written.
pub fn note_toggled(error: &str) -> String {
    format!("window.__visorNote && window.__visorNote.toggled({})", json!(error))
}

/// Ctrl+E: opens or closes the editor of a note.
pub fn note_toggle() -> &'static str {
    "window.__visorNote && window.__visorNote.toggle()"
}

/// The note changed on disk: the editor keeps its text, a rendered note reloads.
pub fn note_changed() -> &'static str {
    "window.__visorNote ? window.__visorNote.changed() : location.reload()"
}

/// Asks the page for its reading position as its tab is hidden. The answer
/// is untrusted until `clean_position` has seen it.
pub fn hide() -> &'static str {
    "window.__visorNote ? __visorNote.hide() : { y: scrollY }"
}

/// Puts the page back at a position `clean_position` has cleaned.
pub fn place(position: &str) -> String {
    format!("(t => window.__visorNote ? __visorNote.place(t) : scrollTo(0, t.y))({position})")
}

pub fn annot_reload() -> &'static str {
    "window.__visorAnnot && window.__visorAnnot.reload()"
}

pub fn annot_pick_element() -> &'static str {
    "window.__visorAnnot && window.__visorAnnot.pickElement()"
}

pub fn annot_toggle_panel() -> &'static str {
    "window.__visorAnnot && window.__visorAnnot.togglePanel()"
}

/// The reply could not be saved: the page takes the message back and shows why.
pub fn annot_reply_failed(id: &str, text: &str, error: &str) -> String {
    format!(
        "window.__visorAnnot && window.__visorAnnot.replyFailed({}, {}, {})",
        json!(id),
        json!(text),
        json!(error)
    )
}

/// The annotation could not be saved: the page keeps the person's text and shows why.
pub fn annot_save_failed(id: &str, error: &str) -> String {
    format!("window.__visorAnnot && window.__visorAnnot.saveFailed({}, {})", json!(id), json!(error))
}

/// The annotation could not be deleted: the page puts it back and shows why.
pub fn annot_delete_failed(id: &str, error: &str) -> String {
    format!("window.__visorAnnot && window.__visorAnnot.deleteFailed({}, {})", json!(id), json!(error))
}

/// The note a `[[link]]` pointed at could not be created: the page says why.
pub fn note_create_failed(error: &str) -> String {
    format!("window.__visorNote && window.__visorNote.createFailed({})", json!(error))
}

/// The theme changed: the page's `--visor-*` variables already hold the new
/// values when the event runs. The script runs later than it is sent, maybe in
/// a website the view has gone to since; `window.visor` is only set in pages of visor.
pub fn theme_changed(dark: bool) -> String {
    let modo = if dark { "dark" } else { "light" };
    format!(r#"window.visor && window.dispatchEvent(new CustomEvent("visor:tema", {{ detail: {{ modo: {} }} }}))"#, json!(modo))
}

/// Frosted glass over a page to mark where a dragged file would land: a half
/// (0 left, 1 right), the whole page (any other zone), or nothing (`None`).
/// `accent` is a CSS colour from the theme.
pub fn frost(zone: Option<u8>, accent: &str) -> String {
    let (left, right) = match zone {
        Some(0) => ("8px", "calc(50% + 4px)"),
        Some(1) => ("calc(50% + 4px)", "8px"),
        _ => ("8px", "8px"),
    };
    format!(
        r#"(() => {{
  let d = document.getElementById("__visor_drop");
  if (!{on}) {{ d?.remove(); return; }}
  if (!d) {{ d = document.createElement("div"); d.id = "__visor_drop"; document.documentElement.append(d); }}
  d.style.cssText = "position:fixed;z-index:2147483647;pointer-events:none;top:8px;bottom:8px;left:{left};right:{right};"
    + "border-radius:10px;border:2px solid color-mix(in srgb, {accent} 70%, transparent);"
    + "background:color-mix(in srgb, {accent} 12%, transparent);"
    + "backdrop-filter:blur(12px) saturate(1.2);-webkit-backdrop-filter:blur(12px) saturate(1.2)";
}})()"#,
        on = zone.is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_escapes_the_error() {
        assert_eq!(
            note_saved(false, r#"no "se" pudo"#),
            r#"window.__visorNote && window.__visorNote.saved(false, "no \"se\" pudo")"#
        );
        assert_eq!(note_saved(true, ""), r#"window.__visorNote && window.__visorNote.saved(true, "")"#);
    }

    #[test]
    fn reply_failed_passes_every_argument_as_a_string() {
        assert_eq!(
            annot_reply_failed("a1", "hola\n</script>", "error"),
            r#"window.__visorAnnot && window.__visorAnnot.replyFailed("a1", "hola\n</script>", "error")"#
        );
    }

    #[test]
    fn failures_escape_what_they_carry() {
        assert_eq!(
            annot_save_failed("a1", "no \"va\"\n</script>"),
            r#"window.__visorAnnot && window.__visorAnnot.saveFailed("a1", "no \"va\"\n</script>")"#
        );
        assert_eq!(annot_delete_failed("a1", "x"), r#"window.__visorAnnot && window.__visorAnnot.deleteFailed("a1", "x")"#);
        assert_eq!(note_create_failed("Ya existe a.md"), r#"window.__visorNote && window.__visorNote.createFailed("Ya existe a.md")"#);
    }

    #[test]
    fn place_wraps_the_position() {
        assert!(place(r#"{"y":4.0}"#).ends_with(r#"({"y":4.0})"#));
    }

    #[test]
    fn theme_changed_names_the_mode() {
        assert_eq!(
            theme_changed(true),
            r#"window.visor && window.dispatchEvent(new CustomEvent("visor:tema", { detail: { modo: "dark" } }))"#
        );
        assert!(theme_changed(false).ends_with(r#"{ detail: { modo: "light" } }))"#));
    }

    #[test]
    fn frost_marks_the_zone_or_removes_the_mark() {
        let left = frost(Some(0), "#fff");
        assert!(left.contains("if (!true)") && left.contains("left:8px;right:calc(50% + 4px)"));
        assert!(frost(Some(1), "#fff").contains("left:calc(50% + 4px);right:8px"));
        assert!(frost(None, "#fff").contains("if (!false)"));
    }
}
