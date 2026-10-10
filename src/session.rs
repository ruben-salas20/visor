//! What the window and the servers (agent socket, local HTTP) share, and the
//! requests the socket server makes of the window. Nothing here knows GTK.

use crate::paths;
use crate::vault::{self, Vault};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_CONSOLE: usize = 500;
const MAX_EVENTS: usize = 1000;

/// The view that will load and the load count it had before, to wait for the next one.
pub type Loading = (u64, u64);
pub type Reply = mpsc::Sender<Result<Option<Loading>, String>>;

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
    /// Reloads the page on the focused side.
    Reload,
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

impl Shared {
    pub fn with_saved_events() -> Self {
        let mut events: Vec<Value> = fs::File::open(paths::events())
            .map(|f| {
                BufReader::new(f)
                    .lines()
                    .map_while(Result::ok)
                    .filter_map(|line| serde_json::from_str(&line).ok())
                    .collect()
            })
            .unwrap_or_default();
        let next_id = events.last().and_then(|e| e["id"].as_u64()).unwrap_or(0) + 1;
        let skip = events.len().saturating_sub(MAX_EVENTS);
        events.drain(..skip);
        Shared { events, next_id, ..Default::default() }
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

/// Keeps an event for `visor events` and the agents waiting on it, with its id, time and vault.
pub fn push_event(shared: &mut Shared, mut event: Value) {
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

fn append_event(event: &Value) {
    let path = paths::events();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{event}");
    }
}
