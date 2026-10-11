use crate::session::{LOADED, Place, AgentRequest, SharedRef};
use crate::agent::command::{Command, Level, NotesScope, OpenPlace, Page, Target};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const LOAD_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

/// `send` hands a request to the window.
pub fn spawn(listener: UnixListener, shared: SharedRef, send: impl Fn(AgentRequest) + Send + Sync + 'static) {
    let send: Arc<dyn Fn(AgentRequest) + Send + Sync> = Arc::new(send);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (shared, send) = (shared.clone(), send.clone());
            thread::spawn(move || handle(stream, &shared, &*send));
        }
    });
}

fn handle(stream: UnixStream, shared: &SharedRef, send: &dyn Fn(AgentRequest)) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(request) => match Command::from_wire(&request) {
            Ok(command) => respond(command, shared, send),
            Err(e) => fail(e),
        },
        Err(e) => fail(format!("Petición no válida: {e}")),
    };
    let _ = writeln!(&stream, "{response}");
}

fn fail(error: String) -> Value {
    json!({ "ok": false, "error": error })
}

fn respond(command: Command, shared: &SharedRef, send: &dyn Fn(AgentRequest)) -> Value {
    match command {
        Command::Ping => json!({ "ok": true }),
        Command::Status => shared.lock().unwrap().status(),
        Command::Open { target, place } => {
            let place = match place {
                Some(OpenPlace::Tab) => Place::NewTab,
                Some(OpenPlace::Beside) => Place::Beside,
                Some(OpenPlace::Here) | None => Place::Here,
            };
            let (reply, answer) = mpsc::channel();
            let event = match target {
                Target::Url { url } => AgentRequest::OpenUrl { url, place, reply },
                Target::Path(page) => AgentRequest::Open { path: page.path.unwrap_or_default(), raw: page.raw, place, reply },
            };
            let start = Instant::now();
            send(event);
            match answer.recv_timeout(Duration::from_secs(15)) {
                Ok(Ok(Some((view, before)))) => {
                    let mut out = wait_for_load(shared, view, before);
                    // For tools/medir: the load as the app sees it, without the CLI's own start.
                    if std::env::var_os("VISOR_TIEMPOS").is_some() {
                        out["ms"] = json!((start.elapsed().as_secs_f64() * 10000.0).round() / 10.0);
                    }
                    out
                }
                Ok(Ok(None)) => shared.lock().unwrap().status(),
                Ok(Err(e)) => fail(e),
                Err(_) => fail("La app no respondió.".into()),
            }
        }
        Command::Close { target } => {
            let (reply, answer) = mpsc::channel();
            let target = target.map(|page| (page.path.unwrap_or_default(), page.raw));
            send(AgentRequest::Close { target, reply });
            match answer.recv_timeout(Duration::from_secs(15)) {
                Ok(Ok(closed)) => {
                    let mut out = shared.lock().unwrap().status();
                    out["closed"] = json!(closed);
                    out
                }
                Ok(Err(e)) => fail(e),
                Err(_) => fail("La app no respondió.".into()),
            }
        }
        Command::Reload => {
            let (view, before, showing) = {
                let s = shared.lock().unwrap();
                let page = s.page();
                (s.focused, page.load_seq, page.file.is_some() || page.url.is_some())
            };
            if !showing {
                return fail("No hay nada abierto.".into());
            }
            send(AgentRequest::Reload);
            wait_for_load(shared, view, before)
        }
        Command::Console { level, clear } => {
            let mut s = shared.lock().unwrap();
            let focused = s.focused;
            let Some(page) = s.pages.get_mut(&focused) else {
                return json!({ "ok": true, "file": null, "console": [] });
            };
            let entries: Vec<Value> = match level {
                Some(Level::Error) => page.console.iter().filter(|e| e["level"] == "error").cloned().collect(),
                None => page.console.clone(),
            };
            let out = json!({ "ok": true, "file": page.file, "console": entries });
            if clear {
                page.console.clear();
            }
            out
        }
        Command::Screenshot { target } => screenshot(target, send),
        Command::Eval { code } => run_script(code, send),
        Command::Events { after, wait, limit } => events(after, wait, limit, shared),
        Command::Notes { scope } => notes(&scope, shared),
        Command::NotesReply { id, text } => {
            let Some(vault) = shared.lock().unwrap().vault.clone() else {
                return fail("No hay ninguna bóveda abierta.".into());
            };
            match crate::vault::annot::reply(vault.path(), &id, &text) {
                Ok(page) => json!({ "ok": true, "page": page }),
                Err(e) => fail(e),
            }
        }
        // Installs only; the window keeps running the version it has (see ui/app.rs `restart`).
        Command::Update => match crate::update::run(true) {
            outcome @ (crate::update::Outcome::Latest(_) | crate::update::Outcome::Installed(_)) => {
                json!({ "ok": true, "message": outcome.message() })
            }
            outcome => fail(outcome.message()),
        },
        // `visor mcp` answers the guide itself.
        Command::Guide => fail("Comando desconocido: guide".into()),
    }
}

