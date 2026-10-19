//! Servidor HTTP mínimo en 127.0.0.1 con std::net. Sirve los archivos de `web/`.
//! Copiado de spikes/w2-webviews y adaptado para servir archivos del disco.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

pub fn start(raiz: PathBuf) -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let puerto = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let raiz = raiz.clone();
            thread::spawn(move || responder(stream, &raiz));
        }
    });
    Ok(puerto)
}

fn responder(mut stream: TcpStream, raiz: &Path) {
    let mut peticion = Vec::new();
    let mut buf = [0u8; 1024];
    while !peticion.windows(4).any(|w| w == b"\r\n\r\n") && peticion.len() < 8192 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => peticion.extend_from_slice(&buf[..n]),
        }
    }
    let texto = String::from_utf8_lossy(&peticion);
    let ruta_pedida = texto
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let relativa = if ruta_pedida == "/" { "marco.html".to_string() } else { ruta_pedida.trim_start_matches('/').to_string() };
    // Sin `..` ni rutas absolutas: el servidor solo sirve lo que hay bajo `web/`.
    let segura = !relativa.split('/').any(|p| p == ".." || p.is_empty()) && !relativa.contains('\\');
    let archivo = if segura { fs::read(raiz.join(&relativa)).ok() } else { None };
    let (estado, tipo, cuerpo) = match archivo {
        Some(bytes) => ("200 OK", tipo_de(&relativa), bytes),
        None => ("404 Not Found", "text/plain; charset=utf-8", b"no encontrado".to_vec()),
    };
    let cabecera = format!(
        "HTTP/1.1 {estado}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        cuerpo.len()
    );
    let _ = stream.write_all(cabecera.as_bytes());
    let _ = stream.write_all(&cuerpo);
}

fn tipo_de(ruta: &str) -> &'static str {
    if ruta.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if ruta.ends_with(".html") {
        "text/html; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}
