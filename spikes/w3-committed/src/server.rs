//! Servidor HTTP mínimo con std: dos puertos en 127.0.0.1 (dos orígenes distintos)
//! y un puerto sin nadie escuchando para el caso de error de red.

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
pub struct Ports {
    pub main: u16,
    pub other: u16,
    pub closed: u16,
}

pub fn start() -> std::io::Result<Ports> {
    let main = TcpListener::bind("127.0.0.1:0")?;
    let other = TcpListener::bind("127.0.0.1:0")?;
    // Bound, read and dropped at the end of the statement: nothing listens there afterwards.
    let closed = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let ports = Ports {
        main: main.local_addr()?.port(),
        other: other.local_addr()?.port(),
        closed,
    };
    for listener in [main, other] {
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // One thread per connection: /lenta must not block the other requests.
                thread::spawn(move || handle(stream, ports));
            }
        });
    }
    Ok(ports)
}

fn handle(stream: TcpStream, ports: Ports) {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {}
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let response = route(&path, ports);
    let _ = (&stream).write_all(response.as_bytes());
}

fn route(path: &str, ports: Ports) -> String {
    match path {
        "/a" => html("<p>Página A</p>"),
        "/b" => html("<p>Página B</p>"),
        "/redir" => "HTTP/1.1 302 Found\r\nLocation: /b\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        // pushState runs after the load event, so the document is already committed.
        "/push" => html("<script>addEventListener('load', () => history.pushState({}, '', '/falsa'));</script>"),
        "/frag" => html("<script>addEventListener('load', () => { location.hash = 'seccion'; });</script>"),
        "/iframe" => html(&format!(
            r#"<iframe src="http://127.0.0.1:{}/hijo"></iframe>"#,
            ports.other
        )),
        // The child frame navigates itself to another path on the other origin.
        "/hijo" => html("<script>setTimeout(() => { location.href = '/hijo2'; }, 300);</script>"),
        "/hijo2" => html("<p>Hijo 2</p>"),
        "/script" => html("<script>addEventListener('load', () => { location.href = '/b'; });</script>"),
        // 204 keeps the current document: nothing is committed.
        "/vacio" => "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),
        "/descarga" => "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=prueba.bin\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd".into(),
        "/lenta" => {
            thread::sleep(Duration::from_secs(5));
            html("<p>Lenta</p>")
        }
        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
    }
}

fn html(body: &str) -> String {
    let document = format!("<!doctype html><meta charset=utf-8><title>w3</title>{body}");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{document}",
        document.len()
    )
}
