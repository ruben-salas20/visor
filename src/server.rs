use crate::app::{Action, Place, SharedRef, UserEvent};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

const LOAD_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

pub fn spawn(listener: UnixListener, shared: SharedRef, proxy: EventLoopProxy<UserEvent>) {
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (shared, proxy) = (shared.clone(), proxy.clone());
            thread::spawn(move || handle(stream, &shared, &proxy));
        }
    });
}

fn handle(stream: UnixStream, shared: &SharedRef, proxy: &EventLoopProxy<UserEvent>) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(request) => respond(&request, shared, proxy),
        Err(e) => fail(format!("Petición no válida: {e}")),
    };
    let _ = writeln!(&stream, "{response}");
}

fn fail(error: String) -> Value {
    json!({ "ok": false, "error": error })
}

fn respond(request: &Value, shared: &SharedRef, proxy: &EventLoopProxy<UserEvent>) -> Value {
    match request["cmd"].as_str().unwrap_or("") {
        "ping" => json!({ "ok": true }),
        "status" => shared.lock().unwrap().status(),
        "open" => {
            let place = match request["place"].as_str() {
                Some("tab") => Place::NewTab,
                Some("beside") => Place::Beside,
                _ => Place::Here,
            };
            let (reply, answer) = mpsc::channel();
            let event = match request["url"].as_str() {
                Some(url) => UserEvent::OpenUrl { url: url.to_string(), place, reply },
                None => UserEvent::Open {
                    path: PathBuf::from(request["path"].as_str().unwrap_or("")),
                    raw: request["raw"].as_str().unwrap_or("").to_string(),
                    place,
                    reply,
                },
            };
            let _ = proxy.send_event(event);
            match answer.recv_timeout(Duration::from_secs(15)) {
                Ok(Ok(Some((view, before)))) => wait_for_load(shared, view, before),
                Ok(Ok(None)) => shared.lock().unwrap().status(),
                Ok(Err(e)) => fail(e),
                Err(_) => fail("La app no respondió.".into()),
            }
        }
        "reload" => {
            let (view, before, showing) = {
                let s = shared.lock().unwrap();
                let page = s.page();
                (s.focused, page.load_seq, page.file.is_some() || page.url.is_some())
            };
            if !showing {
                return fail("No hay nada abierto.".into());
            }
            let _ = proxy.send_event(UserEvent::Action(Action::Reload));
            wait_for_load(shared, view, before)
        }
        "console" => {
            let mut s = shared.lock().unwrap();
            let focused = s.focused;
            let Some(page) = s.pages.get_mut(&focused) else {
                return json!({ "ok": true, "file": null, "console": [] });
            };
            let entries: Vec<Value> = match request["level"].as_str() {
                Some(level) => page.console.iter().filter(|e| e["level"] == level).cloned().collect(),
                None => page.console.clone(),
            };
            let out = json!({ "ok": true, "file": page.file, "console": entries });
            if request["clear"].as_bool().unwrap_or(false) {
                page.console.clear();
            }
            out
        }
        "events" => events(request, shared),
        "notes" => notes(request, shared),
        "notes_reply" => {
            let Some(vault) = shared.lock().unwrap().vault.clone() else {
                return fail("No hay ninguna bóveda abierta.".into());
            };
            let (id, text) = (request["id"].as_str().unwrap_or(""), request["text"].as_str().unwrap_or(""));
            match crate::annot::reply(vault.path(), id, text) {
                Ok(page) => json!({ "ok": true, "page": page }),
                Err(e) => fail(e),
            }
        }
        other => fail(format!("Comando desconocido: {other}")),
    }
}

/// Waits until `view` finishes loading and reports the problems it logged.
fn wait_for_load(shared: &SharedRef, view: u64, before: u64) -> Value {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    loop {
        {
            let s = shared.lock().unwrap();
            let page = s.page_of(view);
            if page.load_seq > before || Instant::now() >= deadline {
                let mut out = s.status();
                let problems: Vec<Value> = page
                    .console
                    .iter()
                    .filter(|e| e["level"] == "error" || e["level"] == "warn")
                    .cloned()
                    .collect();
                out["problems"] = json!(problems);
                if page.load_seq <= before {
                    out["warning"] = json!("La página no terminó de cargar en 10 s.");
                }
                return out;
            }
        }
        thread::sleep(POLL);
    }
}

/// Annotations of the current page, of a given page, or of the whole vault.
/// For the page on screen it also says which ones could not be placed.
fn notes(request: &Value, shared: &SharedRef) -> Value {
    let (vault, current, orphans) = {
        let s = shared.lock().unwrap();
        (s.vault.clone(), s.page().file.clone(), s.page().orphans.clone())
    };
    let Some(vault) = vault else { return fail("No hay ninguna bóveda abierta.".into()) };
    let page = if request["all"].as_bool().unwrap_or(false) {
        None
    } else if let Some(path) = request["path"].as_str() {
        let path = PathBuf::from(path);
        let found = path.canonicalize().ok().filter(|p| p.starts_with(vault.root()));
        let raw = request["raw"].as_str().unwrap_or("");
        Some(found.map_or(raw.trim_start_matches("./").to_string(), |p| vault.rel(&p)))
    } else {
        match &current {
            Some(file) if crate::vault::is_loose(file) => {
                return fail("Las páginas abiertas fuera de una bóveda no tienen anotaciones.".into());
            }
            Some(file) => Some(file.clone()),
            None => return fail("No hay ninguna página abierta; indica una ruta o usa --all.".into()),
        }
    };
    let mut list = crate::annot::list(vault.path(), page.as_deref());
    for note in &mut list {
        if note["page"].as_str() == current.as_deref()
            && let Some(orphans) = &orphans
        {
            note["anchored"] = json!(!orphans.iter().any(|o| note["id"] == *o.as_str()));
        }
    }
    json!({ "ok": true, "vault": vault.path(), "notes": list })
}

fn events(request: &Value, shared: &SharedRef) -> Value {
    let wait = request["wait"].as_f64().map(Duration::from_secs_f64);
    let limit = request["limit"].as_u64().unwrap_or(20) as usize;
    let after = request["after"]
        .as_u64()
        .or_else(|| wait.map(|_| shared.lock().unwrap().next_id - 1));
    let deadline = Instant::now() + wait.unwrap_or_default();
    loop {
        {
            let s = shared.lock().unwrap();
            let found: Vec<Value> = match after {
                Some(after) => s.events.iter().filter(|e| e["id"].as_u64() > Some(after)).cloned().collect(),
                None => s.events[s.events.len().saturating_sub(limit)..].to_vec(),
            };
            if !found.is_empty() || Instant::now() >= deadline {
                return json!({ "ok": true, "events": found, "last_event_id": s.next_id - 1 });
            }
        }
        thread::sleep(POLL * 4);
    }
}
