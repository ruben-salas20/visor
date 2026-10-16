//! Servidor HTTP mínimo en 127.0.0.1 con std::net. Sirve la misma página a todos los webviews;
//! el número del webview va en la consulta (`?n=`) y la página lo devuelve por IPC al cargar.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub fn start() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || answer(stream));
        }
    });
    Ok(port)
}

fn answer(mut stream: TcpStream) {
    // Leemos hasta la cabecera completa; la petición es corta, pero no asumimos una sola lectura.
    let mut request = Vec::new();
    let mut buf = [0u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 8192 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    let body = PAGE;
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

const PAGE: &str = r#"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>W2 webview</title>
<style>
  body { font-family: sans-serif; margin: 0; padding: 12px; background: #f4f1ea; color: #222; }
  h1 { font-size: 18px; margin: 0 0 8px; }
  p { font-size: 13px; line-height: 1.4; margin: 6px 0; }
  .bloque { border: 1px solid #bbb; border-radius: 6px; padding: 8px; margin: 8px 0; background: #fff; }
  code { background: #eee; padding: 0 3px; }
</style>
</head>
<body>
<h1>Webview de prueba <span id="n"></span></h1>
<div class="bloque">
  <p>Texto de prueba para medir el coste de un webview hijo de WebView2 en una ventana compartida.</p>
  <p>Este párrafo ocupa algo de altura para que la rejilla de webviews tenga contenido real que pintar.</p>
</div>
<div class="bloque">
  <p>Se usa un entorno de WebView2 común y un solo servidor local. El código <code>n</code> identifica el webview.</p>
</div>
<script>
  const n = new URLSearchParams(location.search).get('n');
  document.getElementById('n').textContent = n;
  window.addEventListener('load', () => window.ipc.postMessage('loaded ' + n));
</script>
</body>
</html>
"#;
