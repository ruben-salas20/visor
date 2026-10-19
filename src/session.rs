//! What the window and the servers (agent socket, local HTTP) share, and the
//! requests the socket server makes of the window. Nothing here knows GTK.

use crate::paths;
use crate::ui::windows::{WinId, target_in};
use crate::vault::{self, Vault};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, VecDeque};
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
    /// `loose`: a `.md` outside every vault opens as a loose note (`visor open --loose`).
    Open { path: PathBuf, raw: String, place: Place, loose: bool, reply: Reply },
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
    /// Opens another window, for the process to handle. `token` lets the
    /// compositor give it the focus.
    NewWindow { token: Option<String>, reply: mpsc::Sender<Result<WinId, String>> },
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

/// What the socket server can read of one window.
#[derive(Default)]
pub struct WindowShared {
    pub vault: Option<Arc<Vault>>,
    /// Folders of the pages opened on their own, which the server may read.
    pub loose: Vec<PathBuf>,
    /// Every live page view of the window, by view id.
    pub pages: HashMap<u64, Page>,
    /// The view of the focused side's active tab: the one agents act on.
    pub focused: u64,
    /// Sides and tabs, as `visor status` reports them.
    pub layout: Value,
}

/// What the socket server can read without going through the event loop.
#[derive(Default)]
pub struct Shared {
    /// The open windows, by window id. A window is here from its opening to its closing.
    pub windows: BTreeMap<WinId, WindowShared>,
    /// The window with the focus: the one agents act on unless they name another.
    pub focused_window: Option<WinId>,
    /// While the saved windows reopen at startup. Requests wait for all of
    /// them, or `open` would pick a window before the one showing its vault exists.
    pub restoring: bool,
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

    pub fn window(&self, window: WinId) -> Option<&WindowShared> {
        self.windows.get(&window)
    }

    /// For a window that is open. Creating one for a closed window would show a
    /// ghost in `visor status`, so late replies for closed windows use `windows.get_mut`.
    pub fn window_mut(&mut self, window: WinId) -> &mut WindowShared {
        self.windows.entry(window).or_default()
    }

    /// The window an agent's request goes to: the one asked for, or the focused one.
    pub fn target(&self, asked: Option<u64>) -> Result<WinId, String> {
        target_in(self.focused_window, |w| self.windows.contains_key(&w), asked)
    }

    /// The page of `view` in `window`, or an empty page when either is gone.
    pub fn page_of(&self, window: WinId, view: u64) -> &Page {
        self.window(window).and_then(|w| w.pages.get(&view)).unwrap_or(&NO_PAGE)
    }

    /// The page on the focused side of `window`.
    pub fn focused_page(&self, window: WinId) -> &Page {
        self.page_of(window, self.window(window).map_or(0, |w| w.focused))
    }

    /// Like [`Shared::target`], for a request about `file`: without a window
    /// asked for, a window that shows the vault the file is in, the focused
    /// one first. `file` must come from [`real_path`], or `a/../b/x.md` would
    /// count as inside `a`.
    pub fn target_for(&self, asked: Option<u64>, file: Option<&Path>) -> Result<WinId, String> {
        let focused = self.target(asked)?;
        let Some(file) = file.filter(|_| asked.is_none()) else { return Ok(focused) };
        let shows = |w: &WindowShared| w.vault.as_ref().is_some_and(|v| file.starts_with(v.path()) || file.starts_with(v.root()));
        if self.window(focused).is_some_and(shows) {
            return Ok(focused);
        }
        Ok(self.windows.iter().find(|(_, w)| shows(w)).map_or(focused, |(id, _)| *id))
    }

