//! Serves each open vault on a loopback address of its own (127.0.0.1, .3,
//! .4…), pages opened on their own on 127.0.0.2, and the frame (the window's
//! interface, `assets/marco/`) on 127.0.0.255, over plain HTTP.
//! WebKitGTK only plays video and offers Media Source to network origins,
//! not to custom schemes.

use crate::i18n::tr;
use crate::session::SharedRef;
use crate::trf;
use crate::vault::{self, Vault};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, ListenAddr, Request, Response, Server, StatusCode};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Where the server listens, as the app needs it.
pub struct Started {
    /// The port of every origin, kept so pages keep their origin and their storage.
    pub port: u16,
    /// URL prefix of loose pages, on an origin of their own.
    pub loose_base: String,
    /// URL prefix of the frame, on an origin no page shares.
    pub frame_base: String,
}

/// The frame's address: vaults take 127.0.0.1 and 3 to 254 (`Config::origin_for`), loose pages .2.
const FRAME_ADDRESS: &str = "127.0.0.255";

/// Where the frame's files are among the bundled assets.
const FRAME_ASSETS: &str = "marco/";

/// What one origin serves. Each vault gets an origin of its own, on its own
/// loopback address and with its own secret, so a page of one vault can read
/// neither another vault nor a loose page; loose pages share one more.
#[derive(Clone)]
enum Realm {
    Vault(Arc<Vault>),
    Loose,
    /// Only the frame's own files.
    Frame,
}

/// Starts the origin of loose pages. The preferred port is reused so pages
/// keep their origin, and with it their localStorage, between sessions.
pub fn start(preferred_port: Option<u16>, shared: SharedRef) -> Option<Started> {
    let loose = listen("127.0.0.2", preferred_port)?;
    let port = port_of(&loose)?;
    // Other programs on this machine can reach the port; without the secret
    // path they cannot read the pages.
    let prefix = format!("/{}/", secret()?);
    let loose_base = format!("http://127.0.0.2:{port}{prefix}");
    serve(loose, Realm::Loose, format!("127.0.0.2:{port}"), prefix, shared.clone());
    let frame = listen(FRAME_ADDRESS, Some(port))?;
    let frame_port = port_of(&frame)?;
    let frame_prefix = format!("/{}/", secret()?);
    let frame_base = format!("http://{FRAME_ADDRESS}:{frame_port}{frame_prefix}");
    serve(frame, Realm::Frame, format!("{FRAME_ADDRESS}:{frame_port}"), frame_prefix, shared);
    Some(Started { port, loose_base, frame_base })
}

/// Starts the origin of the vault at `root`, on `127.0.0.<host>` and the
/// same port as the others when it is free. It serves for as long as the
/// process runs: a vault opened again later keeps its address.
pub fn serve_vault(root: &Path, host: u8, port: u16, shared: SharedRef) -> Option<Arc<Vault>> {
    let address = format!("127.0.0.{host}");
    let server = listen(&address, Some(port))?;
    let port = port_of(&server)?;
    let prefix = format!("/{}/", secret()?);
    let vault = Arc::new(Vault::open_at(root, format!("http://{address}:{port}{prefix}")));
    serve(server, Realm::Vault(vault.clone()), format!("{address}:{port}"), prefix, shared);
    Some(vault)
}

fn listen(address: &str, port: Option<u16>) -> Option<Server> {
    port.and_then(|port| Server::http((address, port)).ok()).or_else(|| Server::http((address, 0)).ok())
}

fn port_of(server: &Server) -> Option<u16> {
    let ListenAddr::IP(addr) = server.server_addr() else { return None };
    Some(addr.port())
}

fn serve(server: Server, realm: Realm, host: String, prefix: String, shared: SharedRef) {
    let server = Arc::new(server);
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let (realm, shared, prefix, host) = (realm.clone(), shared.clone(), prefix.clone(), host.clone());
            thread::spawn(move || handle(request, &realm, &shared, &prefix, &host));
        }
    });
}

