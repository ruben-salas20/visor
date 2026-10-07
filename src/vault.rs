use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');

#[derive(Serialize)]
pub struct Dir {
    name: String,
    path: String,
    dirs: Vec<Dir>,
    files: Vec<File>,
}

#[derive(Serialize)]
pub struct File {
    name: String,
    path: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub vaults: Vec<PathBuf>,
    pub current: Option<PathBuf>,
    /// Last file shown in each vault, from before tabs; read when a vault has no workspace.
    pub last: HashMap<PathBuf, String>,
    /// Open tabs and split of each vault.
    pub workspaces: HashMap<PathBuf, Workspace>,
    pub sidebar_width: Option<i32>,
    /// Where the divider between the two sides sits, as a share of their width.
    pub split_ratio: Option<f64>,
    /// Port of the local server, kept so pages keep their origin and storage.
    pub port: Option<u16>,
    pub bar_shown: bool,
    pub zoom: Option<f64>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Workspace {
    /// One entry per side, left first.
    pub panes: Vec<PaneState>,
    pub focused: usize,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct PaneState {
    /// What each tab shows: a vault path, a web address, or empty.
    pub tabs: Vec<String>,
    pub active: usize,
}

impl Config {
    pub fn load() -> Self {
        fs::read(crate::paths::config())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = crate::paths::config();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, serde_json::to_vec_pretty(self).unwrap());
    }

    pub fn add(&mut self, vault: &Path) {
        if !self.vaults.iter().any(|v| v == vault) {
            self.vaults.push(vault.to_path_buf());
        }
        self.current = Some(vault.to_path_buf());
    }

    /// The vault that holds `file`, preferring the innermost one.
    pub fn vault_for(&self, file: &Path) -> Option<PathBuf> {
        self.vaults
            .iter()
            .filter(|v| file.starts_with(v))
            .max_by_key(|v| v.components().count())
            .cloned()
    }
}

/// Files the viewer lists and shows: HTML pages and Markdown notes.
pub fn is_page(path: &Path) -> bool {
    is_html(path) || is_markdown(path)
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

pub fn is_html(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("html" | "htm")
    )
}

fn hidden(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules"
}

/// True if a change at `path` should not affect what the viewer shows.
pub fn ignored(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).map_or(true, |rel| {
        rel.components().any(|c| hidden(&c.as_os_str().to_string_lossy()))
    })
}

pub fn tree(root: &Path) -> Dir {
    walk(root, root).unwrap_or_else(|| Dir {
        name: name_of(root),
        path: String::new(),
        dirs: vec![],
        files: vec![],
    })
}

fn walk(root: &Path, dir: &Path) -> Option<Dir> {
    let mut dirs = vec![];
    let mut files = vec![];
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if hidden(&name) {
            continue;
        }
        if path.is_dir() {
            if let Some(sub) = walk(root, &path) {
                dirs.push(sub);
            }
        } else if is_page(&path) {
            files.push(File {
                name: path.file_stem().unwrap().to_string_lossy().into_owned(),
                path: rel(root, &path),
            });
        }
    }
    dirs.sort_by(|a, b| natural(&a.name, &b.name));
    files.sort_by(|a, b| natural(&a.name, &b.name));
    Some(Dir {
        name: name_of(dir),
        path: rel(root, dir),
        dirs,
        files,
    })
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Orders "tema 2" before "tema 10", ignoring case.
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek(), b.peek()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut n = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        n.push(c);
                    }
                    n.trim_start_matches('0').to_string()
                };
                let (x, y) = (take(&mut a), take(&mut b));
                let ord = x.len().cmp(&y.len()).then_with(|| x.cmp(&y));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(_), Some(_)) => {
                let x = a.next().unwrap().to_lowercase().collect::<String>();
                let y = b.next().unwrap().to_lowercase().collect::<String>();
                if x != y {
                    return x.cmp(&y);
                }
            }
        }
    }
}

/// Resolves a request path inside the vault, refusing anything that escapes it.
pub fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let path = root.join(rel.trim_start_matches('/')).canonicalize().ok()?;
    let path = if path.is_dir() { path.join("index.html") } else { path };
    (path.starts_with(&root) && path.is_file()).then_some(path)
}

pub fn mime(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        "txt" | "md" | "csv" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The bytes asked for by an HTTP `Range` header, as `(start, end)` inclusive.
/// Players use ranges to stream and seek; without them local video fails.
pub fn byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let (start, end) = match (start.trim(), end.trim()) {
        ("", suffix) => (len.saturating_sub(suffix.parse().ok()?), len.checked_sub(1)?),
        (s, "") => (s.parse().ok()?, len.checked_sub(1)?),
        (s, e) => (s.parse().ok()?, e.parse::<u64>().ok()?.min(len.checked_sub(1)?)),
    };
    (start <= end && start < len).then_some((start, end))
}

static BASE: OnceLock<String> = OnceLock::new();

/// Sets the URL the local server serves the vault from. Called once at startup.
pub fn set_base(base: String) {
    let _ = BASE.set(base);
}

/// URL prefix of every vault page, e.g. `http://127.0.0.1:PORT/SECRET/`.
pub fn base() -> &'static str {
    BASE.get().map_or("", String::as_str)
}

pub fn url(rel: &str) -> String {
    let path: Vec<String> = rel.split('/').map(|seg| utf8_percent_encode(seg, SEGMENT).to_string()).collect();
    format!("{}{}", base(), path.join("/"))
}

/// Inverse of [`url`]; `None` for pages outside the vault.
pub fn rel_from_url(url: &str) -> Option<String> {
    let path = url.strip_prefix(base()).filter(|_| !base().is_empty())?;
    let path = path.split(['?', '#']).next()?;
    Some(percent_encoding::percent_decode_str(path).decode_utf8_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["tema 10", "Tema 2", "tema 1", "anexo"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["anexo", "tema 1", "Tema 2", "tema 10"]);
    }

    #[test]
    fn byte_ranges() {
        assert_eq!(byte_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(byte_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(byte_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(byte_range("bytes=0-", 1000), Some((0, 999)));
        assert_eq!(byte_range("bytes=2000-", 1000), None);
        assert_eq!(byte_range("bytes=0-", 0), None);
    }

    #[test]
    fn url_round_trip() {
        set_base("http://127.0.0.1:1/s/".into());
        let rel = "física/tema 2/¿qué?#1.html";
        let url = url(rel);
        assert_eq!(url, "http://127.0.0.1:1/s/f%C3%ADsica/tema%202/%C2%BFqu%C3%A9%3F%231.html");
        assert_eq!(rel_from_url(&url).as_deref(), Some(rel));
        assert_eq!(rel_from_url("https://example.com/"), None);
    }
}
