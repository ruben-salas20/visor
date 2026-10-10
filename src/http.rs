//! Serves the open vault, and pages opened on their own, over plain HTTP on
//! 127.0.0.1. WebKitGTK only plays video and offers Media Source to network
//! origins, not to custom schemes.

use crate::session::SharedRef;
use crate::vault;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, ListenAddr, Request, Response, Server, StatusCode};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Starts the server and returns its port and the base URL of vault pages.
/// The preferred port is reused so pages keep their origin, and with it their
/// localStorage, between sessions.
pub fn start(preferred_port: Option<u16>, shared: SharedRef) -> Option<(u16, String)> {
    let server = preferred_port
        .and_then(|port| Server::http(("127.0.0.1", port)).ok())
        .or_else(|| Server::http("127.0.0.1:0").ok())?;
    let ListenAddr::IP(addr) = server.server_addr() else { return None };
    let port = addr.port();
    // Other programs on this machine can reach the port; without the secret
    // path they cannot read the vault.
    let prefix = format!("/{}/", secret()?);
    let base = format!("http://127.0.0.1:{port}{prefix}");
    let host = format!("127.0.0.1:{port}");
    let server = Arc::new(server);
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let (shared, prefix, host) = (shared.clone(), prefix.clone(), host.clone());
            thread::spawn(move || handle(request, &shared, &prefix, &host));
        }
    });
    Some((port, base))
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

fn secret() -> Option<String> {
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

fn handle(request: Request, shared: &SharedRef, prefix: &str, host: &str) {
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
    if let Some(name) = rel.strip_prefix(crate::vault::markdown::ASSETS) {
        serve_asset(request, name);
        return;
    }
    let rel = percent_encoding::percent_decode_str(rel).decode_utf8_lossy().into_owned();
    if vault::is_loose(&rel) {
        // Only from the folders of pages opened on their own, as plain files.
        let allowed = shared.lock().unwrap().loose.clone();
        let path = Path::new(&rel).canonicalize().ok().filter(|p| p.is_file() && allowed.iter().any(|d| p.starts_with(d)));
        match path {
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