fn serve_asset(request: Request, name: &str) {
    let Some((_, bytes, brotli)) = ASSETS.iter().find(|(n, _, _)| *n == name) else {
        let _ = request.respond(text(404, tr("No encontrado", "Not found")));
        return;
    };
    let mut response = Response::from_data(*bytes)
        .with_header(header("Content-Type", vault::mime(Path::new(name))))
        .with_header(header("Cache-Control", "max-age=86400"));
    if *brotli {
        response = response.with_header(header("Content-Encoding", "br"));
    }
    let _ = request.respond(response);
}

/// 32 random hex digits.
pub fn secret() -> Option<String> {
    let mut bytes = [0u8; 16];
    crate::platform::random(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).expect("cabecera válida")
}

fn text(status: u16, message: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(message)
        .with_status_code(status)
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
}

fn handle(request: Request, realm: &Realm, shared: &SharedRef, prefix: &str, host: &str) {
    let value = |name: &str| {
        request
            .headers()
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str().to_string())
    };
    // A web page could point a hostname of its own at 127.0.0.1 (DNS rebinding);
    // the Host header still carries that hostname.
    if value("Host").as_deref() != Some(host) {
        let _ = request.respond(text(403, tr("Prohibido", "Forbidden")));
        return;
    }
    let url = request.url().split('#').next().unwrap_or("").to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let Some(rel) = path.strip_prefix(prefix) else {
        let _ = request.respond(text(404, tr("No encontrado", "Not found")));
        return;
    };
    if let Realm::Frame = realm {
        serve_asset(request, &format!("{FRAME_ASSETS}{rel}"));
        return;
    }
    // Both page origins serve the bundled libraries: a loose note's page loads them from its own origin.
    // The frame's files are the frame's alone.
    if let Some(name) = rel.strip_prefix(crate::vault::markdown::ASSETS)
        && !name.starts_with(FRAME_ASSETS)
    {
        serve_asset(request, name);
        return;
    }
    let rel = percent_encoding::percent_decode_str(rel).decode_utf8_lossy().into_owned();
    // Each origin serves only its own kind of page.
    if vault::is_loose(&rel) != matches!(realm, Realm::Loose) {
        let _ = request.respond(text(404, &trf!("No existe: {}", "Does not exist: {}", rel)));
        return;
    }
    let Realm::Vault(vault) = realm else {
        // Only the loose origin is left: the frame's was answered above.
        let allowed = shared.lock().unwrap().loose_folders();
        let range = value("Range");
        match vault::loose_file(Path::new(&rel), &allowed) {
            Some((path, folder)) if vault::is_markdown(&path) => {
                let _ = request.respond(loose_note(&path, &folder, query));
            }
            Some((path, _)) => serve_file(request, &path, range),
            None => {
                let _ = request.respond(text(404, &trf!("No existe: {}", "Does not exist: {}", rel)));
            }
        }
        return;
    };
    if let Some(page) = vault::annot::requested(&rel) {
        let response = match vault.page_at(page).map(|(page, _)| vault::annot::load(vault.root(), &page)) {
            Ok(Ok(doc)) => Response::from_string(doc.to_string()).with_header(header("Content-Type", "application/json")),
            Ok(Err(e)) => text(500, &e),
            Err(e) => text(404, &e),
        };
        let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
        return;
    }
    let Some(path) = vault.resolve(&rel) else {
        let _ = request.respond(text(404, &trf!("No existe: {}", "Does not exist: {}", rel)));
        return;
    };
    // Notes are rendered unless the editor asks for the source with `?raw`,
    // or where one of its `[[links]]` leads with `?wiki=target`. Drawings
    // share these queries; without `?raw` they get the Excalidraw page.
    if vault::is_markdown(&path) || vault::is_drawing(&path) {
        // `![[image]]` in the editor: the image file, wherever it is in the vault.
        if let Some(name) = query.split('&').find_map(|q| q.strip_prefix("embed=")) {
            let name = percent_encoding::percent_decode_str(name).decode_utf8_lossy();
            let response = match crate::vault::markdown::embed_url(&name, &vault.rel(&path), vault) {
                Some(url) => text(302, "").with_header(header("Location", &url)),
                None => text(404, tr("No existe", "Does not exist")),
            };
            let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
            return;
        }
        if let Some(target) = query.split('&').find_map(|q| q.strip_prefix("wiki=")) {
            let target = percent_encoding::percent_decode_str(target).decode_utf8_lossy();
            let response = match crate::vault::markdown::wiki_url(&target, &vault.rel(&path), vault) {
                Some(url) => text(200, &url),
                None => text(404, tr("No existe", "Does not exist")),
            };
            let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
            return;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            let _ = request.respond(text(500, tr("No se pudo leer la nota", "Could not read the note")));
            return;
        };
        let (body, mime) = if query.split('&').any(|q| q == "raw") {
            (source, "text/plain; charset=utf-8")
        } else if vault::is_drawing(&path) {
            (crate::vault::drawing::page(&vault.rel(&path), vault), "text/html; charset=utf-8")
        } else {
            (crate::vault::markdown::page(&source, &vault.rel(&path), vault), "text/html; charset=utf-8")
        };
        let response = Response::from_string(body)
            .with_header(header("Content-Type", mime))
            .with_header(header("Cache-Control", "no-store"));
        let _ = request.respond(response);
        return;
    }
    // A PDF opens in its viewer page, which reads the bytes back with `?raw`
    // (through `serve_file`, so PDF.js can ask for ranges).
    if vault::is_pdf(&path) && !query.split('&').any(|q| q == "raw") {
        let response = Response::from_string(crate::vault::pdf::page(&vault.rel(&path), vault))
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header("Cache-Control", "no-store"));
        let _ = request.respond(response);
        return;
    }
    let range = value("Range");
    serve_file(request, &path, range);
}

