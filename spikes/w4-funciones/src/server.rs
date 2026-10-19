//! Servidor HTTP mínimo en 127.0.0.1 con std::net. Sirve las páginas de prueba, que van
//! dentro del binario. `{PUERTO}` y `{CERRADO}` se sustituyen por el puerto del servidor y
//! por un puerto en el que nadie escucha. `localhost:{PUERTO}` es el mismo servidor visto
//! como otro sitio, para tener un iframe de otro origen.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub struct Puertos {
    pub servidor: u16,
    pub cerrado: u16,
}

pub fn start() -> std::io::Result<Puertos> {
    // A port that was free a moment ago and has no listener: loads to it fail at the network level.
    let cerrado = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let servidor = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || responder(stream, servidor, cerrado));
        }
    });
    Ok(Puertos { servidor, cerrado })
}

fn responder(mut stream: TcpStream, puerto: u16, cerrado: u16) {
    let mut peticion = Vec::new();
    let mut buf = [0u8; 1024];
    while !peticion.windows(4).any(|w| w == b"\r\n\r\n") && peticion.len() < 8192 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => peticion.extend_from_slice(&buf[..n]),
        }
    }
    let texto = String::from_utf8_lossy(&peticion);
    let ruta = texto
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let (estado, cuerpo) = match pagina(&ruta) {
        Some(html) => ("200 OK", html.replace("{PUERTO}", &puerto.to_string()).replace("{CERRADO}", &cerrado.to_string())),
        None => ("404 Not Found", "no encontrado".to_string()),
    };
    let tipo = if estado.starts_with("200") { "text/html; charset=utf-8" } else { "text/plain; charset=utf-8" };
    let cabecera = format!(
        "HTTP/1.1 {estado}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        cuerpo.len()
    );
    let _ = stream.write_all(cabecera.as_bytes());
    let _ = stream.write_all(cuerpo.as_bytes());
}

const BASE: &str = "<!doctype html><meta charset=utf-8><style>html,body{margin:0;height:100%}</style>";

fn pagina(ruta: &str) -> Option<String> {
    let cuerpo = match ruta {
        // 1. Two halves of known colours for the pixel check.
        "/color.html" => r#"<title>color</title>
<div style="position:fixed;left:0;top:0;width:50%;height:100%;background:rgb(10,200,90)"></div>
<div style="position:fixed;right:0;top:0;width:50%;height:100%;background:rgb(200,30,60)"></div>"#,
        // 2. An element to put in fullscreen.
        "/pantalla.html" => r#"<title>pantalla</title>
<div id=caja style="width:200px;height:120px;background:rgb(30,60,200)">caja</div>"#,
        // 3. Three failed subresources: a 404 image, a script on a closed port, and a 404 image
        // inside a same-origin iframe. Plus a cross-site iframe (localhost) with its own 404.
        "/fallos.html" => r#"<title>fallos</title>
<img src="/no-existe.png" alt="">
<script src="http://127.0.0.1:{CERRADO}/red.js"></script>
<iframe src="/fallos-hijo.html"></iframe>
<iframe src="http://localhost:{PUERTO}/fallos-externo.html"></iframe>"#,
        "/fallos-hijo.html" => r#"<title>hijo</title><img src="/hijo-no-existe.png" alt="">"#,
        "/fallos-externo.html" => r#"<title>externo</title><img src="/externo-no-existe.png" alt="">"#,
        // 4. Page rules against the injected theme: same colour rgb(1, 2, 3), with and without !important.
        "/estilo.html" => r#"<title>estilo</title>
<style>p.normal{color:rgb(1,2,3)} p.importante{color:rgb(1,2,3)!important}</style>
<p class=normal>normal</p><p class=importante>importante</p>
<iframe id=mismo src="/estilo-hijo.html"></iframe>
<iframe id=otro src="http://localhost:{PUERTO}/estilo-hijo.html"></iframe>"#,
        "/estilo-hijo.html" => r#"<title>estilo hijo</title>
<style>p.normal{color:rgb(1,2,3)} p.importante{color:rgb(1,2,3)!important}</style>
<p class=normal>normal</p><p class=importante>importante</p>"#,
        // 5. Zoom: two pages of the same origin.
        "/zoom-1.html" | "/zoom-2.html" => r#"<title>zoom</title><p>zoom</p>"#,
        // 6. Focus: one field per view.
        "/foco.html" => r#"<title>foco</title><input id=campo autofocus>"#,
        // 7. Side buttons. The page logs mouse events in sessionStorage (survives navigation in
        // the same origin) and, with ?bloquear=1, cancels buttons 3 and 4.
        "/lateral-1.html" | "/lateral-2.html" => r#"<title>lateral</title>
<div style="position:fixed;inset:0;background:rgb(240,240,200)">lateral</div>
<script>
const bloquear = location.search.includes('bloquear=1');
for (const t of ['mousedown', 'mouseup', 'pointerdown', 'pointerup', 'auxclick']) {
  addEventListener(t, e => {
    const lista = JSON.parse(sessionStorage.getItem('ev') || '[]');
    lista.push(location.pathname + location.search + ' ' + t + ' ' + e.button);
    sessionStorage.setItem('ev', JSON.stringify(lista));
    if (bloquear && (e.button === 3 || e.button === 4)) e.preventDefault();
  }, true);
}
</script>"#,
        _ => return None,
    };
    Some(format!("{BASE}{cuerpo}"))
}
