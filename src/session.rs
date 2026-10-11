//! What the window and the servers (agent socket, local HTTP) share, and the
//! requests the socket server makes of the window. Nothing here knows GTK.

use crate::paths;
use crate::vault::{self, Vault};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_CONSOLE: usize = 500;
const MAX_EVENTS: usize = 1000;

/// The view that will load and the load count it had before, to wait for the next one.
pub type Loading = (u64, u64);
pub type Reply = mpsc::Sender<Result<Option<Loading>, String>>;
/// How many tabs were closed, or why none was.
pub type CloseReply = mpsc::Sender<Result<usize, String>>;
/// What a page's script answered, as `visor eval` prints it.
pub type ValueReply = mpsc::Sender<Result<Value, String>>;
/// How many bytes a screenshot wrote, or why it did not.
pub type SizeReply = mpsc::Sender<Result<u64, String>>;

/// Where to open something: replacing the focused tab, in a new tab, or on the other side.
#[derive(Clone, Copy, PartialEq)]
pub enum Place {
    Here,
    NewTab,
    Beside,
}

/// What the socket server asks of the window.
pub enum AgentRequest {
    Open { path: PathBuf, raw: String, place: Place, reply: Reply },
    OpenUrl { url: String, place: Place, reply: Reply },
    /// Closes the focused side's active tab, or every tab showing the page
    /// (`path` as resolved by the caller, `raw` as written).
    Close { target: Option<(PathBuf, String)>, reply: CloseReply },
    /// Reloads the page on the focused side.
    Reload,
    /// Runs `code` in the focused page; the answer comes when the script ends.
    Eval { code: String, reply: ValueReply },
    /// Saves the focused page as a PNG at `path`, which the caller has checked.
    Screenshot { path: PathBuf, reply: SizeReply },
}

/// What a page view is showing, for the socket server.
#[derive(Default)]
pub struct Page {
    pub file: Option<String>,
    /// Set when the view shows a website instead of a vault file.
    pub url: Option<String>,
    pub loaded: bool,
    pub load_seq: u64,
    pub console: Vec<Value>,
    /// Annotations of the page that could not be placed, as reported by it.
    pub orphans: Option<Vec<String>>,
}

static NO_PAGE: Page = Page { file: None, url: None, loaded: false, load_seq: 0, console: Vec::new(), orphans: None };

/// What the socket server can read without going through the event loop.
#[derive(Default)]
pub struct Shared {
    pub vault: Option<Arc<Vault>>,
    /// Folders of the pages opened on their own, which the server may read.
    pub loose: Vec<PathBuf>,
    /// Every live page view, by view id.
    pub pages: HashMap<u64, Page>,
    /// The view of the focused side's active tab: the one agents act on.
    pub focused: u64,
    /// Sides and tabs, as `visor status` reports them.
    pub layout: Value,
    pub events: Vec<Value>,
    pub next_id: u64,
}

pub type SharedRef = Arc<Mutex<Shared>>;

/// Signalled, with the shared state, when a page finishes loading, so the
/// socket server answers `open` at once instead of polling.
pub static LOADED: Condvar = Condvar::new();

impl Shared {
    /// The last events of the log, so ids go on where they stopped. The file
    /// is cut to those at startup, so it never grows without limit.
    pub fn with_saved_events() -> Self {
        Self::with_events_from(&paths::events())
    }

    fn with_events_from(path: &Path) -> Self {
        let mut events = VecDeque::new();
        let mut last_id = 0;
        let mut total = 0;
        if let Ok(file) = fs::File::open(path) {
            let parsed = BufReader::new(file).lines().map_while(Result::ok);
            for event in parsed.filter_map(|line| serde_json::from_str::<Value>(&line).ok()) {
                last_id = event["id"].as_u64().unwrap_or(last_id);
                if events.len() == MAX_EVENTS {
                    events.pop_front();
                }
                events.push_back(event);
                total += 1;
            }
        }
        if total > MAX_EVENTS {
            let text: String = events.iter().map(|e| format!("{e}\n")).collect();
            let _ = vault::write_atomic(path, text);
        }
        Shared { events: events.into(), next_id: last_id + 1, ..Default::default() }
    }

    /// The page on the focused side.
    pub fn page(&self) -> &Page {
        self.page_of(self.focused)
    }

    pub fn page_of(&self, view: u64) -> &Page {
        self.pages.get(&view).unwrap_or(&NO_PAGE)
    }

