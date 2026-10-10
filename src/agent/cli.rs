use crate::agent::command::{Command, Target};
use crate::paths;
use crate::vault::{self, config};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::time::{Duration, Instant};

pub fn run(args: &[String]) -> i32 {
    let command = match Command::from_args(args) {
        Ok(command) => command,
        Err(e) => return print(&json!({ "ok": false, "error": e })),
    };
    print(&call(&command))
}

/// Sends one request to the app. `open` starts the app when it is closed.
pub(crate) fn call(command: &Command) -> Value {
    match send(command) {
        Some(r) => r,
        None if matches!(command, Command::Open { .. }) => match start(loose(command)).then(|| send(command)).flatten() {
            Some(r) => r,
            None => json!({ "ok": false, "error": "No se pudo abrir visor." }),
        },
        None => json!({
            "ok": false,
            "error": "visor no está abierto. Ábrelo con `visor` o con `visor open <ruta>`.",
        }),
    }
}

fn send(command: &Command) -> Option<Value> {
    let mut stream = UnixStream::connect(paths::socket()).ok()?;
    writeln!(stream, "{}", command.to_wire()).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

/// Whether `open` asks for an HTML page outside every known vault, which the
/// app shows on its own. Decided here too, so the app starts without the last vault.
fn loose(command: &Command) -> bool {
    let Command::Open { target: Target::Path(page), .. } = command else { return false };
    let Some(path) = page.path.as_ref().and_then(|p| std::fs::canonicalize(p).ok()) else { return false };
    path.is_file() && vault::is_html(&path) && config::Config::load().vault_for(&path).is_none()
}

/// Starts the app detached from this terminal and waits until it answers.
fn start(loose: bool) -> bool {
    let Ok(mut exe) = std::env::current_exe() else { return false };
    // A long-lived `visor mcp` outlives rebuilds and updates; Linux then reports
    // its old binary as "<path> (deleted)", and the new one sits at <path>.
    if let Some(path) = exe.to_str().and_then(|p| p.strip_suffix(" (deleted)")) {
        exe = path.into();
    }
    let mut process = std::process::Command::new(exe);
    if loose {
        process.env("VISOR_LOOSE", "1");
    }
    let spawned = process
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
        if send(&Command::Ping).is_some() {
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
