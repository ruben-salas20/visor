use crate::i18n::tr;
use crate::session::{LOADED, Place, AgentRequest, SharedRef};
use crate::trf;
use crate::agent::command::{Command, Level, NotesScope, OpenPlace, Page, Request, Target};
use crate::ui::windows::WinId;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const LOAD_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

/// `send` hands a request to the window it is for.
pub fn spawn(listener: UnixListener, shared: SharedRef, send: impl Fn(WinId, AgentRequest) + Send + Sync + 'static) {
    let send: Arc<dyn Fn(WinId, AgentRequest) + Send + Sync> = Arc::new(send);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (shared, send) = (shared.clone(), send.clone());
            thread::spawn(move || handle(stream, &shared, &*send));
        }
    });
}

fn handle(stream: UnixStream, shared: &SharedRef, send: &dyn Fn(WinId, AgentRequest)) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let response = match serde_json::from_str::<Value>(&line) {
        Ok(request) => match Request::from_wire(&request) {
            Ok(request) => respond(request, shared, send),
            Err(e) => fail(e),
        },
        Err(e) => fail(trf!("Petición no válida: {e}", "Invalid request: {e}")),
    };
    let _ = writeln!(&stream, "{response}");
}

fn fail(error: String) -> Value {
    json!({ "ok": false, "error": error })
}

/// Answers a request. Only the commands that act on a window get one resolved:
/// the one asked for, or the focused one.
fn respond(request: Request, shared: &SharedRef, send: &dyn Fn(WinId, AgentRequest)) -> Value {
    let Request { command, window } = request;
    match command {
        Command::Ping => json!({ "ok": true }),
        Command::Events { after, wait, limit } => events(after, wait, limit, shared),
        // Installs only; the window keeps running the version it has (see ui/app.rs `restart`).
        Command::Update => match crate::update::run(true) {
            outcome @ (crate::update::Outcome::Latest(_) | crate::update::Outcome::Installed(_)) => {
                json!({ "ok": true, "message": outcome.message() })
            }
            outcome => fail(outcome.message()),
        },
        // `visor mcp` answers the guide itself.
        Command::Guide => fail(tr("Comando desconocido: guide", "Unknown command: guide").into()),
        Command::NewWindow { token } => {
            let (reply, answer) = mpsc::channel();
            // Window 0 is the process, which opens windows.
            send(0, AgentRequest::NewWindow { token, reply });
            match answer.recv_timeout(Duration::from_secs(15)) {
                Ok(Ok(window)) => json!({ "ok": true, "window": window }),
                Ok(Err(e)) => fail(e),
                Err(_) => fail(tr("La app no respondió.", "The app did not respond.").into()),
            }
        }
        command => {
            // `open <file>` and `close <file>` go to a window that shows the file's vault, if one does.
            let file = match &command {
                Command::Open { target: Target::Path(page), .. } | Command::Close { target: Some(page) } => page.path.clone(),
                _ => None,
            };
            let target = target_once_open(shared, window, file.as_deref());
            match target {
                Ok(win) => respond_in(command, win, shared, send),
                Err(e) => fail(e),
            }
        }
    }
}

