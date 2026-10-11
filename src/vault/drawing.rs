//! Drawings: the page that shows one in Excalidraw, and the check a new text
//! must pass before it replaces the file. The format is read and written by
//! `page/drawing-format.js`, which `tools/excalidraw/test` covers.

use crate::vault::{self, markdown::ASSETS};
use serde_json::{Value, json};
use std::path::Path;

const FORMAT: &str = include_str!("../page/drawing-format.js");
const SCRIPT: &str = include_str!("../page/drawing.js");
const STYLE: &str = include_str!("../page/drawing.css");

/// Obsidian's form, `.excalidraw.md`; otherwise the JSON of excalidraw.com.
fn obsidian(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".excalidraw.md")
}

/// The page of the drawing at `rel`. Excalidraw loads only here, never in a note.
pub fn page(rel: &str) -> String {
    let assets = format!("{}{ASSETS}", vault::base());
    let kind = if obsidian(rel) { "md" } else { "json" };
    let config = json!({ "kind": kind }).to_string();
    let fonts = json!(format!("{assets}excalidraw/")).to_string();
    let title = Path::new(rel).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    // The script tags carry the folder and file names: a `<` in them must not end the script.
    let escape = |json: String| json.replace('<', "\\u003c");
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title>\
<link rel=\"stylesheet\" href=\"{assets}excalidraw/excalidraw.css\"><style>{STYLE}</style></head><body>\
<div id=\"dibujo\"></div><div id=\"status\" role=\"status\" data-visor-skip></div>\
<script>window.EXCALIDRAW_ASSET_PATH = {fonts};window.__visorDrawingConfig = {config};</script>\
<script src=\"{assets}excalidraw/excalidraw.min.js\"></script>\
<script>{FORMAT}</script><script>{SCRIPT}</script></body></html>",
        title = crate::vault::markdown::escape(&title),
        config = escape(config),
        fonts = escape(fonts),
    )
}

/// Whether `text` may replace the drawing at `path`. A page with a bug must not
/// empty a drawing, so the text has to be a drawing too.
pub fn accepts(path: &Path, text: &str) -> bool {
    if obsidian(&path.to_string_lossy()) {
        let frontmatter = text.contains("excalidraw-plugin:");
        let section = text.lines().any(|line| matches!(line.trim_end(), "## Drawing" | "# Drawing"));
        return frontmatter && section;
    }
    serde_json::from_str::<Value>(text).ok().and_then(|v| v.get("elements").map(Value::is_array)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_drawing_may_replace_a_drawing() {
        let md = "---\nexcalidraw-plugin: parsed\n---\n\n# Excalidraw Data\n\n## Drawing\n```json\n{}\n```\n%%";
        assert!(accepts(Path::new("/v/a.excalidraw.md"), md));
        assert!(!accepts(Path::new("/v/a.excalidraw.md"), "# Nota\n\ntexto"), "a note is not a drawing");
        assert!(!accepts(Path::new("/v/a.excalidraw.md"), ""), "an empty file is not a drawing");
        assert!(accepts(Path::new("/v/a.excalidraw"), r#"{"type":"excalidraw","elements":[]}"#));
        assert!(!accepts(Path::new("/v/a.excalidraw"), r#"{"type":"excalidraw"}"#));
        assert!(!accepts(Path::new("/v/a.excalidraw"), "no es JSON"));
    }

    #[test]
    fn the_page_loads_excalidraw_only_for_drawings() {
        let md = super::page("a/b.excalidraw.md");
        assert!(md.contains("excalidraw/excalidraw.min.js"));
        assert!(md.contains(r#"{"kind":"md"}"#));
        assert!(md.contains("window.EXCALIDRAW_ASSET_PATH"));
        assert!(md.contains("<title>b.excalidraw</title>"));
        assert!(super::page("c.excalidraw").contains(r#"{"kind":"json"}"#));
    }

    #[test]
    fn a_name_cannot_close_the_script_tag() {
        let page = super::page("x</script><script>alert(1)//.excalidraw");
        assert!(!page.contains("</script><script>alert"));
    }
}
