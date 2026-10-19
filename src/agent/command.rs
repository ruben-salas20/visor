//! Every order an agent can give, typed once. The command line (`from_args`) and
//! MCP (`from_mcp`) build a `Command`; the app answers it with an exhaustive `match`.
//! Its JSON form is what travels through the socket, and it must stay readable by
//! the other side of a different build (a long-lived `visor mcp` outlives updates).
pub mod tools;

use crate::i18n::tr;
use crate::platform;
use crate::trf;
use crate::vault::is_web;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Longest `events` wait MCP accepts, in seconds.
const MAX_WAIT: f64 = 600.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Ping,
    Status,
    Reload,
    /// `loose` opens a `.md` outside every vault as a loose note, like a loose `.html`.
    Open { target: Target, place: Option<OpenPlace>, loose: bool },
    /// No target closes the focused side's active tab; a page closes every tab showing it.
    Close { target: Option<Page> },
    Console { level: Option<Level>, clear: bool },
    /// Saves the focused page as a PNG. The target is resolved like `open`'s.
    Screenshot { target: Page },
    /// Runs JS in the focused page and returns its value.
    Eval { code: String },
    Events { after: Option<u64>, wait: Option<f64>, limit: Option<u64> },
    Notes { scope: NotesScope },
    NotesReply { id: String, text: String },
    /// Checks for a new release and installs it. Never restarts the app.
    Update,
    /// Answered by `visor mcp` itself, never sent to the app.
    Guide,
    /// Opens another window; sent by a second `visor` launch. `token` is the XDG
    /// activation token, so the compositor gives the new window focus.
    NewWindow { token: Option<String> },
}

/// A command and the window it goes to; `None` is the focused window.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub command: Command,
    pub window: Option<u64>,
}

/// What `open` shows: a web, or a path as the caller wrote it.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Url { url: String },
    Path(Page),
}

/// A path as written (`raw`) and, when the caller knows its directory, resolved (`path`).
/// Without `path` the app looks for `raw` inside the open vault.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub raw: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpenPlace {
    Here,
    Tab,
    Beside,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    Error,
}

/// Which annotations `notes` returns.
#[derive(Debug, Clone, PartialEq)]
pub enum NotesScope {
    Current,
    All,
    Page(Page),
}

const WIRE_NAMES: [&str; 13] = [
    "ping",
    "status",
    "reload",
    "open",
    "close",
    "console",
    "screenshot",
    "eval",
    "events",
    "notes",
    "notes_reply",
    "update",
    "new_window",
];

/// Orders that do not act on a window: events and the update are the whole
/// process's, and guide is fixed text. They refuse `--window` and `window`.
pub const WINDOWLESS: [&str; 3] = ["events", "update", "guide"];

/// The error for a socket field that is missing or of the wrong kind.
fn invalid_field(field: &str) -> String {
    trf!("Petición no válida: el campo {field} no es válido.", "Invalid request: field {field} is not valid.")
}

/// Removes `--window N` at `at` and returns N. Window numbers start at 1.
fn take_window(args: &mut Vec<String>, at: usize) -> Result<u64, String> {
    let number = args.get(at + 1).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0);
    let number = number.ok_or_else(|| tr("--window necesita un número de ventana.", "--window needs a window number.").to_string())?;
    args.drain(at..=at + 1);
    Ok(number)
}

impl Page {
    /// `base` is the directory `raw` is relative to, when the caller has one.
    fn resolve(raw: &str, base: Option<&Path>) -> Self {
        Self { raw: raw.to_string(), path: base.map(|base| base.join(raw)) }
    }

    /// For callers without a directory of their own (MCP): only absolute paths resolve.
    fn absolute_only(raw: &str) -> Self {
        // Same test as vault::is_loose: on Windows `\x` and `C:/x` count too.
        Self::resolve(raw, platform::is_absolute_text(raw).then(|| Path::new("")))
    }
}