/// Saves the focused page as a PNG. The window writes the file once WebKit has
/// drawn it; the path is checked here, since it is used exactly as given.
fn screenshot(target: Page, send: &dyn Fn(AgentRequest)) -> Value {
    let Some(path) = target.path else {
        return fail("Indica una ruta absoluta para la captura.".into());
    };
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return fail("La captura debe guardarse en un archivo .png.".into());
    }
    let (reply, answer) = mpsc::channel();
    send(AgentRequest::Screenshot { path: path.clone(), reply });
    match answer.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(bytes)) => json!({ "ok": true, "path": path.to_string_lossy(), "bytes": bytes }),
        Ok(Err(e)) => fail(e),
        Err(_) => fail("La app no respondió.".into()),
    }
}

/// Runs JS in the focused page. The answer waits for the script, so a script
/// that never ends freezes the window, as it would freeze the page.
fn run_script(code: String, send: &dyn Fn(AgentRequest)) -> Value {
    let (reply, answer) = mpsc::channel();
    send(AgentRequest::Eval { code, reply });
    match answer.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => fail(e),
        Err(_) => fail("La app no respondió.".into()),
    }
}

/// Waits until `view` finishes loading and reports the problems it logged.
fn wait_for_load(shared: &SharedRef, view: u64, before: u64) -> Value {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    let mut s = shared.lock().unwrap();
    loop {
        let loaded = s.page_of(view).load_seq > before;
        let left = deadline.saturating_duration_since(Instant::now());
        if loaded || left.is_zero() {
            let page = s.page_of(view);
            let mut out = s.status();
            let problems: Vec<Value> = page
                .console
                .iter()
                .filter(|e| e["level"] == "error" || e["level"] == "warn")
                .cloned()
                .collect();
            out["problems"] = json!(problems);
            if !loaded {
                out["warning"] = json!("La página no terminó de cargar en 10 s.");
            }
            return out;
        }
        s = LOADED.wait_timeout(s, left).unwrap().0;
    }
}

/// Annotations of the current page, of a given page, or of the whole vault.
/// For the page on screen it also says which ones could not be placed.
fn notes(scope: &NotesScope, shared: &SharedRef) -> Value {
    let (vault, current, orphans) = {
        let s = shared.lock().unwrap();
        (s.vault.clone(), s.page().file.clone(), s.page().orphans.clone())
    };
    let Some(vault) = vault else { return fail("No hay ninguna bóveda abierta.".into()) };
    let page = match scope {
        NotesScope::All => None,
        NotesScope::Page(page) => {
            let found = page.path.as_ref().and_then(|p| p.canonicalize().ok()).filter(|p| p.starts_with(vault.root()));
            Some(found.map_or(page.raw.trim_start_matches("./").to_string(), |p| vault.rel(&p)))
        }
        NotesScope::Current => match &current {
            Some(file) if crate::vault::is_loose(file) => {
                return fail("Las páginas abiertas fuera de una bóveda no tienen anotaciones.".into());
            }
            Some(file) => Some(file.clone()),
            None => return fail("No hay ninguna página abierta; indica una ruta o usa --all.".into()),
        },
    };
    let mut list = crate::vault::annot::list(vault.path(), page.as_deref());
    for note in &mut list {
        if note["page"].as_str() == current.as_deref()
            && let Some(orphans) = &orphans
        {
            note["anchored"] = json!(!orphans.iter().any(|o| note["id"] == *o.as_str()));
        }
    }
    json!({ "ok": true, "vault": vault.path().to_string_lossy(), "notes": list })
}

fn events(after: Option<u64>, wait: Option<f64>, limit: Option<u64>, shared: &SharedRef) -> Value {
    // An older client may send what Duration refuses (negative, NaN); wait nothing then.
    let wait = wait.map(|w| Duration::try_from_secs_f64(w).unwrap_or_default());
    let limit = limit.unwrap_or(20) as usize;
    let after = after.or_else(|| wait.map(|_| shared.lock().unwrap().next_id - 1));
    let deadline = Instant::now() + wait.unwrap_or_default();
    loop {
        {
            let s = shared.lock().unwrap();
            let found: Vec<Value> = match after {
                Some(after) => s.events.iter().filter(|e| e["id"].as_u64() > Some(after)).cloned().collect(),
                None => s.events.iter().skip(s.events.len().saturating_sub(limit)).cloned().collect(),
            };
            if !found.is_empty() || Instant::now() >= deadline {
                return json!({ "ok": true, "events": found, "last_event_id": s.next_id - 1 });
            }
        }
        thread::sleep(POLL * 4);
    }
}