    /// The loose-page folders of every window, each one once.
    pub fn loose_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = Vec::new();
        for folder in self.windows.values().flat_map(|w| &w.loose) {
            if !folders.contains(folder) {
                folders.push(folder.clone());
            }
        }
        folders
    }

    /// What `visor status` reports for `window`, plus the list of all open windows.
    pub fn status(&self, window: WinId) -> Value {
        let empty = WindowShared::default();
        let shown = self.window(window).unwrap_or(&empty);
        let page = self.focused_page(window);
        let vault = shown.vault.as_ref().map(|v| v.path());
        let path = page.file.as_ref().and_then(|f| match vault {
            _ if vault::is_loose(f) => Some(PathBuf::from(f)),
            Some(v) => Some(v.join(f)),
            None => None,
        });
        let errors = page.console.iter().filter(|e| e["level"] == "error").count();
        let windows: Vec<Value> = self
            .windows
            .iter()
            .map(|(id, w)| {
                json!({
                    "window": id,
                    "focused": self.focused_window == Some(*id),
                    "vault": w.vault.as_ref().map(|v| v.path().to_string_lossy()),
                })
            })
            .collect();
        json!({
            "ok": true,
            "window": window,
            "vault": vault.map(Path::to_string_lossy),
            "file": page.file,
            "path": path.as_deref().map(Path::to_string_lossy),
            "url": page.url,
            "loaded": page.loaded,
            "errors": errors,
            "last_event_id": self.next_id - 1,
            "sides": shown.layout,
            "windows": windows,
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

/// Keeps an event for `visor events` and the agents waiting on it, with its id, time, window and vault.
/// A page's own event (the only kind with `data`) is dropped if its data is too big.
pub fn push_event(shared: &mut Shared, mut event: Value, window: Option<WinId>) {
    if data_too_big(&event) {
        eprintln!("{}", crate::trf!("Un evento de visor.send supera {} KB y se ha descartado.", "A visor.send event is over {} KB and was dropped.", MAX_EVENT_DATA / 1024));
        return;
    }
    let vault = window.and_then(|w| shared.window(w)).and_then(|w| w.vault.as_ref());
    event["id"] = json!(shared.next_id);
    event["time"] = json!(now());
    event["window"] = json!(window);
    event["vault"] = json!(vault.map(|v| v.path().to_string_lossy()));
    shared.next_id += 1;
    if shared.events.len() >= MAX_EVENTS {
        shared.events.remove(0);
    }
    shared.events.push(event.clone());
    append_event(&event);
}

/// Adds a line to a view's console, dropping the oldest past `MAX_CONSOLE`.
/// Server addresses leave the text: vault paths read better, and the secret must stay out of logs.
pub fn push_console(shared: &mut Shared, window: WinId, view: u64, level: &str, text: &str) {
    let Some(w) = shared.windows.get_mut(&window) else { return };
    let base = w.vault.as_ref().map_or(vault::NO_VAULT_BASE, |v| v.base());
    let text = text.replace(base, "").replace(vault::loose_base(), "");
    let Some(page) = w.pages.get_mut(&view) else { return };
    if page.console.len() >= MAX_CONSOLE {
        page.console.remove(0);
    }
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

/// `path` without `..` or links, for [`Shared::target_for`]. A file that does
/// not exist (a tab whose file was deleted) resolves through its folder.
pub fn real_path(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok().or_else(|| Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?)))
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
        push_event(&mut shared, big, None);
        assert!(shared.events.is_empty() && shared.next_id == 1);
        // The app's own annotation events carry no `data` and are not subject to it.
        assert!(!data_too_big(&json!({ "annotation": { "text": "x".repeat(MAX_EVENT_DATA * 2) } })));
    }

    #[test]
    fn status_survives_a_vault_path_that_is_not_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
        let mut shared = Shared { next_id: 1, ..Default::default() };
        shared.window_mut(1).vault = Some(Arc::new(Vault::open(&path)));
        assert_eq!(shared.status(1)["vault"], "/tmp/\u{fffd}");
    }

    fn with_windows(ids: &[WinId]) -> Shared {
        let mut shared = Shared { next_id: 1, ..Default::default() };
        for &id in ids {
            shared.window_mut(id);
        }
        shared.focused_window = ids.last().copied();
        shared
    }

    #[test]
    fn target_picks_the_asked_window_or_the_focused_one() {
        let mut shared = with_windows(&[1, 2, 3]);
        assert_eq!(shared.target(None), Ok(3));
        assert_eq!(shared.target(Some(1)), Ok(1));
        assert_eq!(shared.target(Some(9)).unwrap_err(), "No hay ninguna ventana con el número 9");
        shared.focused_window = None;
        assert_eq!(shared.target(None).unwrap_err(), "No hay ninguna ventana abierta");
    }

    #[test]
    fn status_lists_every_window_and_reports_the_asked_one() {
        let mut shared = with_windows(&[1, 2]);
        shared.window_mut(2).focused = 7;
        let status = shared.status(1);
        assert_eq!(status["window"], 1);
        assert_eq!(status["windows"].as_array().map(Vec::len), Some(2));
        assert_eq!(status["windows"][0], json!({ "window": 1, "focused": false, "vault": null }));
        assert_eq!(status["windows"][1], json!({ "window": 2, "focused": true, "vault": null }));
    }

    #[test]
    fn events_are_stamped_with_their_window_and_its_vault() {
        let mut shared = with_windows(&[1, 2]);
        shared.window_mut(2).vault = Some(Arc::new(Vault::open(Path::new("/tmp/visor-b"))));
        push_event(&mut shared, json!({ "file": "a.html" }), Some(2));
        push_event(&mut shared, json!({ "file": "b.html" }), None);
        assert_eq!(shared.events[0]["window"], 2);
        assert_eq!(shared.events[0]["vault"], "/tmp/visor-b");
        assert_eq!(shared.events[1]["window"], json!(null));
        assert_eq!(shared.events[1]["vault"], json!(null));
    }

    #[test]
    fn loose_folders_are_listed_once_across_windows() {
        let mut shared = with_windows(&[1, 2]);
        shared.window_mut(1).loose = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        shared.window_mut(2).loose = vec![PathBuf::from("/b"), PathBuf::from("/c")];
        assert_eq!(shared.loose_folders(), vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]);
    }

    #[test]
    fn a_path_through_another_vault_goes_to_the_vault_it_ends_in() {
        let dir = temp("real-path");
        let (a, b) = (dir.join("a"), dir.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join("x.md"), "").unwrap();
        let mut shared = with_windows(&[1, 2]);
        shared.focused_window = Some(1);
        shared.window_mut(1).vault = Some(Arc::new(Vault::open(&a)));
        shared.window_mut(2).vault = Some(Arc::new(Vault::open(&b)));
        let through = real_path(&a.join("../b/x.md")).unwrap();
        assert_eq!(shared.target_for(None, Some(&through)), Ok(2));
        let gone = real_path(&a.join("../b/borrada.md")).unwrap();
        assert_eq!(shared.target_for(None, Some(&gone)), Ok(2), "a deleted file resolves through its folder");
        assert_eq!(real_path(&dir.join("no-existe/x.md")), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn open_goes_to_a_window_that_shows_the_files_vault() {
        let mut shared = with_windows(&[1, 2, 3]);
        shared.focused_window = Some(3);
        shared.window_mut(1).vault = Some(Arc::new(Vault::open(Path::new("/tmp/visor-a"))));
        shared.window_mut(2).vault = Some(Arc::new(Vault::open(Path::new("/tmp/visor-b"))));
        let file = Path::new("/tmp/visor-b/notas/x.md");
        assert_eq!(shared.target_for(None, Some(file)), Ok(2));
        assert_eq!(shared.target_for(Some(1), Some(file)), Ok(1), "a window asked for wins");
        assert_eq!(shared.target_for(None, Some(Path::new("/otra/x.md"))), Ok(3), "no vault has it: the focused window");
        assert_eq!(shared.target_for(None, None), Ok(3));
        shared.window_mut(3).vault = Some(Arc::new(Vault::open(Path::new("/tmp/visor-b"))));
        assert_eq!(shared.target_for(None, Some(file)), Ok(3), "the focused window first");
    }
}