impl Command {
    /// Reads the command line. `args` is not empty: its first item is the command.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        Self::from_args_in(args, || std::env::current_dir().map_err(|e| e.to_string()))
    }

    /// `cwd` is asked only when a path has to be resolved.
    fn from_args_in(args: &[String], cwd: impl Fn() -> Result<PathBuf, String>) -> Result<Self, String> {
        let (cmd, rest) = args.split_first().map_or(("", &[][..]), |(cmd, rest)| (cmd.as_str(), rest));
        let flag = |name: &str| rest.iter().any(|a| a == name);
        let value = |name: &str| -> Result<Option<&str>, String> {
            match rest.iter().position(|a| a == name) {
                None => Ok(None),
                Some(i) => rest.get(i + 1).map(|v| Some(v.as_str())).ok_or_else(|| trf!("{name} necesita un número.", "{name} needs a number.")),
            }
        };
        let count = |name: &str| -> Result<Option<u64>, String> {
            value(name)?
                .map(|v| v.parse().map_err(|_| trf!("{name} necesita un número.", "{name} needs a number.")))
                .transpose()
        };
        match cmd {
            "open" => {
                let usage = tr(
                    "Uso: visor open <carpeta, archivo o URL> [--tab | --beside] [--loose]",
                    "Usage: visor open <folder, file or URL> [--tab | --beside] [--loose]",
                );
                let raw = rest.iter().find(|a| !a.starts_with("--")).ok_or(usage)?;
                if let Some(unknown) = rest.iter().find(|a| a.starts_with("--") && !matches!(a.as_str(), "--tab" | "--beside" | "--loose")) {
                    return Err(trf!("Opción desconocida: {unknown}. {usage}", "Unknown option: {unknown}. {usage}"));
                }
                let place = if flag("--beside") {
                    Some(OpenPlace::Beside)
                } else if flag("--tab") {
                    Some(OpenPlace::Tab)
                } else {
                    None
                };
                let target = if is_web(raw) {
                    Target::Url { url: raw.clone() }
                } else {
                    Target::Path(Page::resolve(raw, Some(&cwd()?)))
                };
                Ok(Self::Open { target, place, loose: flag("--loose") })
            }
            "close" => {
                let usage = tr("Uso: visor close [<ruta>]", "Usage: visor close [<path>]");
                if let Some(unknown) = rest.iter().find(|a| a.starts_with("--")) {
                    return Err(trf!("Opción desconocida: {unknown}. {usage}", "Unknown option: {unknown}. {usage}"));
                }
                match rest {
                    [] => Ok(Self::Close { target: None }),
                    [raw] => Ok(Self::Close { target: Some(Page::resolve(raw, Some(&cwd()?))) }),
                    _ => Err(usage.into()),
                }
            }
            "status" => Ok(Self::Status),
            "reload" => Ok(Self::Reload),
            "update" => Ok(Self::Update),
            "screenshot" => {
                let usage = tr("Uso: visor screenshot <archivo.png>", "Usage: visor screenshot <file.png>");
                if let Some(unknown) = rest.iter().find(|a| a.starts_with("--")) {
                    return Err(trf!("Opción desconocida: {unknown}. {usage}", "Unknown option: {unknown}. {usage}"));
                }
                match rest {
                    [raw] => Ok(Self::Screenshot { target: Page::resolve(raw, Some(&cwd()?)) }),
                    _ => Err(usage.into()),
                }
            }
            "eval" => {
                if rest.is_empty() {
                    return Err(tr("Uso: visor eval <js>", "Usage: visor eval <js>").into());
                }
                Ok(Self::Eval { code: rest.join(" ") })
            }
            "console" => Ok(Self::Console { level: flag("--errors").then_some(Level::Error), clear: flag("--clear") }),
            "events" => {
                let wait = value("--wait")?
                    .map(|v| v.parse::<f64>().ok().filter(|w| w.is_finite() && *w >= 0.0))
                    .map(|w| w.ok_or(tr("--wait necesita un número.", "--wait needs a number.")))
                    .transpose()?;
                Ok(Self::Events { after: count("--after")?, wait, limit: count("--limit")? })
            }
            "notes" => match rest.first().map(String::as_str) {
                Some("reply") => {
                    let usage = tr("Uso: visor notes reply <id> <texto>", "Usage: visor notes reply <id> <text>");
                    let id = rest.get(1).ok_or(usage)?;
                    let text = rest[2..].join(" ");
                    if text.is_empty() {
                        return Err(usage.into());
                    }
                    Ok(Self::NotesReply { id: id.clone(), text })
                }
                Some("--all") => Ok(Self::Notes { scope: NotesScope::All }),
                Some(raw) => Ok(Self::Notes { scope: NotesScope::Page(Page::resolve(raw, Some(&cwd()?))) }),
                None => Ok(Self::Notes { scope: NotesScope::Current }),
            },
            other => Err(trf!("Comando desconocido: {other}. Usa `visor help`.", "Unknown command: {other}. Use `visor help`.")),
        }
    }

    /// Reads an MCP tool call, checking its arguments against the tool's schema
    /// (see `tools::list`). A relative path stays unresolved: this process's
    /// directory belongs to the client, not to the person's vault.
    pub fn from_mcp(tool: &str, args: &Value) -> Result<Self, String> {
        let allowed: &[&str] = match tool {
            "open" => &["target", "place", "loose"],
            "close" => &["target"],
            "console" => &["errors_only", "clear"],
            "screenshot" => &["path"],
            "eval" => &["code"],
            "events" => &["after", "wait", "limit"],
            "notes" => &["page", "all"],
            "notes_reply" => &["id", "text"],
            "status" | "reload" | "update" | "guide" => &[],
            _ => return Err(trf!(
                "Herramienta desconocida: {tool}. Usa tools/list para ver las disponibles.",
                "Unknown tool: {tool}. Use tools/list to see the available tools.",
            )),
        };
        let args = Args::new(args, allowed)?;
        Ok(match tool {
            "open" => {
                args.require("target")?;
                let target = args.string("target")?.unwrap_or_default();
                let place = match args.choice("place", &["here", "tab", "beside"])? {
                    Some("here") => Some(OpenPlace::Here),
                    Some("tab") => Some(OpenPlace::Tab),
                    Some(_) => Some(OpenPlace::Beside),
                    None => None,
                };
                let target = if is_web(target) {
                    Target::Url { url: target.to_string() }
                } else {
                    Target::Path(Page::absolute_only(target))
                };
                let loose = args.boolean("loose")?.unwrap_or(false);
                Self::Open { target, place, loose }
            }
            "close" => Self::Close { target: args.string("target")?.map(Page::absolute_only) },
            "console" => Self::Console {
                level: args.boolean("errors_only")?.unwrap_or(false).then_some(Level::Error),
                clear: args.boolean("clear")?.unwrap_or(false),
            },
            "events" => Self::Events {
                after: args.integer("after")?,
                wait: args.number("wait", MAX_WAIT)?,
                limit: args.integer("limit")?,
            },
            "notes" => {
                let (page, all) = (args.string("page")?, args.boolean("all")?.unwrap_or(false));
                match (page, all) {
                    (Some(_), true) => return Err(tr("Usa page o all, no los dos.", "Use page or all, not both.").into()),
                    (_, true) => Self::Notes { scope: NotesScope::All },
                    (Some(page), _) => Self::Notes { scope: NotesScope::Page(Page::absolute_only(page)) },
                    (None, _) => Self::Notes { scope: NotesScope::Current },
                }
            }
            "notes_reply" => {
                args.require("id")?;
                args.require("text")?;
                Self::NotesReply {
                    id: args.string("id")?.unwrap_or_default().to_string(),
                    text: args.string("text")?.unwrap_or_default().to_string(),
                }
            }
            "screenshot" => {
                args.require("path")?;
                Self::Screenshot { target: Page::absolute_only(args.string("path")?.unwrap_or_default()) }
            }
            "eval" => {
                args.require("code")?;
                Self::Eval { code: args.string("code")?.unwrap_or_default().to_string() }
            }
            "status" => Self::Status,
            "reload" => Self::Reload,
            "update" => Self::Update,
            _ => Self::Guide,
        })
    }

    /// Reads a request from the socket.
    pub fn from_wire(request: &Value) -> Result<Self, String> {
        let name = request["cmd"].as_str().unwrap_or("");
        if !WIRE_NAMES.contains(&name) {
            return Err(trf!("Comando desconocido: {name}", "Unknown command: {name}"));
        }
        let invalid = invalid_field;
        let text = |field: &str| request[field].as_str().map(String::from).ok_or_else(|| invalid(field));
        let optional = |field: &str| match &request[field] {
            Value::Null => Ok(None),
            Value::String(text) => Ok(Some(text.as_str())),
            _ => Err(invalid(field)),
        };
        let number = |field: &str| match &request[field] {
            Value::Null => Ok(None),
            value => value.as_f64().map(Some).ok_or_else(|| invalid(field)),
        };
        let count = |field: &str| match &request[field] {
            Value::Null => Ok(None),
            value => value.as_u64().map(Some).ok_or_else(|| invalid(field)),
        };
        // "" is how the socket says there is no resolved path.
        let path = || Ok::<_, String>(optional("path")?.filter(|p| !p.is_empty()).map(PathBuf::from));
        Ok(match name {
            "ping" => Self::Ping,
            "status" => Self::Status,
            "reload" => Self::Reload,
            "update" => Self::Update,
            "open" => Self::Open {
                target: match optional("url")? {
                    Some(url) => Target::Url { url: url.to_string() },
                    None => Target::Path(Page { raw: text("raw")?, path: path()? }),
                },
                place: match optional("place")? {
                    None => None,
                    Some("here") => Some(OpenPlace::Here),
                    Some("tab") => Some(OpenPlace::Tab),
                    Some("beside") => Some(OpenPlace::Beside),
                    Some(_) => return Err(invalid("place")),
                },
                // Clients from before this flag send no `loose`.
                loose: match &request["loose"] {
                    Value::Null => false,
                    Value::Bool(loose) => *loose,
                    _ => return Err(invalid("loose")),
                },
            },
            "close" => Self::Close {
                target: match (optional("raw")?, path()?) {
                    (Some(raw), path) => Some(Page { raw: raw.to_string(), path }),
                    (None, Some(path)) => Some(Page { raw: String::new(), path: Some(path) }),
                    (None, None) => None,
                },
            },
            "console" => Self::Console {
                level: match optional("level")? {
                    None => None,
                    Some("error") => Some(Level::Error),
                    Some(_) => return Err(invalid("level")),
                },
                clear: request["clear"].as_bool().unwrap_or(false),
            },
            "screenshot" => Self::Screenshot { target: Page { raw: text("raw")?, path: path()? } },
            "eval" => Self::Eval { code: text("code")? },
            "events" => Self::Events { after: count("after")?, wait: number("wait")?, limit: count("limit")? },
            "notes" => Self::Notes {
                scope: if request["all"].as_bool() == Some(true) {
                    NotesScope::All
                } else if let Some(raw) = optional("raw")? {
                    NotesScope::Page(Page { raw: raw.to_string(), path: path()? })
                } else if let Some(path) = path()? {
                    NotesScope::Page(Page { raw: String::new(), path: Some(path) })
                } else {
                    NotesScope::Current
                },
            },
            "new_window" => Self::NewWindow { token: optional("token")?.filter(|t| !t.is_empty()).map(String::from) },
            _ => Self::NotesReply { id: text("id")?, text: text("text")? },
        })
    }

    /// The JSON line for a command without a window. Tests only: the CLI sends a `Request`.
    #[cfg(test)]
    pub fn to_wire(&self) -> String {
        self.wire_value().to_string()
    }

    fn wire_value(&self) -> Value {
        let place = |place: &Option<OpenPlace>| {
            place.map(|p| match p {
                OpenPlace::Here => "here",
                OpenPlace::Tab => "tab",
                OpenPlace::Beside => "beside",
            })
        };
        // No resolved path travels as "".
        let page = |page: &Page| json!({ "path": page.path.as_deref().unwrap_or(Path::new("")).to_string_lossy(), "raw": page.raw });
        match self {
            Self::Ping => json!({ "cmd": "ping" }),
            Self::Status => json!({ "cmd": "status" }),
            Self::Reload => json!({ "cmd": "reload" }),
            Self::Update => json!({ "cmd": "update" }),
            Self::Open { target, place: at, loose } => {
                let mut wire = match target {
                    Target::Url { url } => json!({ "url": url }),
                    Target::Path(target) => page(target),
                };
                wire["cmd"] = json!("open");
                wire["place"] = json!(place(at));
                if *loose {
                    wire["loose"] = json!(true);
                }
                wire
            }
            Self::Close { target } => match target {
                None => json!({ "cmd": "close" }),
                Some(target) => {
                    let mut wire = page(target);
                    wire["cmd"] = json!("close");
                    wire
                }
            },
            Self::Console { level, clear } => {
                json!({ "cmd": "console", "level": level.map(|_| "error"), "clear": clear })
            }
            Self::Screenshot { target } => {
                let mut wire = page(target);
                wire["cmd"] = json!("screenshot");
                wire
            }
            Self::Eval { code } => json!({ "cmd": "eval", "code": code }),
            Self::Events { after, wait, limit } => {
                json!({ "cmd": "events", "after": after, "wait": wait, "limit": limit })
            }
            Self::Notes { scope } => match scope {
                NotesScope::Current => json!({ "cmd": "notes" }),
                NotesScope::All => json!({ "cmd": "notes", "all": true }),
                NotesScope::Page(target) => {
                    let mut wire = page(target);
                    wire["cmd"] = json!("notes");
                    wire
                }
            },
            Self::NotesReply { id, text } => json!({ "cmd": "notes_reply", "id": id, "text": text }),
            Self::Guide => json!({ "cmd": "guide" }),
            Self::NewWindow { token } => json!({ "cmd": "new_window", "token": token }),
        }
    }
}

