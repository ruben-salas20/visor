use crate::paths;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn run(args: &[String]) -> i32 {
    let request = match parse(args) {
        Ok(r) => r,
        Err(e) => return print(&json!({ "ok": false, "error": e })),
    };
    let response = match send(&request) {
        Some(r) => r,
        None if request["cmd"] == "open" => match start().then(|| send(&request)).flatten() {
            Some(r) => r,
            None => json!({ "ok": false, "error": "No se pudo abrir visor." }),
        },
        None => json!({
            "ok": false,
            "error": "visor no está abierto. Ábrelo con `visor` o con `visor open <ruta>`.",
        }),
    };
    print(&response)
}

fn parse(args: &[String]) -> Result<Value, String> {
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let flag = |name: &str| rest.iter().any(|a| a == name);
    let value = |name: &str| -> Result<Option<f64>, String> {
        match rest.iter().position(|a| a == name) {
            None => Ok(None),
            Some(i) => rest
                .get(i + 1)
                .and_then(|v| v.parse().ok())
                .map(Some)
                .ok_or_else(|| format!("{name} necesita un número.")),
        }
    };
    match cmd {
        "open" => {
            let usage = "Uso: visor open <carpeta, archivo o URL> [--tab | --beside]";
            let raw = rest.iter().find(|a| !a.starts_with("--")).ok_or(usage)?;
            if let Some(unknown) = rest.iter().find(|a| a.starts_with("--") && *a != "--tab" && *a != "--beside") {
                return Err(format!("Opción desconocida: {unknown}. {usage}"));
            }
            let place = if flag("--beside") { Some("beside") } else if flag("--tab") { Some("tab") } else { None };
            if raw.starts_with("http://") || raw.starts_with("https://") {
                return Ok(json!({ "cmd": "open", "url": raw, "place": place }));
            }
            let path = std::env::current_dir().map_err(|e| e.to_string())?.join(raw);
            Ok(json!({ "cmd": "open", "path": path, "raw": raw, "place": place }))
        }
        "status" | "reload" => Ok(json!({ "cmd": cmd })),
        "console" => Ok(json!({
            "cmd": "console",
            "level": flag("--errors").then_some("error"),
            "clear": flag("--clear"),
        })),
        "events" => Ok(json!({
            "cmd": "events",
            "after": value("--after")?.map(|v| v as u64),
            "wait": value("--wait")?,
            "limit": value("--limit")?.map(|v| v as u64),
        })),
        "notes" => match rest.first().map(String::as_str) {
            Some("reply") => {
                let id = rest.get(1).ok_or("Uso: visor notes reply <id> <texto>")?;
                let text = rest[2..].join(" ");
                if text.is_empty() {
                    return Err("Uso: visor notes reply <id> <texto>".into());
                }
                Ok(json!({ "cmd": "notes_reply", "id": id, "text": text }))
            }
            Some("--all") => Ok(json!({ "cmd": "notes", "all": true })),
            Some(raw) => {
                let path = std::env::current_dir().map_err(|e| e.to_string())?.join(raw);
                Ok(json!({ "cmd": "notes", "path": path, "raw": raw }))
            }
            None => Ok(json!({ "cmd": "notes" })),
        },
        other => Err(format!("Comando desconocido: {other}. Usa `visor help`.")),
    }
}

fn send(request: &Value) -> Option<Value> {
    let mut stream = UnixStream::connect(paths::socket()).ok()?;
    writeln!(stream, "{request}").ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

/// Starts the app detached from this terminal and waits until it answers.
fn start() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let spawned = Command::new(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    if spawned.is_err() {
        return false;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if send(&json!({ "cmd": "ping" })).is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn print(response: &Value) -> i32 {
    println!("{}", serde_json::to_string_pretty(response).unwrap());
    if response["ok"] == true { 0 } else { 1 }
}