/// A loose note, answered like a vault note: rendered, or for the editor its
/// source (`?raw`) or where a link leads (`?wiki=`, `?embed=`). Its links reach
/// only its folder, as `loose_page` says.
fn loose_note(path: &Path, folder: &Path, query: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    use crate::vault::markdown;
    let rel = vault::rel(folder, path);
    let param = |name: &str| {
        query.split('&').find_map(|q| q.strip_prefix(name)).map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy().into_owned())
    };
    let response = if let Some(name) = param("embed=") {
        match markdown::loose_embed_url(&name, &rel, folder) {
            Some(url) => text(302, "").with_header(header("Location", &url)),
            None => text(404, tr("No existe", "Does not exist")),
        }
    } else if let Some(target) = param("wiki=") {
        match markdown::loose_wiki_url(&target, &rel, folder) {
            Some(url) => text(200, &url),
            None => text(404, tr("No existe", "Does not exist")),
        }
    } else {
        let Ok(source) = std::fs::read_to_string(path) else {
            return text(500, tr("No se pudo leer la nota", "Could not read the note"));
        };
        let (body, mime) = if query.split('&').any(|q| q == "raw") {
            (source, "text/plain; charset=utf-8")
        } else {
            (markdown::loose_page(&source, &rel, folder), "text/html; charset=utf-8")
        };
        Response::from_string(body).with_header(header("Content-Type", mime))
    };
    response.with_header(header("Cache-Control", "no-store"))
}