impl Request {
    /// Reads the command line. `--window N` goes before the command name, or right
    /// after it. Not after `eval`: its code may contain that text.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut rest = args.to_vec();
        let mut window = None;
        if rest.first().map(String::as_str) == Some("--window") {
            window = Some(take_window(&mut rest, 0)?);
        }
        let after_name = rest.first().map(String::as_str) != Some("eval") && rest.get(1).map(String::as_str) == Some("--window");
        if after_name {
            if window.is_some() {
                return Err(tr("--window solo puede aparecer una vez.", "--window can appear only once.").into());
            }
            window = Some(take_window(&mut rest, 1)?);
        }
        let command = Command::from_args(&rest)?;
        if let Some(name) = rest.first().filter(|n| window.is_some() && WINDOWLESS.contains(&n.as_str())) {
            return Err(trf!("{name} no va a una ventana: no acepta --window.", "{name} does not go to a window: it does not take --window."));
        }
        Ok(Self { command, window })
    }

    /// Reads an MCP tool call. Every tool but those in [`WINDOWLESS`] accepts `window`.
    pub fn from_mcp(tool: &str, arguments: &Value) -> Result<Self, String> {
        // The windowless ones keep their own checks, which refuse `window` as an unknown argument.
        let Some(map) = arguments.as_object().filter(|_| !WINDOWLESS.contains(&tool)) else {
            return Ok(Self { command: Command::from_mcp(tool, arguments)?, window: None });
        };
        let mut map = map.clone();
        let window = match map.remove("window") {
            None => None,
            Some(value) => Some(value.as_u64().filter(|n| *n > 0).ok_or_else(|| Args::invalid("window"))?),
        };
        Ok(Self { command: Command::from_mcp(tool, &Value::Object(map))?, window })
    }

    /// Reads a request from the socket line.
    pub fn from_wire(request: &Value) -> Result<Self, String> {
        let command = Command::from_wire(request)?;
        let window = match &request["window"] {
            Value::Null => None,
            value => Some(value.as_u64().filter(|n| *n > 0).ok_or_else(|| invalid_field("window"))?),
        };
        Ok(Self { command, window })
    }

    /// The JSON line sent through the socket. Without a window, no `window` key:
    /// the line is the same one older apps have always read.
    pub fn to_wire(&self) -> String {
        let mut wire = self.command.wire_value();
        if let Some(window) = self.window {
            wire["window"] = json!(window);
        }
        wire.to_string()
    }
}

