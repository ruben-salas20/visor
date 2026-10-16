//! The viewer of a PDF of the vault: the page that PDF.js draws. The bytes come
//! from the same address with `?raw` (see `http.rs`).

use crate::i18n::tr;
use crate::vault::{self, markdown::ASSETS};
use serde_json::json;
use std::path::{Path, PathBuf};

const SCRIPT: &str = include_str!("../page/pdf.js");
const STYLE: &str = include_str!("../page/pdf.css");

/// The page that shows the PDF at `rel`: the full viewer of PDF.js, loaded only
/// here and from the vault's own address. `pdf.js` puts its markup in the body.
pub fn page(rel: &str) -> String {
    let assets = format!("{}{ASSETS}pdfjs/", vault::base());
    let document = format!("{}?raw", vault::url(rel));
    let title = Path::new(rel).file_name().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let lang = crate::i18n::lang().code();
    // A `<` in a file name must not end the script tag.
    let config = json!({
        "locale": if lang == "es" { "es-ES" } else { "en-US" },
        "assets": &assets,
        "document": document,
        "texts": {
            "openElsewhere": tr("Abrir con otra aplicación", "Open with another application"),
            "failed": tr("No se pudo abrir el PDF.", "The PDF could not be opened."),
        }
    })
    .to_string()
    .replace('<', "\\u003c");
    format!(
        "<!doctype html><html lang=\"{lang}\" dir=\"ltr\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, maximum-scale=1\">\
<title>{title}</title>\
<link rel=\"resource\" type=\"application/l10n\" href=\"{assets}locale/locale.json\">\
<link rel=\"stylesheet\" href=\"{assets}viewer.css\"><style>{STYLE}</style></head><body tabindex=\"0\">\
<script>window.__visorPdfConfig = {config};</script><script>{SCRIPT}</script></body></html>",
        title = crate::vault::markdown::escape(&title),
    )
}

/// The PDF file that the page at `url` shows. A loose PDF and a website are
/// refused, as `Vault::page` refuses them.
pub fn file_for(vault: &vault::Vault, url: &str) -> Result<PathBuf, String> {
    let (_, path) = vault.page(url)?;
    pdf_file(path)
}

fn pdf_file(path: PathBuf) -> Result<PathBuf, String> {
    if vault::is_pdf(&path) {
        Ok(path)
    } else {
        Err(tr("La página no es un PDF.", "The page is not a PDF.").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A folder of the test's own, removed at the end.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_pdf_of_the_vault_is_the_file_shown() {
        let dir = scratch("pdf-boveda");
        fs::write(dir.join("libro.pdf"), b"%PDF-1.4").unwrap();
        let v = vault::Vault::open(&dir);
        let (_, path) = v.page_at("libro.pdf").unwrap();
        assert_eq!(pdf_file(path).unwrap().file_name().unwrap(), "libro.pdf");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_vault_pdf_of_the_view_opens_elsewhere() {
        let dir = scratch("pdf-abrir-fuera");
        fs::write(dir.join("libro.pdf"), b"%PDF-1.4").unwrap();
        fs::write(dir.join("nota.html"), "<p>x</p>").unwrap();
        let v = vault::Vault::open(&dir);
        assert!(file_for(&v, &vault::url("libro.pdf")).is_ok());
        assert!(file_for(&v, &vault::url("nota.html")).is_err());
        assert!(file_for(&v, "https://example.com/libro.pdf").is_err());
        assert!(file_for(&v, "http://127.0.0.2:1/s/libro.pdf").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_that_is_not_a_pdf_is_refused() {
        let dir = scratch("pdf-no-pdf");
        fs::write(dir.join("nota.html"), "<p>x</p>").unwrap();
        let v = vault::Vault::open(&dir);
        let (_, path) = v.page_at("nota.html").unwrap();
        assert!(pdf_file(path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pdf_outside_the_vault_hidden_or_loose_is_not_reached() {
        let dir = scratch("pdf-fuera");
        let outside = scratch("pdf-fuera-otra");
        fs::write(outside.join("secreto.pdf"), b"%PDF-1.4").unwrap();
        fs::create_dir_all(dir.join(".oculto")).unwrap();
        fs::write(dir.join(".oculto/x.pdf"), b"%PDF-1.4").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secreto.pdf"), dir.join("enlace.pdf")).unwrap();
        let v = vault::Vault::open(&dir);
        assert!(v.page_at(".oculto/x.pdf").is_err());
        assert!(v.page_at(&outside.join("secreto.pdf").to_string_lossy()).is_err());
        #[cfg(unix)]
        assert!(v.page_at("enlace.pdf").is_err());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn the_page_loads_the_viewer_and_reads_the_bytes_with_raw() {
        let html = page("libro.pdf");
        assert!(html.contains(&format!("\"document\":\"{}?raw\"", vault::url("libro.pdf"))));
        assert!(html.contains("__visor__/pdfjs/"));
        assert!(!html.contains("cdn.") && !html.contains("unpkg.com") && !html.contains("jsdelivr"));
    }

    #[test]
    fn a_name_cannot_close_the_script_tag() {
        // Two script tags of the page itself, and none from the name.
        let html = page("</script><b>x.pdf");
        assert_eq!(html.matches("</script>").count(), 2);
        assert!(!html.contains("<b>x"));
    }
}