fn serve_file(request: Request, path: &Path, range: Option<String>) {
    let Ok(mut file) = File::open(path) else {
        let _ = request.respond(text(500, tr("No se pudo leer el archivo", "Could not read the file")));
        return;
    };
    let len = file.metadata().map_or(0, |m| m.len());
    let mut headers = vec![
        header("Content-Type", vault::mime(path)),
        header("Cache-Control", "no-store"),
        header("Accept-Ranges", "bytes"),
    ];
    let range = range.and_then(|r| vault::byte_range(&r, len));
    let (status, start, size) = match range {
        Some((start, end)) => {
            headers.push(header("Content-Range", &format!("bytes {start}-{end}/{len}")));
            (206, start, end - start + 1)
        }
        None => (200, 0, len),
    };
    if file.seek(SeekFrom::Start(start)).is_err() {
        let _ = request.respond(text(500, tr("No se pudo leer el archivo", "Could not read the file")));
        return;
    }
    let body = file.take(size);
    let _ = request.respond(Response::new(StatusCode(status), headers, body, Some(size as usize), None));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loose_pages_found_with_nfc_names() {
        let dir = std::env::temp_dir().join(format!("visor-loose-nfc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let nfd = dir.join("Pra\u{301}ctica");
        fs::create_dir_all(&nfd).unwrap();
        fs::write(nfd.join("Ejercicios de Pra\u{301}ctica.pdf"), "").unwrap();
        let root = crate::platform::canonicalize(&dir).unwrap();
        let allowed = [root.join("Pra\u{301}ctica")];
        let nfc = format!("{}/Práctica/Ejercicios de Práctica.pdf", root.display());
        let (found, _) = vault::loose_file(Path::new(&nfc), &allowed).expect("NFC spelling");
        assert_eq!(found, root.join("Pra\u{301}ctica/Ejercicios de Pra\u{301}ctica.pdf"));
        let nfd_name = format!("{}/Pra\u{301}ctica/Ejercicios de Pra\u{301}ctica.pdf", root.display());
        assert!(vault::loose_file(Path::new(&nfd_name), &allowed).is_some(), "NFD spelling");
        assert!(vault::loose_file(Path::new(&format!("{}/Práctica/../Otra/x.pdf", root.display())), &allowed).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn loose_pages_read_their_folder_but_nothing_hidden() {
        let dir = std::env::temp_dir().join(format!("visor-loose-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for sub in ["pagina/css", "pagina/.oculta", "otra"] {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
        for file in ["pagina/x.html", "pagina/css/e.css", "pagina/.oculta/s.txt", "pagina/.s.txt", "otra/y.html"] {
            fs::write(dir.join(file), "").unwrap();
        }
        let root = crate::platform::canonicalize(&dir).unwrap();
        std::os::unix::fs::symlink(root.join("otra/y.html"), root.join("pagina/enlace.html")).unwrap();
        let allowed = [root.join("pagina")];
        let reads = |rel: &str| vault::loose_file(&root.join(rel), &allowed).is_some();
        assert!(vault::loose_file(&root.join("pagina/x.html"), &allowed).is_some_and(|(_, folder)| folder == root.join("pagina")));
        assert!(reads("pagina/x.html"));
        assert!(reads("pagina/css/e.css"), "subfolders");
        assert!(!reads("pagina/.oculta/s.txt"), "hidden folder");
        assert!(!reads("pagina/.s.txt"), "hidden file");
        assert!(!reads("otra/y.html"), "another folder");
        assert!(!reads("pagina/enlace.html"), "a link out of the folder");
        assert!(!reads("pagina/css/../../otra/y.html"));
        assert!(!reads("pagina/falta.html"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loose_notes_render_from_their_folder() {
        let dir = std::env::temp_dir().join(format!("visor-loose-note-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("notas")).unwrap();
        fs::write(dir.join("notas/tema.md"), "# Tema\n\n- [ ] uno\n").unwrap();
        let root = crate::platform::canonicalize(&dir).unwrap();
        let note = root.join("notas/tema.md");
        let allowed = [root.join("notas")];
        let (path, folder) = vault::loose_file(&note, &allowed).expect("a loose note is readable");
        let mut html = String::new();
        loose_note(&path, &folder, "").into_reader().read_to_string(&mut html).unwrap();
        assert!(html.contains("<h1 "), "{html}");
        let mut raw = String::new();
        loose_note(&path, &folder, "raw").into_reader().read_to_string(&mut raw).unwrap();
        assert_eq!(raw, "# Tema\n\n- [ ] uno\n", "the editor gets the source");
        assert!(vault::loose_file(&root.join("notas/../tema.md"), &allowed).is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