fn respond_in(command: Command, win: WinId, shared: &SharedRef, send: &dyn Fn(WinId, AgentRequest)) -> Value {
    match command {
        Command::Status => shared.lock().unwrap().status(win),
        Command::Open { target, place, loose } => {
            let place = match place {
                Some(OpenPlace::Tab) => Place::NewTab,
                Some(OpenPlace::Beside) => Place::Beside,
                Some(OpenPlace::Here) | None => Place::Here,
            };
            let (reply, answer) = mpsc::channel();
            let event = match target {
                Target::Url { url } => AgentRequest::OpenUrl { url, place, reply },
                Target::Path(page) => AgentRequest::Open { path: page.path.unwrap_or_default(), raw: page.raw, place, loose, reply },
            };
            let start = Instant::now();
            send(win, event);
            match answer.recv_timeout(Duration::from_secs(15)) {
                Ok(Ok(Some((view, before)))) => {
                    let mut out = wait_for_load(shared, win, view, before);
                    // For tools/medir: the load as the app sees it, without the CLI's own start.
                    if std::env::var_os("VISOR_TIEMPOS").is_some() {
                        out["ms"] = json!((start.elapsed().as_secs_f64() * 10000.0).round() / 10.0);
                    }
                    out
                }
                Ok(Ok(None)) => shared.lock().unwrap().status(win),
                Ok(Err(e)) => fail(e),
                Err(_) => fail(tr("La app no respondió.", "The app did not respond.").into()),
            }
        }
        Command::Close { target } => {
            let (reply, answer) = mpsc::channel();
            let target = target.map(|page| (page.path.unwrap_or_default(), page.raw));
            send(win, AgentRequest::Close { target, reply });
            match answer.recv_timeout(Duration::from_secs(15)) {
                Ok(Ok(closed)) => {
                    let mut out = shared.lock().unwrap().status(win);
                    out["closed"] = json!(closed);
                    out
                }
                Ok(Err(e)) => fail(e),
                Err(_) => fail(tr("La app no respondió.", "The app did not respond.").into()),
            }
        }
        Command::Reload => {
            let (view, before, showing) = {
                let s = shared.lock().unwrap();
                let view = s.window(win).map_or(0, |w| w.focused);
                let page = s.page_of(win, view);
                (view, page.load_seq, page.file.is_some() || page.url.is_some())
            };
            if !showing {
                return fail(tr("No hay nada abierto.", "Nothing is open.").into());
            }
            send(win, AgentRequest::Reload);
            wait_for_load(shared, win, view, before)
        }
        Command::Console { level, clear } => {
            let mut s = shared.lock().unwrap();
            let focused = s.window(win).map_or(0, |w| w.focused);
            let Some(page) = s.windows.get_mut(&win).and_then(|w| w.pages.get_mut(&focused)) else {
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
        Command::Screenshot { target } => screenshot(target, win, send),
        Command::Eval { code } => run_script(code, win, send),
        Command::Notes { scope } => notes(&scope, win, shared),
        Command::NotesReply { id, text } => notes_reply(&id, &text, win, shared),
        Command::Ping | Command::Events { .. } | Command::Update | Command::Guide | Command::NewWindow { .. } => {
            unreachable!("answered in `respond`, which needs no window")
        }
    }
}

/// Answers an annotation reply. The window's own vault comes first; the other
/// windows' vaults are tried after it, so an id from another window still works.
fn notes_reply(id: &str, text: &str, win: WinId, shared: &SharedRef) -> Value {
    let vaults: Vec<Arc<crate::vault::Vault>> = {
        let s = shared.lock().unwrap();
        let own = s.window(win).and_then(|w| w.vault.clone());
        let others = s.windows.iter().filter(|(other, _)| **other != win).filter_map(|(_, w)| w.vault.clone());
        own.into_iter().chain(others).collect()
    };
    let mut first_error = None;
    for vault in &vaults {
        match crate::vault::annot::reply(vault.path(), id, text) {
            Ok(page) => return json!({ "ok": true, "page": page }),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    fail(first_error.unwrap_or_else(|| tr("No hay ninguna bóveda abierta.", "No vault is open.").into()))
}

/// Saves the focused page as a PNG. The window writes the file once WebKit has
/// drawn it; the path is checked here, since it is used exactly as given.
fn screenshot(target: Page, win: WinId, send: &dyn Fn(WinId, AgentRequest)) -> Value {
    let Some(path) = target.path else {
        return fail(tr("Indica una ruta absoluta para la captura.", "Give an absolute path for the screenshot.").into());
    };
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return fail(tr("La captura debe guardarse en un archivo .png.", "The screenshot must be saved as a .png file.").into());
    }
    let (reply, answer) = mpsc::channel();
    send(win, AgentRequest::Screenshot { path: path.clone(), reply });
    match answer.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(bytes)) => json!({ "ok": true, "path": path.to_string_lossy(), "bytes": bytes }),
        Ok(Err(e)) => fail(e),
        Err(_) => fail(tr("La app no respondió.", "The app did not respond.").into()),
    }
}

/// Runs JS in the focused page. The answer waits for the script, so a script
/// that never ends freezes the window, as it would freeze the page.
fn run_script(code: String, win: WinId, send: &dyn Fn(WinId, AgentRequest)) -> Value {
    let (reply, answer) = mpsc::channel();
    send(win, AgentRequest::Eval { code, reply });
    match answer.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => fail(e),
        Err(_) => fail(tr("La app no respondió.", "The app did not respond.").into()),
    }
}

/// Waits until `view` finishes loading and reports the problems it logged.
/// The window for a request. At startup the socket answers before the first
/// window exists, and `visor open` on a closed visor asks at once: it waits
/// for that window instead of failing.
fn target_once_open(shared: &SharedRef, window: Option<u64>, file: Option<&std::path::Path>) -> Result<WinId, String> {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    let mut s = shared.lock().unwrap();
    while s.windows.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        s = LOADED.wait_timeout(s, left).unwrap().0;
    }
    s.target_for(window, file)
}

fn wait_for_load(shared: &SharedRef, win: WinId, view: u64, before: u64) -> Value {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    let mut s = shared.lock().unwrap();
    loop {
        let loaded = s.page_of(win, view).load_seq > before;
        let left = deadline.saturating_duration_since(Instant::now());
        if loaded || left.is_zero() {
            let page = s.page_of(win, view);
            let mut out = s.status(win);
            let problems: Vec<Value> = page
                .console
                .iter()
                .filter(|e| e["level"] == "error" || e["level"] == "warn")
                .cloned()
                .collect();
            out["problems"] = json!(problems);
            if !loaded {
                out["warning"] = json!(tr("La página no terminó de cargar en 10 s.", "The page did not finish loading in 10 s."));
            }
            return out;
        }
        s = LOADED.wait_timeout(s, left).unwrap().0;
    }
}

/// Annotations of the current page, of a given page, or of the whole vault.
/// For the page on screen it also says which ones could not be placed.
fn notes(scope: &NotesScope, win: WinId, shared: &SharedRef) -> Value {
    let (vault, current, orphans) = {
        let s = shared.lock().unwrap();
        let page = s.focused_page(win);
        (s.window(win).and_then(|w| w.vault.clone()), page.file.clone(), page.orphans.clone())
    };
    let Some(vault) = vault else { return fail(tr("No hay ninguna bóveda abierta.", "No vault is open.").into()) };
    let page = match scope {
        NotesScope::All => None,
        NotesScope::Page(page) => {
            let found = page.path.as_ref().and_then(|p| p.canonicalize().ok()).filter(|p| p.starts_with(vault.root()));
            Some(found.map_or(page.raw.trim_start_matches("./").to_string(), |p| vault.rel(&p)))
        }
        NotesScope::Current => match &current {
            Some(file) if crate::vault::is_loose(file) => {
                return fail(tr(
                    "Las páginas abiertas fuera de una bóveda no tienen anotaciones.",
                    "Pages open outside a vault have no annotations.",
                ).into());
            }
            Some(file) => Some(file.clone()),
            None => return fail(tr(
                "No hay ninguna página abierta; indica una ruta o usa --all.",
                "No page is open; give a path or use --all.",
            ).into()),
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