    pub fn status(&self) -> Value {
        let page = self.page();
        let vault = self.vault.as_ref().map(|v| v.path());
        let path = page.file.as_ref().and_then(|f| match vault {
            _ if vault::is_loose(f) => Some(PathBuf::from(f)),
            Some(v) => Some(v.join(f)),
            None => None,
        });
        let errors = page.console.iter().filter(|e| e["level"] == "error").count();
        json!({
            "ok": true,
            "vault": vault.map(Path::to_string_lossy),
            "file": page.file,
            "path": path.as_deref().map(Path::to_string_lossy),
            "url": page.url,
            "loaded": page.loaded,
            "errors": errors,
            "last_event_id": self.next_id - 1,
            "sides": self.layout,
        })
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Most that a page may put in one `visor.send`, serialized. Events pile up in
/// memory and in the log, and agents read them whole.
pub const MAX_EVENT_DATA: usize = 16 * 1024;

fn data_too_big(event: &Value) -> bool {
    event.get("data").is_some_and(|data| data.to_string().len() > MAX_EVENT_DATA)
}

/// Keeps an event for `visor events` and the agents waiting on it, with its id, time and vault.
/// A page's own event (the only kind with `data`) is dropped if its data is too big.
pub fn push_event(shared: &mut Shared, mut event: Value) {
    if data_too_big(&event) {
        eprintln!("Un evento de visor.send supera {} KB y se ha descartado.", MAX_EVENT_DATA / 1024);
        return;
    }
    event["id"] = json!(shared.next_id);
    event["time"] = json!(now());
    event["vault"] = json!(shared.vault.as_ref().map(|v| v.path().to_string_lossy()));
    shared.next_id += 1;
    if shared.events.len() >= MAX_EVENTS {
        shared.events.remove(0);
    }
    shared.events.push(event.clone());
    append_event(&event);
}

/// Adds a line to a view's console, dropping the oldest past `MAX_CONSOLE`.
/// Server addresses leave the text: vault paths read better, and the secret must stay out of logs.
pub fn push_console(shared: &mut Shared, view: u64, level: &str, text: &str) {
    let Some(page) = shared.pages.get_mut(&view) else { return };
    if page.console.len() >= MAX_CONSOLE {
        page.console.remove(0);
    }
    let text = text.replace(vault::base(), "").replace(vault::loose_base(), "");
    page.console.push(json!({ "time": now(), "level": level, "text": text }));
}

fn append_event(event: &Value) {
    let path = paths::events();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{event}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-session-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn log(range: std::ops::RangeInclusive<u64>) -> String {
        range.map(|id| format!("{}\n", json!({ "id": id, "data": id }))).collect()
    }

    #[test]
    fn saved_events_are_cut_to_the_last_thousand() {
        let dir = temp("cut");
        let path = dir.join("events.jsonl");
        fs::write(&path, format!("{}no es json\n{}", log(1..=1200), log(1201..=1500))).unwrap();
        let shared = Shared::with_events_from(&path);
        assert_eq!(shared.events.len(), MAX_EVENTS);
        assert_eq!(shared.events[0]["id"], 501);
        assert_eq!(shared.events[MAX_EVENTS - 1]["id"], 1500);
        assert_eq!(shared.next_id, 1501, "ids go on after the cut");
        assert_eq!(fs::read_to_string(&path).unwrap(), log(501..=1500), "the file is cut too");
        // A second start finds the same thing and leaves the file as it is.
        assert_eq!(Shared::with_events_from(&path).next_id, 1501);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn short_or_missing_logs_are_not_rewritten() {
        let dir = temp("short");
        let path = dir.join("events.jsonl");
        assert_eq!(Shared::with_events_from(&path).next_id, 1);
        assert!(!path.exists());
        fs::write(&path, format!("{}\n", log(1..=3))).unwrap();
        let shared = Shared::with_events_from(&path);
        assert_eq!((shared.events.len(), shared.next_id), (3, 4));
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{}\n", log(1..=3)), "untouched");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn page_events_with_big_data_are_refused() {
        let small = json!({ "file": "a.html", "data": "x".repeat(1000) });
        let big = json!({ "file": "a.html", "data": "x".repeat(MAX_EVENT_DATA) });
        assert!(!data_too_big(&small));
        assert!(data_too_big(&big));
        let mut shared = Shared { next_id: 1, ..Default::default() };
        push_event(&mut shared, big);
        assert!(shared.events.is_empty() && shared.next_id == 1);
        // The app's own annotation events carry no `data` and are not subject to it.
        assert!(!data_too_big(&json!({ "annotation": { "text": "x".repeat(MAX_EVENT_DATA * 2) } })));
    }

    #[test]
    fn status_survives_a_vault_path_that_is_not_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
        let shared = Shared { vault: Some(Arc::new(Vault::open(&path))), next_id: 1, ..Default::default() };
        assert_eq!(shared.status()["vault"], "/tmp/\u{fffd}");
    }
}
