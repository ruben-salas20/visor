//! The calls Rust makes into pages, as the JavaScript to run. Each one
//! names a function of `note.js`, `editor.js`, `annot.js` or `pdf.js` (or `bridge.js`),
//! so that a rename there shows up in one place here. Values go through
//! serde_json, so a page's text cannot break out of its string.

use crate::i18n::tr;
use serde_json::{Value, json};

/// Tells the editor whether its save worked.
pub fn note_saved(ok: bool, error: &str) -> String {
    format!("window.__visorNote && window.__visorNote.saved({ok}, {})", json!(error))
}

/// Tells a rendered note whether the task it ticked was written.
pub fn note_toggled(error: &str) -> String {
    format!("window.__visorNote && window.__visorNote.toggled({})", json!(error))
}

/// Tells a drawing's editor whether its save worked.
pub fn drawing_saved(ok: bool, error: &str) -> String {
    format!("window.__visorDrawing && window.__visorDrawing.saved({ok}, {})", json!(error))
}

/// Tells a drawing's editor whether its pasted image was saved, and under which name.
pub fn image_saved(file_id: &str, ok: bool, name: &str, error: &str) -> String {
    format!(
        "window.__visorDrawing && window.__visorDrawing.imageSaved({}, {ok}, {}, {})",
        json!(file_id),
        json!(name),
        json!(error)
    )
}

/// The zoom of visor, for the PDF viewer to scale its pages by (see `App::apply_zoom`).
pub fn pdf_zoom(zoom: f64) -> String {
    format!("window.__visorPdf && window.__visorPdf.zoom({zoom})")
}

/// Ctrl+E: opens or closes the editor of a note or a drawing.
pub fn note_toggle() -> &'static str {
    "(window.__visorNote || window.__visorDrawing)?.toggle()"
}

/// The note or drawing changed on disk: its editor keeps its text, otherwise it reloads.
pub fn note_changed() -> &'static str {
    "(window.__visorNote || window.__visorDrawing) ? (window.__visorNote || window.__visorDrawing).changed() : location.reload()"
}

/// Whether a note has text the user typed that is not saved yet. Pages that are
/// not notes answer false.
pub fn note_unsaved() -> &'static str {
    "!!(window.__visorNote && window.__visorNote.unsaved)"
}

/// Asks the page for its reading position as its tab is hidden. The answer
/// is untrusted until `clean_position` has seen it.
pub fn hide() -> &'static str {
    "(window.__visorNote || window.__visorDrawing) ? (window.__visorNote || window.__visorDrawing).hide() : { y: scrollY }"
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

/// Runs `code` for `visor eval`. The page's address is checked again here,
/// because the webview may have moved on to a website since the decision was made.
/// The result is a JSON string for `eval_answer`; the code's own value is
/// serialized inside the page, where a circular value can still be caught.
pub fn eval(code: &str, bases: [&str; 2]) -> String {
    let bases = json!(bases);
    let code = json!(code);
    let gone = json!(tr("La página ya no es de la bóveda ni un archivo suelto.", "The page is no longer in the vault or a loose file."));
    let promise = json!(tr("El resultado es una promesa; visor eval no espera promesas.", "The result is a promise; visor eval does not wait for promises."));
    format!(
        r#"(() => {{
  const fail = (error) => JSON.stringify({{ ok: false, error }});
  if (!{bases}.some((base) => base && location.href.startsWith(base)))
    return fail({gone});
  try {{
    const value = (0, eval)({code});
    if (value && typeof value.then === "function")
      return fail({promise});
    return JSON.stringify({{ ok: true, type: typeof value, value }});
  }} catch (e) {{
    return fail(e instanceof Error ? e.message : String(e));
  }}
}})()"#
    )
}

/// Reads what `eval` returned: the value, or the reason the code did not run.
pub fn eval_answer(raw: &str) -> Result<Value, String> {
    let answer: Value = serde_json::from_str(raw).map_err(|_| tr("La página devolvió algo que visor no entiende.", "The page returned something visor does not understand.").to_string())?;
    if answer["ok"] == true {
        // A value with no JSON form (undefined, a function) leaves its key out.
        Ok(json!({ "ok": true, "type": answer["type"], "value": answer.get("value").cloned().unwrap_or(Value::Null) }))
    } else {
        Err(answer["error"].as_str().unwrap_or(tr("El código falló.", "The code failed.")).to_string())
    }
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

    #[test]
    fn eval_quotes_the_code_as_one_string() {
        let script = eval("'a\"b'; </script>", ["http://127.0.0.1:1/", "http://127.0.0.2:1/"]);
        assert!(script.contains(r#"(0, eval)("'a\"b'; </script>")"#), "{script}");
        assert!(script.contains(r#"["http://127.0.0.1:1/","http://127.0.0.2:1/"]"#), "{script}");
    }

    #[test]
    fn eval_answers_are_read() {
        assert_eq!(eval_answer(r#"{"ok":true,"type":"number","value":2}"#).unwrap(), json!({ "ok": true, "type": "number", "value": 2 }));
        assert_eq!(eval_answer(r#"{"ok":true,"type":"undefined"}"#).unwrap(), json!({ "ok": true, "type": "undefined", "value": null }));
        assert_eq!(eval_answer(r#"{"ok":false,"error":"x is not defined"}"#), Err("x is not defined".into()));
        assert!(eval_answer("no es JSON").is_err());
        assert!(eval_answer("{}").is_err());
    }
}
