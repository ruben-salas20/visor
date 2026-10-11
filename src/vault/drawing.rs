//! Drawings: the page that shows one in Excalidraw, the check a new text must
//! pass before it replaces the file, and the images pasted into one. The format
//! is read and written by `page/drawing-format.js`, which `tools/excalidraw/test`
//! covers.

use crate::vault::{self, markdown::ASSETS};
use serde_json::{Value, json};
use std::{fs, fs::OpenOptions, io::ErrorKind, path::Path};

/// The largest image a drawing may take in.
pub const MAX_IMAGE: usize = 10 * 1024 * 1024;

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

/// The extension of an image by its first bytes, or None. SVG is refused on
/// purpose: it can carry scripts, and the vault would serve it.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else {
        None
    }
}

/// The drawing's name without `.excalidraw.md`, cleaned of what a `[[link]]` or a
/// `|`/`#`/`^` in an embed would cut.
fn stem_of(drawing: &Path) -> String {
    let name = drawing.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let stem = if obsidian(&name) { &name[..name.len() - ".excalidraw.md".len()] } else { &name };
    let clean: String = stem.chars().filter(|c| !c.is_control() && !"[]|#^/\\".contains(*c)).collect();
    let clean = clean.trim().trim_start_matches('.').trim();
    if clean.is_empty() { "dibujo".into() } else { clean.into() }
}

/// Saves a pasted image next to the drawing at `drawing`, as `<name> <stamp>.<ext>`,
/// with ` 2`, ` 3`… before the extension if taken. Returns the file name. The name
/// is reserved with `create_new` first, so an existing file is never replaced.
pub fn save_image(drawing: &Path, stamp: &str, bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_IMAGE {
        return Err("La imagen pesa más de 10 MB.".into());
    }
    let ext = image_extension(bytes).ok_or("Solo se pueden añadir imágenes PNG, JPEG, WebP o GIF.")?;
    let dir = drawing.parent().ok_or("El dibujo no está en una carpeta.")?;
    let base = format!("{} {stamp}", stem_of(drawing));
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{base}.{ext}") } else { format!("{base} {n}.{ext}") };
        let path = dir.join(&name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => {
                // The empty reservation is ours: removing it is not removing a file of the vault.
                if let Err(e) = vault::write_atomic(&path, bytes) {
                    let _ = fs::remove_file(&path);
                    return Err(format!("No se pudo guardar la imagen: {e}"));
                }
                return Ok(name);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(format!("No se pudo crear la imagen: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn an_image_is_kept_only_by_its_bytes() {
        assert_eq!(image_extension(PNG), Some("png"));
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_extension(b"GIF89a...."), Some("gif"));
        assert_eq!(image_extension(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>x</script></svg>"), None);
        assert_eq!(image_extension(b"<html>"), None);
        assert_eq!(image_extension(b"RIFF\0\0\0\0WAVE"), None, "a RIFF that is not WebP");
        assert_eq!(image_extension(b""), None);
    }

    #[test]
    fn the_name_drops_what_a_link_cannot_hold() {
        assert_eq!(stem_of(Path::new("/v/mapa.excalidraw.md")), "mapa");
        assert_eq!(stem_of(Path::new("/v/a|b#c^d[e]f.excalidraw.md")), "abcdef");
        assert_eq!(stem_of(Path::new("/v/.excalidraw.md")), "dibujo");
    }

    /// A folder of the test's own, removed at the end.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_pasted_image_goes_next_to_the_drawing_and_never_replaces_one() {
        let dir = scratch("imagen-nombre");
        let drawing = dir.join("mapa.excalidraw.md");
        let stamp = "2026-10-10 153012";
        assert_eq!(save_image(&drawing, stamp, PNG).unwrap(), "mapa 2026-10-10 153012.png");
        assert_eq!(save_image(&drawing, stamp, PNG).unwrap(), "mapa 2026-10-10 153012 2.png");
        fs::write(dir.join("mapa 2026-10-10 153012 3.png"), "de otro").unwrap();
        assert_eq!(save_image(&drawing, stamp, PNG).unwrap(), "mapa 2026-10-10 153012 4.png");
        assert_eq!(fs::read_to_string(dir.join("mapa 2026-10-10 153012 3.png")).unwrap(), "de otro");
        assert_eq!(fs::read(dir.join("mapa 2026-10-10 153012.png")).unwrap(), PNG);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_svg_or_a_too_big_image_is_refused_and_leaves_no_file() {
        let dir = scratch("imagen-rechazo");
        let drawing = dir.join("mapa.excalidraw.md");
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        assert!(save_image(&drawing, "2026-10-10 153012", svg).is_err());
        let mut big = PNG.to_vec();
        big.resize(MAX_IMAGE, 0);
        assert!(save_image(&drawing, "2026-10-10 153012", &big).is_ok(), "exactly 10 MB is allowed");
        big.push(0);
        assert!(save_image(&drawing, "2026-10-10 153013", &big).unwrap_err().contains("10 MB"));
        assert!(!dir.join("mapa 2026-10-10 153013.png").exists());
        let _ = fs::remove_dir_all(&dir);
    }

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
