//! Serves the open vault on 127.0.0.1, and pages opened on their own on
//! 127.0.0.2, over plain HTTP. WebKitGTK only plays video and offers Media
//! Source to network origins, not to custom schemes.

use crate::session::SharedRef;
use crate::vault;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, ListenAddr, Request, Response, Server, StatusCode};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Where the server listens, as the app needs it.
pub struct Started {
    pub port: u16,
    /// URL prefix of vault pages.
    pub base: String,
    /// URL prefix of loose pages, on an origin of their own.
    pub loose_base: String,
}

/// The two origins the server answers for. Loose pages get their own, on
/// another loopback address, so they can neither read the vault nor make
/// their address look like a vault page's.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Realm {
    Vault,
    Loose,
}

/// Starts the server. The preferred port is reused so pages keep their
/// origin, and with it their localStorage, between sessions.
pub fn start(preferred_port: Option<u16>, shared: SharedRef) -> Option<Started> {
    let vault = listen("127.0.0.1", preferred_port)?;
    let port = port_of(&vault)?;
    let loose = listen("127.0.0.2", Some(port))?;
    let loose_port = port_of(&loose)?;
    // Other programs on this machine can reach the port; without the secret
    // path they cannot read the vault.
    let prefix = format!("/{}/", secret()?);
    let started = Started {
        port,
        base: format!("http://127.0.0.1:{port}{prefix}"),
        loose_base: format!("http://127.0.0.2:{loose_port}{prefix}"),
    };
    serve(vault, Realm::Vault, format!("127.0.0.1:{port}"), prefix.clone(), shared.clone());
    serve(loose, Realm::Loose, format!("127.0.0.2:{loose_port}"), prefix, shared);
    Some(started)
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
            let (shared, prefix, host) = (shared.clone(), prefix.clone(), host.clone());
            thread::spawn(move || handle(request, realm, &shared, &prefix, &host));
        }
    });
}

fn serve_asset(request: Request, name: &str) {
    let Some((_, bytes, brotli)) = ASSETS.iter().find(|(n, _, _)| *n == name) else {
        let _ = request.respond(text(404, "No encontrado"));
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
    File::open("/dev/urandom").ok()?.read_exact(&mut bytes).ok()?;
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

fn handle(request: Request, realm: Realm, shared: &SharedRef, prefix: &str, host: &str) {
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
        let _ = request.respond(text(403, "Prohibido"));
        return;
    }
    let url = request.url().split('#').next().unwrap_or("").to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let Some(rel) = path.strip_prefix(prefix) else {
        let _ = request.respond(text(404, "No encontrado"));
        return;
    };
    if realm == Realm::Vault
        && let Some(name) = rel.strip_prefix(crate::vault::markdown::ASSETS)
    {
        serve_asset(request, name);
        return;
    }
    let rel = percent_encoding::percent_decode_str(rel).decode_utf8_lossy().into_owned();
    // Each origin serves only its own kind of page.
    if vault::is_loose(&rel) != (realm == Realm::Loose) {
        let _ = request.respond(text(404, &format!("No existe: {rel}")));
        return;
    }
    if realm == Realm::Loose {
        let allowed = shared.lock().unwrap().loose.clone();
        match loose_file(Path::new(&rel), &allowed) {
            Some(path) => {
                let range = value("Range");
                serve_file(request, &path, range);
            }
            None => {
                let _ = request.respond(text(404, &format!("No existe: {rel}")));
            }
        }
        return;
    }
    let Some(vault) = shared.lock().unwrap().vault.clone() else {
        let _ = request.respond(text(404, "No hay ninguna bóveda abierta"));
        return;
    };
    let Some(path) = vault.resolve(&rel) else {
        let _ = request.respond(text(404, &format!("No existe: {rel}")));
        return;
    };
    // Notes are rendered unless the editor asks for the source with `?raw`,
    // or where one of its `[[links]]` leads with `?wiki=target`.
    if vault::is_markdown(&path) {
        // `![[image]]` in the editor: the image file, wherever it is in the vault.
        if let Some(name) = query.split('&').find_map(|q| q.strip_prefix("embed=")) {
            let name = percent_encoding::percent_decode_str(name).decode_utf8_lossy();
            let response = match crate::vault::markdown::embed_url(&name, &vault.rel(&path), vault.root()) {
                Some(url) => text(302, "").with_header(header("Location", &url)),
                None => text(404, "No existe"),
            };
            let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
            return;
        }
        if let Some(target) = query.split('&').find_map(|q| q.strip_prefix("wiki=")) {
            let target = percent_encoding::percent_decode_str(target).decode_utf8_lossy();
            let response = match crate::vault::markdown::wiki_url(&target, &vault.rel(&path), vault.root()) {
                Some(url) => text(200, &url),
                None => text(404, "No existe"),
            };
            let _ = request.respond(response.with_header(header("Cache-Control", "no-store")));
            return;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            let _ = request.respond(text(500, "No se pudo leer la nota"));
            return;
        };
        let (body, mime) = if query.split('&').any(|q| q == "raw") {
            (source, "text/plain; charset=utf-8")
        } else {
            (crate::vault::markdown::page(&source, &vault.rel(&path), vault.root()), "text/html; charset=utf-8")
        };
        let response = Response::from_string(body)
            .with_header(header("Content-Type", mime))
            .with_header(header("Cache-Control", "no-store"));
        let _ = request.respond(response);
        return;
    }
    let range = value("Range");
    serve_file(request, &path, range);
}

/// A file a loose page may read: inside the folder of a loose tab that is
/// open, subfolders included, and with nothing hidden on the way.
fn loose_file(path: &Path, allowed: &[PathBuf]) -> Option<PathBuf> {
    // A loose path is absolute, so the lookup starts at `/`.
    let real = |p: &Path| p.canonicalize().ok().or_else(|| vault::find_nfc(Path::new("/"), p)?.canonicalize().ok());
    let path = real(path).filter(|p| p.is_file())?;
    let visible = |rel: &Path| !rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
    allowed.iter().any(|dir| path.strip_prefix(dir).is_ok_and(visible)).then_some(path)
}

fn serve_file(request: Request, path: &Path, range: Option<String>) {
    let Ok(mut file) = File::open(path) else {
        let _ = request.respond(text(500, "No se pudo leer el archivo"));
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
        let _ = request.respond(text(500, "No se pudo leer el archivo"));
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
        let root = dir.canonicalize().unwrap();
        let allowed = [root.join("Pra\u{301}ctica")];
        let nfc = format!("{}/Práctica/Ejercicios de Práctica.pdf", root.display());
        let found = loose_file(Path::new(&nfc), &allowed).expect("NFC spelling");
        assert_eq!(found, root.join("Pra\u{301}ctica/Ejercicios de Pra\u{301}ctica.pdf"));
        let nfd_name = format!("{}/Pra\u{301}ctica/Ejercicios de Pra\u{301}ctica.pdf", root.display());
        assert!(loose_file(Path::new(&nfd_name), &allowed).is_some(), "NFD spelling");
        assert!(loose_file(Path::new(&format!("{}/Práctica/../Otra/x.pdf", root.display())), &allowed).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

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
        let root = dir.canonicalize().unwrap();
        std::os::unix::fs::symlink(root.join("otra/y.html"), root.join("pagina/enlace.html")).unwrap();
        let allowed = [root.join("pagina")];
        let reads = |rel: &str| loose_file(&root.join(rel), &allowed).is_some();
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
}