/// The arguments of one MCP tool call. A wrong type, an empty string or a
/// value out of range all report the same message, naming the argument.
struct Args<'a>(&'a serde_json::Map<String, Value>);

impl<'a> Args<'a> {
    fn new(value: &'a Value, allowed: &[&str]) -> Result<Self, String> {
        let Some(map) = value.as_object() else {
            return Err(tr("Los argumentos deben ser un objeto.", "Arguments must be an object.").into());
        };
        if let Some(key) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(trf!("Argumento desconocido: {key}.", "Unknown argument: {key}."));
        }
        Ok(Self(map))
    }

    fn require(&self, key: &str) -> Result<(), String> {
        if self.0.contains_key(key) { Ok(()) } else { Err(trf!("Falta el argumento obligatorio: {key}.", "Missing required argument: {key}.")) }
    }

    fn invalid(key: &str) -> String {
        trf!("El argumento {key} no es válido.", "Argument {key} is not valid.")
    }

    /// A non-empty string.
    fn string(&self, key: &str) -> Result<Option<&'a str>, String> {
        match self.0.get(key) {
            None => Ok(None),
            Some(v) => v.as_str().filter(|s| !s.is_empty()).map(Some).ok_or_else(|| Self::invalid(key)),
        }
    }

    fn choice(&self, key: &str, options: &[&str]) -> Result<Option<&'a str>, String> {
        match self.0.get(key) {
            None => Ok(None),
            Some(v) => v.as_str().filter(|s| options.contains(s)).map(Some).ok_or_else(|| Self::invalid(key)),
        }
    }

    fn boolean(&self, key: &str) -> Result<Option<bool>, String> {
        match self.0.get(key) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| Self::invalid(key)),
        }
    }

    /// An integer, zero or more.
    fn integer(&self, key: &str) -> Result<Option<u64>, String> {
        match self.0.get(key) {
            None => Ok(None),
            Some(v) => v.as_u64().map(Some).ok_or_else(|| Self::invalid(key)),
        }
    }

    /// A number from zero to `max`.
    fn number(&self, key: &str, max: f64) -> Result<Option<f64>, String> {
        match self.0.get(key) {
            None => Ok(None),
            Some(v) => v.as_f64().filter(|n| (0.0..=max).contains(n)).map(Some).ok_or_else(|| Self::invalid(key)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    fn cli(text: &str) -> Result<Command, String> {
        Command::from_args_in(&args(text), || Ok(PathBuf::from("/work")))
    }

    fn mcp(tool: &str, arguments: Value) -> Result<Command, String> {
        Command::from_mcp(tool, &arguments)
    }

    /// The path the CLI gives `rel` under the test directory /work, in this platform's form.
    fn work(rel: &str) -> String {
        PathBuf::from("/work").join(rel).to_string_lossy().into_owned()
    }

    fn page(raw: &str, path: Option<&str>) -> Page {
        Page { raw: raw.into(), path: path.map(PathBuf::from) }
    }

    #[test]
    fn open_from_args() {
        assert_eq!(
            cli("open notas/a.html").unwrap(),
            Command::Open { target: Target::Path(page("notas/a.html", Some("/work/notas/a.html"))), place: None, loose: false }
        );
        assert_eq!(
            cli("open /abs/a.html --tab").unwrap(),
            Command::Open { target: Target::Path(page("/abs/a.html", Some("/abs/a.html"))), place: Some(OpenPlace::Tab), loose: false }
        );
        assert_eq!(
            cli("open --beside https://example.com").unwrap(),
            Command::Open { target: Target::Url { url: "https://example.com".into() }, place: Some(OpenPlace::Beside), loose: false }
        );
        // --beside wins over --tab.
        assert!(matches!(cli("open x --tab --beside"), Ok(Command::Open { place: Some(OpenPlace::Beside), .. })));
    }

    #[test]
    fn open_loose_flag() {
        assert!(matches!(cli("open notas/a.md --loose"), Ok(Command::Open { loose: true, .. })));
        assert!(matches!(cli("open notas/a.md"), Ok(Command::Open { loose: false, .. })));
        assert!(matches!(mcp("open", json!({ "target": "/abs/a.md", "loose": true })), Ok(Command::Open { loose: true, .. })));
        assert!(matches!(mcp("open", json!({ "target": "/abs/a.md" })), Ok(Command::Open { loose: false, .. })));
        assert_eq!(mcp("open", json!({ "target": "/abs/a.md", "loose": "yes" })), Err("El argumento loose no es válido.".into()));
    }

    /// Clients from before `loose` send no such key, and the socket still reads them.
    #[test]
    fn wire_carries_loose() {
        let wire = |command: Command| serde_json::from_str::<Value>(&command.to_wire()).unwrap();
        let loose = cli("open /abs/a.md --loose").unwrap();
        assert_eq!(wire(loose.clone())["loose"], json!(true));
        assert_eq!(Command::from_wire(&wire(loose.clone())).unwrap(), loose);
        assert_eq!(
            Command::from_wire(&json!({ "cmd": "open", "path": "", "raw": "a.md", "place": null })).unwrap(),
            Command::Open { target: Target::Path(page("a.md", None)), place: None, loose: false }
        );
    }

    #[test]
    fn open_from_args_errors() {
        let usage = "Uso: visor open <carpeta, archivo o URL> [--tab | --beside] [--loose]";
        assert_eq!(cli("open").unwrap_err(), usage);
        assert_eq!(cli("open --tab").unwrap_err(), usage);
        assert_eq!(cli("open x --fast").unwrap_err(), format!("Opción desconocida: --fast. {usage}"));
    }

    #[test]
    fn close_from_args() {
        assert_eq!(cli("close").unwrap(), Command::Close { target: None });
        assert_eq!(
            cli("close x/a.html").unwrap(),
            Command::Close { target: Some(page("x/a.html", Some("/work/x/a.html"))) }
        );
        let usage = "Uso: visor close [<ruta>]";
        assert_eq!(cli("close a b").unwrap_err(), usage);
        assert_eq!(cli("close --all").unwrap_err(), format!("Opción desconocida: --all. {usage}"));
    }

    #[test]
    fn simple_commands_from_args() {
        assert_eq!(cli("status").unwrap(), Command::Status);
        assert_eq!(cli("reload").unwrap(), Command::Reload);
        assert_eq!(cli("update").unwrap(), Command::Update);
        assert_eq!(cli("console").unwrap(), Command::Console { level: None, clear: false });
        assert_eq!(
            cli("console --errors --clear").unwrap(),
            Command::Console { level: Some(Level::Error), clear: true }
        );
        assert_eq!(cli("notes").unwrap(), Command::Notes { scope: NotesScope::Current });
        assert_eq!(cli("notes --all").unwrap(), Command::Notes { scope: NotesScope::All });
        assert_eq!(
            cli("notes a/b.html").unwrap(),
            Command::Notes { scope: NotesScope::Page(page("a/b.html", Some("/work/a/b.html"))) }
        );
        // The path wins over a later --all, as it always has.
        assert!(matches!(cli("notes a.html --all"), Ok(Command::Notes { scope: NotesScope::Page(_) })));
    }

    #[test]
    fn screenshot_and_eval_from_args() {
        assert_eq!(
            cli("screenshot shots/a.png").unwrap(),
            Command::Screenshot { target: page("shots/a.png", Some("/work/shots/a.png")) }
        );
        let usage = "Uso: visor screenshot <archivo.png>";
        assert_eq!(cli("screenshot").unwrap_err(), usage);
        assert_eq!(cli("screenshot a.png b.png").unwrap_err(), usage);
        assert_eq!(cli("screenshot --size 800x600 a.png").unwrap_err(), format!("Opción desconocida: --size. {usage}"));
        assert_eq!(cli("eval document.title").unwrap(), Command::Eval { code: "document.title".into() });
        // Unquoted code arrives as several words; they are joined back.
        assert_eq!(cli("eval 1 + 2").unwrap(), Command::Eval { code: "1 + 2".into() });
        assert_eq!(cli("eval").unwrap_err(), "Uso: visor eval <js>");
    }

    #[test]
    fn screenshot_and_eval_from_mcp() {
        assert_eq!(
            mcp("screenshot", json!({ "path": "/abs/a.png" })).unwrap(),
            Command::Screenshot { target: page("/abs/a.png", Some("/abs/a.png")) }
        );
        // A relative path stays unresolved; the window refuses it.
        assert_eq!(
            mcp("screenshot", json!({ "path": "a.png" })).unwrap(),
            Command::Screenshot { target: page("a.png", None) }
        );
        assert_eq!(mcp("eval", json!({ "code": "1" })).unwrap(), Command::Eval { code: "1".into() });
        assert_eq!(mcp("eval", json!({})).unwrap_err(), "Falta el argumento obligatorio: code.");
    }

    #[test]
    fn events_from_args() {
        assert_eq!(cli("events").unwrap(), Command::Events { after: None, wait: None, limit: None });
        assert_eq!(
            cli("events --after 7 --wait 2.5 --limit 3").unwrap(),
            Command::Events { after: Some(7), wait: Some(2.5), limit: Some(3) }
        );
        for bad in ["events --after x", "events --after", "events --after -1", "events --limit 1.5"] {
            let name = if bad.contains("limit") { "--limit" } else { "--after" };
            assert_eq!(cli(bad).unwrap_err(), format!("{name} necesita un número."), "{bad}");
        }
        for bad in ["events --wait x", "events --wait", "events --wait -1", "events --wait inf", "events --wait NaN"] {
            assert_eq!(cli(bad).unwrap_err(), "--wait necesita un número.", "{bad}");
        }
        // The command line has no upper bound on --wait; MCP caps it.
        assert!(cli("events --wait 1000").is_ok());
    }

    #[test]
    fn notes_reply_from_args() {
        assert_eq!(
            cli("notes reply abc Hola mundo").unwrap(),
            Command::NotesReply { id: "abc".into(), text: "Hola mundo".into() }
        );
        let usage = "Uso: visor notes reply <id> <texto>";
        assert_eq!(cli("notes reply").unwrap_err(), usage);
        assert_eq!(cli("notes reply abc").unwrap_err(), usage);
    }

    #[test]
    fn unknown_command_from_args() {
        assert_eq!(cli("volar").unwrap_err(), "Comando desconocido: volar. Usa `visor help`.");
        // ping is for the app's own use; it is not a command line order.
        assert!(cli("ping").is_err());
        assert!(cli("guide").is_err());
    }

    #[test]
    fn cwd_is_asked_only_when_needed() {
        let fail = || Err::<PathBuf, _>("sin cwd".to_string());
        assert!(Command::from_args_in(&args("status"), fail).is_ok());
        assert!(Command::from_args_in(&args("open https://a.b"), fail).is_ok());
        assert_eq!(Command::from_args_in(&args("open x"), fail).unwrap_err(), "sin cwd");
    }

    #[test]
    fn open_from_mcp() {
        assert_eq!(
            mcp("open", json!({ "target": "/abs/a.html" })).unwrap(),
            Command::Open { target: Target::Path(page("/abs/a.html", Some("/abs/a.html"))), place: None, loose: false }
        );
        assert_eq!(
            mcp("open", json!({ "target": "notas/a.html", "place": "here" })).unwrap(),
            Command::Open { target: Target::Path(page("notas/a.html", None)), place: Some(OpenPlace::Here), loose: false }
        );
        assert_eq!(
            mcp("open", json!({ "target": "http://x.y", "place": "tab" })).unwrap(),
            Command::Open { target: Target::Url { url: "http://x.y".into() }, place: Some(OpenPlace::Tab), loose: false }
        );
        assert!(matches!(
            mcp("open", json!({ "target": "a", "place": "beside" })),
            Ok(Command::Open { place: Some(OpenPlace::Beside), .. })
        ));
    }

    #[test]
    fn simple_tools_from_mcp() {
        assert_eq!(mcp("status", json!({})).unwrap(), Command::Status);
        assert_eq!(mcp("reload", json!({})).unwrap(), Command::Reload);
        assert_eq!(mcp("update", json!({})).unwrap(), Command::Update);
        assert_eq!(mcp("update", json!({ "force": true })), Err("Argumento desconocido: force.".into()));
        assert_eq!(mcp("guide", json!({})).unwrap(), Command::Guide);
        assert_eq!(mcp("close", json!({})).unwrap(), Command::Close { target: None });
        assert_eq!(
            mcp("close", json!({ "target": "/v/a.html" })).unwrap(),
            Command::Close { target: Some(page("/v/a.html", Some("/v/a.html"))) }
        );
        assert_eq!(
            mcp("close", json!({ "target": "a.html" })).unwrap(),
            Command::Close { target: Some(page("a.html", None)) }
        );
        assert_eq!(mcp("console", json!({})).unwrap(), Command::Console { level: None, clear: false });
        assert_eq!(
            mcp("console", json!({ "errors_only": true, "clear": true })).unwrap(),
            Command::Console { level: Some(Level::Error), clear: true }
        );
        assert_eq!(
            mcp("console", json!({ "errors_only": false })).unwrap(),
            Command::Console { level: None, clear: false }
        );
        assert_eq!(mcp("notes", json!({})).unwrap(), Command::Notes { scope: NotesScope::Current });
        assert_eq!(mcp("notes", json!({ "all": true })).unwrap(), Command::Notes { scope: NotesScope::All });
        assert_eq!(mcp("notes", json!({ "all": false })).unwrap(), Command::Notes { scope: NotesScope::Current });
        assert_eq!(
            mcp("notes", json!({ "page": "a.html" })).unwrap(),
            Command::Notes { scope: NotesScope::Page(page("a.html", None)) }
        );
        assert_eq!(
            mcp("notes", json!({ "page": "/v/a.html" })).unwrap(),
            Command::Notes { scope: NotesScope::Page(page("/v/a.html", Some("/v/a.html"))) }
        );
        assert_eq!(
            mcp("notes_reply", json!({ "id": "x1", "text": "Hecho" })).unwrap(),
            Command::NotesReply { id: "x1".into(), text: "Hecho".into() }
        );
    }

    #[test]
    fn events_from_mcp() {
        assert_eq!(mcp("events", json!({})).unwrap(), Command::Events { after: None, wait: None, limit: None });
        assert_eq!(
            mcp("events", json!({ "after": 4, "wait": 600, "limit": 0 })).unwrap(),
            Command::Events { after: Some(4), wait: Some(600.0), limit: Some(0) }
        );
        assert!(mcp("events", json!({ "wait": 0.5 })).is_ok());
    }

    #[test]
    fn mcp_errors() {
        let invalid = |key: &str| Err(format!("El argumento {key} no es válido."));
        assert_eq!(mcp("volar", json!({})), Err("Herramienta desconocida: volar. Usa tools/list para ver las disponibles.".into()));
        assert_eq!(mcp("status", json!([])), Err("Los argumentos deben ser un objeto.".into()));
        assert_eq!(mcp("status", json!({ "x": 1 })), Err("Argumento desconocido: x.".into()));
        assert_eq!(mcp("open", json!({})), Err("Falta el argumento obligatorio: target.".into()));
        assert_eq!(mcp("open", json!({ "place": "tab" })), Err("Falta el argumento obligatorio: target.".into()));
        assert_eq!(mcp("open", json!({ "target": 3 })), invalid("target"));
        assert_eq!(mcp("open", json!({ "target": "" })), invalid("target"));
        assert_eq!(mcp("open", json!({ "target": "a", "place": "there" })), invalid("place"));
        assert_eq!(mcp("open", json!({ "target": "a", "place": null })), invalid("place"));
        assert_eq!(mcp("console", json!({ "clear": "yes" })), invalid("clear"));
        assert_eq!(mcp("events", json!({ "after": -1 })), invalid("after"));
        assert_eq!(mcp("events", json!({ "after": 1.5 })), invalid("after"));
        assert_eq!(mcp("events", json!({ "limit": "3" })), invalid("limit"));
        assert_eq!(mcp("events", json!({ "wait": 601 })), invalid("wait"));
        assert_eq!(mcp("events", json!({ "wait": -1 })), invalid("wait"));
        assert_eq!(mcp("events", json!({ "wait": "5" })), invalid("wait"));
        assert_eq!(mcp("notes", json!({ "page": "" })), invalid("page"));
        assert_eq!(mcp("notes", json!({ "page": "a", "all": true })), Err("Usa page o all, no los dos.".into()));
        assert_eq!(mcp("close", json!({ "target": "" })), invalid("target"));
        assert_eq!(mcp("close", json!({ "page": "a" })), Err("Argumento desconocido: page.".into()));
        assert_eq!(mcp("notes_reply", json!({ "id": "x" })), Err("Falta el argumento obligatorio: text.".into()));
        assert_eq!(mcp("notes_reply", json!({ "id": "x", "text": "" })), invalid("text"));
    }

    /// The same intent by command line and by MCP is the same `Command`; only
    /// the path resolved from the working directory exists on the command line.
    #[test]
    fn cli_and_mcp_agree() {
        let unresolved = |command: Command| match command {
            Command::Open { target: Target::Path(Page { raw, .. }), place, loose } => {
                Command::Open { target: Target::Path(Page { raw, path: None }), place, loose }
            }
            Command::Notes { scope: NotesScope::Page(Page { raw, .. }) } => {
                Command::Notes { scope: NotesScope::Page(Page { raw, path: None }) }
            }
            Command::Close { target: Some(Page { raw, .. }) } => {
                Command::Close { target: Some(Page { raw, path: None }) }
            }
            other => other,
        };
        let table = [
            ("open x/a.html", "open", json!({ "target": "x/a.html" })),
            ("open x/a.html --tab", "open", json!({ "target": "x/a.html", "place": "tab" })),
            ("open x/a.html --beside", "open", json!({ "target": "x/a.html", "place": "beside" })),
            ("open https://a.b/c", "open", json!({ "target": "https://a.b/c" })),
            ("open /abs/a.html", "open", json!({ "target": "/abs/a.html" })),
            ("open x/a.md --loose", "open", json!({ "target": "x/a.md", "loose": true })),
            ("status", "status", json!({})),
            ("reload", "reload", json!({})),
            ("close", "close", json!({})),
            ("close x/a.html", "close", json!({ "target": "x/a.html" })),
            ("close /abs/a.html", "close", json!({ "target": "/abs/a.html" })),
            ("console", "console", json!({})),
            ("console --errors", "console", json!({ "errors_only": true })),
            ("console --clear", "console", json!({ "clear": true })),
            ("events", "events", json!({})),
            ("events --after 5 --wait 30 --limit 2", "events", json!({ "after": 5, "wait": 30, "limit": 2 })),
            ("notes", "notes", json!({})),
            ("notes --all", "notes", json!({ "all": true })),
            ("notes x/a.html", "notes", json!({ "page": "x/a.html" })),
            ("notes /abs/a.html", "notes", json!({ "page": "/abs/a.html" })),
            ("notes reply n1 Hola", "notes_reply", json!({ "id": "n1", "text": "Hola" })),
        ];
        for (line, tool, arguments) in table {
            assert_eq!(unresolved(cli(line).unwrap()), unresolved(mcp(tool, arguments).unwrap()), "{line}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn wire_survives_paths_that_are_not_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff.html".to_vec()));
        let command = Command::Open { target: Target::Path(Page { raw: "x".into(), path: Some(path) }), place: None, loose: false };
        let wire: Value = serde_json::from_str(&command.to_wire()).unwrap();
        assert_eq!(wire["path"], "/tmp/\u{fffd}.html");
    }

    /// The JSON the socket has always carried, from the CLI and from MCP.
    #[test]
    fn socket_format() {
        let wire = |command: Command| serde_json::from_str::<Value>(&command.to_wire()).unwrap();
        assert_eq!(wire(Command::Ping), json!({ "cmd": "ping" }));
        assert_eq!(wire(cli("status").unwrap()), json!({ "cmd": "status" }));
        assert_eq!(wire(cli("reload").unwrap()), json!({ "cmd": "reload" }));
        assert_eq!(wire(cli("update").unwrap()), json!({ "cmd": "update" }));
        assert_eq!(wire(cli("close").unwrap()), json!({ "cmd": "close" }));
        assert_eq!(
            wire(cli("close x/a.html").unwrap()),
            json!({ "cmd": "close", "path": work("x/a.html"), "raw": "x/a.html" })
        );
        assert_eq!(
            wire(mcp("close", json!({ "target": "/abs/a.html" })).unwrap()),
            json!({ "cmd": "close", "path": "/abs/a.html", "raw": "/abs/a.html" })
        );
        assert_eq!(
            wire(cli("open x/a.html").unwrap()),
            json!({ "cmd": "open", "path": work("x/a.html"), "raw": "x/a.html", "place": null })
        );
        assert_eq!(
            wire(cli("open x --beside").unwrap()),
            json!({ "cmd": "open", "path": work("x"), "raw": "x", "place": "beside" })
        );
        assert_eq!(
            wire(cli("open https://a.b --tab").unwrap()),
            json!({ "cmd": "open", "url": "https://a.b", "place": "tab" })
        );
        assert_eq!(
            wire(mcp("open", json!({ "target": "x/a.html", "place": "here" })).unwrap()),
            json!({ "cmd": "open", "path": "", "raw": "x/a.html", "place": "here" })
        );
        assert_eq!(
            wire(mcp("open", json!({ "target": "/abs/a.html" })).unwrap()),
            json!({ "cmd": "open", "path": "/abs/a.html", "raw": "/abs/a.html", "place": null })
        );
        assert_eq!(wire(cli("console").unwrap()), json!({ "cmd": "console", "level": null, "clear": false }));
        assert_eq!(
            wire(cli("console --errors --clear").unwrap()),
            json!({ "cmd": "console", "level": "error", "clear": true })
        );
        assert_eq!(
            wire(cli("events").unwrap()),
            json!({ "cmd": "events", "after": null, "wait": null, "limit": null })
        );
        assert_eq!(
            wire(cli("events --after 3 --wait 300 --limit 5").unwrap()),
            json!({ "cmd": "events", "after": 3, "wait": 300.0, "limit": 5 })
        );
        assert_eq!(wire(cli("notes").unwrap()), json!({ "cmd": "notes" }));
        assert_eq!(wire(cli("notes --all").unwrap()), json!({ "cmd": "notes", "all": true }));
        assert_eq!(
            wire(cli("notes a.html").unwrap()),
            json!({ "cmd": "notes", "path": work("a.html"), "raw": "a.html" })
        );
        assert_eq!(
            wire(mcp("notes", json!({ "page": "a.html" })).unwrap()),
            json!({ "cmd": "notes", "path": "", "raw": "a.html" })
        );
        assert_eq!(
            wire(cli("notes reply n1 Hola a todos").unwrap()),
            json!({ "cmd": "notes_reply", "id": "n1", "text": "Hola a todos" })
        );
    }

    #[test]
    fn screenshot_and_eval_go_through_the_socket() {
        let text = |command: Command| command.to_wire();
        let shot = cli("screenshot x.png").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&text(shot.clone())).unwrap(),
            json!({ "cmd": "screenshot", "path": work("x.png"), "raw": "x.png" })
        );
        assert_eq!(Command::from_wire(&serde_json::from_str(&text(shot.clone())).unwrap()).unwrap(), shot);
        let eval = cli("eval 1 + 1").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&text(eval.clone())).unwrap(), json!({ "cmd": "eval", "code": "1 + 1" }));
        assert_eq!(Command::from_wire(&serde_json::from_str(&text(eval.clone())).unwrap()).unwrap(), eval);
    }

    /// Requests from an older build, still readable.
    #[test]
    fn reads_older_requests() {
        let read = |request: Value| Command::from_wire(&request);
        assert_eq!(
            read(json!({ "cmd": "open", "path": "", "raw": "a.html", "place": "here" })).unwrap(),
            Command::Open { target: Target::Path(page("a.html", None)), place: Some(OpenPlace::Here), loose: false }
        );
        assert_eq!(
            read(json!({ "cmd": "open", "url": "https://a.b", "place": null })).unwrap(),
            Command::Open { target: Target::Url { url: "https://a.b".into() }, place: None, loose: false }
        );
        assert_eq!(read(json!({ "cmd": "close" })).unwrap(), Command::Close { target: None });
        assert_eq!(
            read(json!({ "cmd": "close", "path": "", "raw": "a.html" })).unwrap(),
            Command::Close { target: Some(page("a.html", None)) }
        );
        assert_eq!(
            read(json!({ "cmd": "console", "level": "error", "clear": false })).unwrap(),
            Command::Console { level: Some(Level::Error), clear: false }
        );
        assert_eq!(
            read(json!({ "cmd": "events", "after": null, "wait": 300.0, "limit": null })).unwrap(),
            Command::Events { after: None, wait: Some(300.0), limit: None }
        );
        assert_eq!(read(json!({ "cmd": "events" })).unwrap(), Command::Events { after: None, wait: None, limit: None });
        assert_eq!(read(json!({ "cmd": "notes" })).unwrap(), Command::Notes { scope: NotesScope::Current });
        assert_eq!(
            read(json!({ "cmd": "notes", "all": true })).unwrap(),
            Command::Notes { scope: NotesScope::All }
        );
        assert_eq!(
            read(json!({ "cmd": "notes", "path": "", "raw": "a.html" })).unwrap(),
            Command::Notes { scope: NotesScope::Page(page("a.html", None)) }
        );
        assert_eq!(
            read(json!({ "cmd": "notes_reply", "id": "n", "text": "t" })).unwrap(),
            Command::NotesReply { id: "n".into(), text: "t".into() }
        );
    }

    #[test]
    fn wire_errors() {
        assert_eq!(Command::from_wire(&json!({ "cmd": "volar" })).unwrap_err(), "Comando desconocido: volar");
        assert_eq!(Command::from_wire(&json!({})).unwrap_err(), "Comando desconocido: ");
        // guide is never a socket order.
        assert_eq!(Command::from_wire(&json!({ "cmd": "guide" })).unwrap_err(), "Comando desconocido: guide");
        assert!(Command::from_wire(&json!({ "cmd": "notes_reply" })).unwrap_err().starts_with("Petición no válida"));
    }

    fn windowed(command: Command, window: u64) -> Request {
        Request { command, window: Some(window) }
    }

    #[test]
    fn window_goes_before_or_after_the_name() {
        let status = Request::from_args(&args("--window 2 status")).unwrap();
        assert_eq!(status, windowed(Command::Status, 2));
        assert_eq!(Request::from_args(&args("status --window 2")).unwrap(), status);
        assert_eq!(Request::from_args(&args("status")).unwrap(), Request { command: Command::Status, window: None });
    }

    #[test]
    fn eval_keeps_window_text_in_its_code() {
        let request = Request::from_args(&args("eval --window 3")).unwrap();
        assert_eq!(request, Request { command: Command::Eval { code: "--window 3".into() }, window: None });
    }

    #[test]
    fn window_must_be_a_positive_number_once() {
        let missing = "--window necesita un número de ventana.";
        assert_eq!(Request::from_args(&args("--window")).unwrap_err(), missing);
        assert_eq!(Request::from_args(&args("status --window")).unwrap_err(), missing);
        assert_eq!(Request::from_args(&args("--window 0 status")).unwrap_err(), missing);
        assert_eq!(Request::from_args(&args("--window x status")).unwrap_err(), missing);
        assert_eq!(Request::from_args(&args("--window -1 status")).unwrap_err(), missing);
        assert_eq!(
            Request::from_args(&args("--window 2 status --window 3")).unwrap_err(),
            "--window solo puede aparecer una vez."
        );
    }

    #[test]
    fn windowless_orders_refuse_a_window() {
        assert_eq!(Request::from_args(&args("--window 2 events")).unwrap_err(), "events no va a una ventana: no acepta --window.");
        assert_eq!(Request::from_args(&args("update --window 2")).unwrap_err(), "update no va a una ventana: no acepta --window.");
        assert!(Request::from_args(&args("events")).is_ok());
        assert_eq!(Request::from_mcp("events", &json!({ "window": 2 })), Err("Argumento desconocido: window.".into()));
        assert_eq!(Request::from_mcp("update", &json!({ "window": 2 })), Err("Argumento desconocido: window.".into()));
    }

    #[test]
    fn new_window_is_not_a_command_line_order() {
        assert!(Request::from_args(&args("new_window")).is_err());
        assert!(Request::from_args(&args("--window 2 new_window")).is_err());
    }

    #[test]
    fn mcp_window_argument() {
        let open = Request::from_mcp("open", &json!({ "target": "/a.html", "window": 3 })).unwrap();
        assert_eq!(open.window, Some(3));
        assert_eq!(open.command, Command::from_mcp("open", &json!({ "target": "/a.html" })).unwrap());
        assert_eq!(Request::from_mcp("status", &json!({})).unwrap(), Request { command: Command::Status, window: None });
        assert_eq!(Request::from_mcp("status", &json!({ "window": 0 })).unwrap_err(), "El argumento window no es válido.");
        assert_eq!(Request::from_mcp("status", &json!({ "window": "2" })).unwrap_err(), "El argumento window no es válido.");
        // guide has no window: its own check refuses the argument.
        assert_eq!(Request::from_mcp("guide", &json!({ "window": 2 })).unwrap_err(), "Argumento desconocido: window.");
    }

    #[test]
    fn wire_carries_the_window_only_when_set() {
        let with = windowed(Command::Status, 2);
        let line: Value = serde_json::from_str(&with.to_wire()).unwrap();
        assert_eq!(line, json!({ "cmd": "status", "window": 2 }));
        assert_eq!(Request::from_wire(&line).unwrap(), with);

        let without: Value = serde_json::from_str(&Request { command: Command::Status, window: None }.to_wire()).unwrap();
        assert_eq!(without, json!({ "cmd": "status" }));
    }

    #[test]
    fn wire_window_must_be_a_positive_integer_or_null() {
        let read = |window: Value| Request::from_wire(&json!({ "cmd": "status", "window": window }));
        assert_eq!(read(json!(null)).unwrap(), Request { command: Command::Status, window: None });
        assert_eq!(read(json!(0)).unwrap_err(), "Petición no válida: el campo window no es válido.");
        assert_eq!(read(json!("2")).unwrap_err(), "Petición no válida: el campo window no es válido.");
        assert_eq!(read(json!(-1)).unwrap_err(), "Petición no válida: el campo window no es válido.");
    }

    #[test]
    fn new_window_is_wire_only() {
        assert_eq!(
            Command::from_wire(&json!({ "cmd": "new_window" })).unwrap(),
            Command::NewWindow { token: None }
        );
        assert_eq!(
            Command::from_wire(&json!({ "cmd": "new_window", "token": "abc" })).unwrap(),
            Command::NewWindow { token: Some("abc".into()) }
        );
        assert_eq!(
            Command::from_wire(&json!({ "cmd": "new_window", "token": "" })).unwrap(),
            Command::NewWindow { token: None }
        );
        assert!(Command::from_wire(&json!({ "cmd": "new_window", "token": 4 })).is_err());

        let line: Value = serde_json::from_str(&Request { command: Command::NewWindow { token: Some("t".into()) }, window: None }.to_wire()).unwrap();
        assert_eq!(line, json!({ "cmd": "new_window", "token": "t" }));
    }
}
