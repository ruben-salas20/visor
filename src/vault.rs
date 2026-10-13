//! The open vault and what lives in it: its configuration, notes, annotations
//! and links. Every path a page or the panel names is checked here.
pub mod annot;
pub mod config;
pub mod drawing;
pub mod files;
pub(crate) mod front_matter;
pub mod links;
pub mod markdown;
#[cfg(test)]
mod walk_tests;

use crate::i18n::tr;
use crate::trf;
use serde::Serialize;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');

#[derive(Serialize)]
pub struct Dir {
    pub name: String,
    pub path: String,
    pub dirs: Vec<Dir>,
    pub files: Vec<File>,
}

#[derive(Serialize)]
pub struct File {
    /// The stem for pages, the whole file name for the rest.
    pub name: String,
    pub path: String,
    pub kind: Kind,
}

/// What the panel makes of a file: a page the viewer shows, an image it can
/// show too, or one it cannot show.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Page,
    Image,
    Other,
}

/// Files the viewer lists and shows: HTML pages, Markdown notes and drawings.
pub fn is_page(path: &Path) -> bool {
    is_html(path) || is_markdown(path) || is_drawing(path)
}

/// An image the viewer can show: the formats browsers display.
pub fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif")
    )
}

/// A Markdown note. A drawing in Obsidian's form is Markdown too, but it is not a note.
pub fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) && !is_drawing(path)
}

/// A drawing: `.excalidraw.md` (Obsidian's Excalidraw plugin) or `.excalidraw` (JSON of excalidraw.com).
pub fn is_drawing(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    name.ends_with(".excalidraw.md") || name.ends_with(".excalidraw")
}

pub fn is_html(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("html" | "htm")
    )
}

/// A page opened on its own, outside every vault, is kept as its absolute
/// path; vault paths never start with `/`.
pub fn is_loose(target: &str) -> bool {
    target.starts_with('/')
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
    let mut walk = Walk::new(root, root);
    scan(&mut walk, root, root).unwrap_or_else(|| Dir {
        name: name_of(root),
        path: String::new(),
        dirs: vec![],
        files: vec![],
    })
}

fn scan(walk: &mut Walk, root: &Path, dir: &Path) -> Option<Dir> {
    let mut dirs = vec![];
    let mut files = vec![];
    for (path, is_dir) in walk.read(dir)? {
        if is_dir {
            if let Some(sub) = scan(walk, root, &path) {
                dirs.push(sub);
            }
        } else {
            let kind = if is_page(&path) {
                Kind::Page
            } else if is_image(&path) {
                Kind::Image
            } else {
                Kind::Other
            };
            let stem = if kind == Kind::Page { path.file_stem() } else { path.file_name() };
            files.push(File {
                name: stem.unwrap_or_default().to_string_lossy().into_owned(),
                path: rel(root, &path),
                kind,
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

/// The one way to walk a folder tree, so the panel, the `[[links]]` index and
/// the annotation listing agree on what is there: names that `hidden` skips
/// are not entered, a real folder is entered once whatever links point at it,
/// and a link to a folder outside `limit` is not followed.
pub struct Walk {
    limit: PathBuf,
    seen: HashSet<(u64, u64)>,
}

impl Walk {
    /// Walks from `start`, inside the vault at `root`.
    pub fn new(root: &Path, start: &Path) -> Self {
        let limit = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let seen = fs::metadata(start).map(|m| (m.dev(), m.ino())).into_iter().collect();
        Walk { limit, seen }
    }

    /// The visible entries of `dir` with whether each is a folder to enter, or
    /// `None` if it cannot be read. Real folders come before links so that
    /// the real one is the one listed when a link points at a sibling.
    pub fn read(&mut self, dir: &Path) -> Option<Vec<(PathBuf, bool)>> {
        let mut real = vec![];
        let mut linked = vec![];
        for entry in fs::read_dir(dir).ok()?.flatten() {
            if hidden(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                linked.push(path);
            } else {
                real.push((path, kind.is_dir()));
            }
        }
        let mut out = vec![];
        for (path, is_dir) in real {
            if !is_dir || self.enter(&path) {
                out.push((path, is_dir));
            }
        }
        for path in linked {
            match fs::metadata(&path) {
                Ok(m) if m.is_dir() => {
                    let inside = path.canonicalize().is_ok_and(|p| p.starts_with(&self.limit));
                    if inside && self.enter(&path) {
                        out.push((path, true));
                    }
                }
                _ => out.push((path, false)),
            }
        }
        Some(out)
    }

    /// False if this real folder was already entered.
    fn enter(&mut self, dir: &Path) -> bool {
        fs::metadata(dir).is_ok_and(|m| self.seen.insert((m.dev(), m.ino())))
    }
}

/// Every file under `from` that `keep` accepts, walked by [`Walk`] within the vault at `root`.
pub fn files(root: &Path, from: &Path, keep: fn(&Path) -> bool) -> Vec<PathBuf> {
    fn go(walk: &mut Walk, dir: &Path, keep: fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
        for (path, is_dir) in walk.read(dir).unwrap_or_default() {
            if is_dir {
                go(walk, &path, keep, out);
            } else if keep(&path) {
                out.push(path);
            }
        }
    }
    let mut out = vec![];
    go(&mut Walk::new(root, from), from, keep, &mut out);
    out
}

/// Whether the vault at `root` has any page; stops at the first one.
pub fn has_pages(root: &Path) -> bool {
    fn go(walk: &mut Walk, dir: &Path) -> bool {
        let Some(entries) = walk.read(dir) else { return false };
        entries.iter().any(|(path, is_dir)| if *is_dir { go(walk, path) } else { is_page(path) })
    }
    go(&mut Walk::new(root, root), root)
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

/// The NFC form of `s`. macOS and zip files often store accents decomposed
/// (NFD: "a" + U+0301), while names typed by people and agents are composed.
pub fn nfc(s: &str) -> Cow<'_, str> {
    icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(s)
}

/// `rel` under `base`, as it is spelled on disk. A name is taken as written when
/// it exists; otherwise the folder is read and the entry whose NFC form matches
/// is used. The result is not canonical: callers still canonicalize it and check
/// that it stays inside the vault, so this never widens what can be reached.
pub fn find_nfc(base: &Path, rel: &Path) -> Option<PathBuf> {
    let mut cur = base.to_path_buf();
    for part in rel.components() {
        match part {
            Component::Normal(name) => {
                let name = if cur.join(name).exists() { name.to_os_string() } else { nfc_entry(&cur, name)? };
                cur.push(name);
            }
            Component::CurDir => {}
            other => cur.push(other.as_os_str()),
        }
    }
    Some(cur)
}

/// The entry of `dir` whose NFC name equals that of `name`, the smallest if there are several.
fn nfc_entry(dir: &Path, name: &OsStr) -> Option<OsString> {
    let want = nfc(name.to_str()?);
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.file_name())
        .filter(|f| f.to_str().is_some_and(|f| nfc(f) == want))
        .min()
}

/// `path` with `.` and `..` resolved as text, without touching the disk. A
/// deleted file has no canonical path, but its name still says where its tab is.
pub fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // Above the root there is nothing to go to; a relative path keeps its `..`.
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other),
        }
    }
    out
}

/// A page's name in the tabs: relative to the open vault `root`, or its own path
/// when it is a loose .html that no known vault holds. None for anything else.
pub fn tab_name(path: &Path, root: Option<&Path>, is_known: impl Fn(&Path) -> bool) -> Option<String> {
    match root.filter(|r| path.starts_with(r)) {
        Some(root) => Some(rel(root, path)),
        None if is_html(path) && !is_known(path) => Some(path.to_string_lossy().into_owned()),
        None => None,
    }
}

/// The names a path given by an agent may have in the tabs, most likely first.
/// `canonical` is the path as the disk resolves it, when the file exists. `raw`
/// is what the agent wrote, which may be the name `status` shows.
pub fn tab_names(
    path: &Path,
    canonical: Option<&Path>,
    raw: &str,
    root: Option<&Path>,
    is_known: impl Fn(&Path) -> bool,
) -> Vec<String> {
    let lexical = lexical(path);
    let mut names: Vec<String> = canonical
        .into_iter()
        .chain(std::iter::once(lexical.as_path()))
        .filter_map(|p| tab_name(p, root, &is_known))
        .collect();
    names.push(raw.trim_start_matches("./").to_string());
    names.retain(|n| !n.is_empty());
    names.dedup();
    names
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

/// The open vault. Every path that comes from a page, the panel or the CLI is
/// checked here, so nothing outside the folder is ever read or written.
pub struct Vault {
    /// As it was opened: the key in the configuration and what the file watcher reports under.
    path: PathBuf,
    /// Canonical, computed once.
    root: PathBuf,
}

impl Vault {
    /// If the folder cannot be read yet, the root stays as given and nothing resolves in it.
    pub fn open(path: &Path) -> Self {
        let root = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Vault { path: path.to_path_buf(), root }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn tree(&self) -> Dir {
        tree(&self.path)
    }

    pub fn has_pages(&self) -> bool {
        has_pages(&self.path)
    }

    /// Path of `path` inside the vault, with `/` separators.
    pub fn rel(&self, path: &Path) -> String {
        rel(&self.root, path)
    }

    /// Resolves a request path inside the vault, refusing anything that escapes it.
    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        self.locate(rel).map(|(_, path)| path)
    }

    /// Like [`Vault::resolve`], also giving `rel` spelled as on disk, which is
    /// what the page is listed under.
    fn locate(&self, rel: &str) -> Option<(String, PathBuf)> {
        let rel = rel.trim_start_matches('/');
        let (rel, path) = match self.root.join(rel).canonicalize() {
            Ok(path) => (rel.to_string(), path),
            Err(_) => {
                let real = find_nfc(&self.root, Path::new(rel))?;
                (self.rel(&real), real.canonicalize().ok()?)
            }
        };
        let path = if path.is_dir() { path.join("index.html") } else { path };
        // Checked on the real path, so a link into a hidden folder is refused too.
        let hidden = self.rel(&path).split('/').any(|seg| seg.starts_with('.'));
        (path.starts_with(&self.root) && path.is_file() && !hidden).then_some((rel, path))
    }

    /// The file of the page shown at `url`, with its vault path. Websites and
    /// loose pages are not in the vault, so they are refused.
    pub fn page(&self, url: &str) -> Result<(String, PathBuf), String> {
        // The name keys the page's annotations, so it must be the one the file
        // is listed under: `a/../b` would resolve, but store them elsewhere.
        let rel = rel_from_url(url)
            .filter(|r| !is_loose(r) && !r.split('/').any(|seg| seg == "." || seg == ".."))
            .ok_or(tr("La página no es de la bóveda", "The page is not in the vault"))?;
        let (rel, path) = self.locate(&rel).ok_or(tr("La página no existe en la bóveda", "The page does not exist in the vault"))?;
        Ok((rel, path))
    }

    /// Like [`Vault::page`], but only Markdown notes.
    pub fn note(&self, url: &str) -> Result<(String, PathBuf), String> {
        let (rel, path) = self.page(url)?;
        if is_markdown(&path) { Ok((rel, path)) } else { Err(tr("Solo se pueden guardar notas .md", "Only .md notes can be saved").into()) }
    }

    /// Like [`Vault::page`], but only drawings.
    pub fn drawing(&self, url: &str) -> Result<(String, PathBuf), String> {
        let (rel, path) = self.page(url)?;
        if is_drawing(&path) { Ok((rel, path)) } else { Err(tr("Solo se pueden guardar dibujos", "Only drawings can be saved").into()) }
    }

    /// A file or folder of the vault named by the panel, never the vault itself
    /// nor anything outside it or hidden.
    pub fn entry(&self, rel: &str) -> Result<PathBuf, String> {
        if rel.is_empty() || rel.split('/').any(|seg| seg.starts_with('.') || seg.is_empty()) {
            return Err(trf!("Ruta no válida: {rel}", "Invalid path: {rel}"));
        }
        // The parent is canonicalized, not the item, so a link is moved and not its target.
        let path = self.root.join(rel);
        let parent = path.parent().and_then(|p| p.canonicalize().ok()).ok_or_else(|| trf!("No existe: {rel}", "Does not exist: {rel}"))?;
        let path = parent.join(path.file_name().ok_or(tr("Ruta no válida", "Invalid path"))?);
        if !parent.starts_with(&self.root) || fs::symlink_metadata(&path).is_err() {
            return Err(trf!("No existe: {rel}", "Does not exist: {rel}"));
        }
        Ok(path)
    }

    /// A folder of the vault; the empty path is the vault itself.
    pub fn folder(&self, rel: &str) -> Result<PathBuf, String> {
        let dir = if rel.is_empty() { self.root.clone() } else { self.entry(rel)? };
        if dir.is_dir() { Ok(dir) } else { Err(trf!("No es una carpeta: {rel}", "Not a folder: {rel}")) }
    }

    /// The vault folder `dir` and a checked `name` for something new inside it.
    pub fn new_in<'a>(&self, dir: &str, name: &'a str) -> Result<(PathBuf, &'a str), String> {
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            return Err(tr(
                "El nombre no puede estar vacío, empezar por punto ni contener «/».",
                "The name cannot be empty, start with a dot or contain «/».",
            )
            .into());
        }
        let folder = if dir.is_empty() {
            self.root.clone()
        } else {
            let folder = self.root.join(dir).canonicalize().map_err(|_| trf!("No existe la carpeta {dir}", "The folder {dir} does not exist"))?;
            if !folder.starts_with(&self.root) {
                return Err(tr("La carpeta está fuera de la bóveda.", "The folder is outside the vault.").into());
            }
            folder
        };
        Ok((folder, name))
    }
}

/// What a page may do, decided by the address of the document it shows: the
/// one it was loaded from, never one it gave itself afterwards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// A website or anything else not served by visor.
    None,
    /// Served by visor but outside every vault: console and events only.
    Local,
    /// A page of the vault: it may also write to it.
    Trusted,
}

pub fn access(url: &str) -> Access {
    match rel_from_url(url) {
        None => Access::None,
        Some(rel) if is_loose(&rel) => Access::Local,
        Some(_) => Access::Trusted,
    }
}

/// Whether agents may read a page's content: run code in it or photograph it.
/// Only pages visor serves. What a website shows would reach an agent's context.
pub fn agent_may_read(url: &str) -> bool {
    access(url) != Access::None
}

/// Writes through a temporary file, so a crash never leaves a half-written file.
/// A link is written through to its target, which keeps its permissions.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let real;
    let path = if fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink()) {
        real = path.canonicalize()?;
        &real
    } else {
        path
    };
    let name = path.file_name().map_or("nota".into(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.visor-tmp"));
    fs::write(&tmp, contents)?;
    let done = match fs::metadata(path) {
        Ok(old) => fs::set_permissions(&tmp, old.permissions()),
        Err(_) => Ok(()),
    }
    .and_then(|()| fs::rename(&tmp, path));
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    done
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
static LOOSE_BASE: OnceLock<String> = OnceLock::new();

/// Sets the URL the local server serves the vault from. Called once at startup.
pub fn set_base(base: String) {
    let _ = BASE.set(base);
}

/// Sets the URL loose pages are served from. It is another origin, so a loose
/// page can neither read the vault nor take the address of one of its pages.
pub fn set_loose_base(base: String) {
    let _ = LOOSE_BASE.set(base);
}

/// URL prefix of every vault page, e.g. `http://127.0.0.1:PORT/SECRET/`.
pub fn base() -> &'static str {
    BASE.get().map_or("", String::as_str)
}

/// URL prefix of loose pages, e.g. `http://127.0.0.2:PORT/SECRET/`.
pub fn loose_base() -> &'static str {
    LOOSE_BASE.get().map_or("", String::as_str)
}

/// Also takes a loose page's absolute path, served at `loose_base()` + `/` + path.
pub fn url(rel: &str) -> String {
    let path: Vec<String> = rel.split('/').map(|seg| utf8_percent_encode(seg, SEGMENT).to_string()).collect();
    let base = if is_loose(rel) { loose_base() } else { base() };
    format!("{base}{}", path.join("/"))
}

/// Inverse of [`url`]; `None` for pages the server does not serve, and for a
/// path asked of the wrong origin.
pub fn rel_from_url(url: &str) -> Option<String> {
    let decode = |base: &str| {
        let path = url.strip_prefix(base).filter(|_| !base.is_empty())?;
        let path = path.split(['?', '#']).next()?;
        Some(percent_encoding::percent_decode_str(path).decode_utf8_lossy().into_owned())
    };
    decode(base()).filter(|rel| !is_loose(rel)).or_else(|| decode(loose_base()).filter(|rel| is_loose(rel)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_resolves_dots_without_the_disk() {
        assert_eq!(lexical(Path::new("/v/a/../b.html")), Path::new("/v/b.html"));
        assert_eq!(lexical(Path::new("/v/./x/./y.html")), Path::new("/v/x/y.html"));
        assert_eq!(lexical(Path::new("/../x.html")), Path::new("/x.html"));
        assert_eq!(lexical(Path::new("a/../../b")), Path::new("../b"));
    }

    #[test]
    fn tab_names_of_a_deleted_page_inside_the_vault() {
        // Deleted: no canonical path. The tab is still named by its vault-relative path.
        let names = tab_names(Path::new("/v/notas/../notas/a.html"), None, "x", Some(Path::new("/v")), |_| false);
        assert_eq!(names, ["notas/a.html", "x"]);
    }

    #[test]
    fn tab_names_of_a_loose_page() {
        let names = tab_names(Path::new("/home/u/s.html"), None, "s.html", Some(Path::new("/v")), |_| false);
        assert_eq!(names, ["/home/u/s.html", "s.html"]);
        // A page of a known vault that is not open is not a loose tab.
        let names = tab_names(Path::new("/home/u/s.html"), None, "", Some(Path::new("/v")), |_| true);
        assert!(names.is_empty());
    }

    #[test]
    fn tab_names_of_a_raw_relative_name() {
        // Nothing on disk and not in the open vault: only the name as written can match.
        let names = tab_names(Path::new("/elsewhere/x"), None, "./guia/a.md", Some(Path::new("/v")), |_| false);
        assert_eq!(names, ["guia/a.md"]);
        // A lone "./" would name the start page, which must never be closed by name.
        assert!(tab_names(Path::new("/elsewhere/x"), None, "./", None, |_| false).is_empty());
    }

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
        bases();
        let rel = "física/tema 2/¿qué?#1.html";
        let url = url(rel);
        assert_eq!(url, "http://127.0.0.1:1/s/f%C3%ADsica/tema%202/%C2%BFqu%C3%A9%3F%231.html");
        assert_eq!(rel_from_url(&url).as_deref(), Some(rel));
        assert_eq!(rel_from_url("https://example.com/"), None);
        let loose = "/tmp/muestra rápida.html";
        assert_eq!(super::url(loose), "http://127.0.0.2:1/s//tmp/muestra%20r%C3%A1pida.html");
        assert_eq!(rel_from_url(&super::url(loose)).as_deref(), Some(loose));
        // Each origin serves only its own kind of page.
        assert_eq!(rel_from_url("http://127.0.0.1:1/s//tmp/x.html"), None);
        assert_eq!(rel_from_url("http://127.0.0.2:1/s/nota.md"), None);
    }

    const BASE: &str = "http://127.0.0.1:1/s/";
    const LOOSE_BASE: &str = "http://127.0.0.2:1/s/";

    /// The same bases in every test: they can only be set once per process.
    fn bases() {
        set_base(BASE.into());
        set_loose_base(LOOSE_BASE.into());
    }

    /// A vault in a temporary folder, plus a file outside it that links try to reach.
    fn make(name: &str) -> (Vault, PathBuf) {
        use std::os::unix::fs::symlink;
        let dir = std::env::temp_dir().join(format!("visor-vault-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = dir.join("boveda");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join("sitio")).unwrap();
        fs::create_dir_all(root.join(".oculta")).unwrap();
        fs::write(root.join("a/nota.md"), "# Nota").unwrap();
        fs::write(root.join("a/pagina.html"), "<p>").unwrap();
        fs::write(root.join("sitio/index.html"), "<p>").unwrap();
        fs::write(root.join(".oculta/x.md"), "").unwrap();
        fs::write(dir.join("fuera.md"), "secreto").unwrap();
        symlink(dir.join("fuera.md"), root.join("a/enlace.md")).unwrap();
        symlink(&dir, root.join("salida")).unwrap();
        (Vault::open(&root), dir)
    }

    #[test]
    fn resolve_stays_inside() {
        let (vault, dir) = make("resolve");
        assert_eq!(vault.resolve("a/nota.md"), Some(vault.root().join("a/nota.md")));
        assert_eq!(vault.resolve("/a/nota.md"), Some(vault.root().join("a/nota.md")));
        assert_eq!(vault.resolve("sitio"), Some(vault.root().join("sitio/index.html")));
        assert_eq!(vault.resolve("a/falta.md"), None);
        assert_eq!(vault.resolve("a"), None, "folder without index.html");
        assert_eq!(vault.resolve("../fuera.md"), None);
        assert_eq!(vault.resolve("a/../../fuera.md"), None);
        assert_eq!(vault.resolve(dir.join("fuera.md").to_str().unwrap()), None, "absolute path");
        assert_eq!(vault.resolve("a/enlace.md"), None, "link out of the vault");
        assert_eq!(vault.resolve("salida/fuera.md"), None, "folder link out of the vault");
        assert_eq!(vault.resolve(".oculta/x.md"), None, "hidden folder");
        std::os::unix::fs::symlink(vault.root().join(".oculta/x.md"), vault.root().join("a/a-oculta.md")).unwrap();
        assert_eq!(vault.resolve("a/a-oculta.md"), None, "link into a hidden folder");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nfd_names_are_found_by_nfc_and_back() {
        bases();
        let (vault, dir) = make("nfc");
        let root = vault.root().to_path_buf();
        let (nfd, nfc_) = ("Pra\u{301}ctica", "Práctica");
        fs::create_dir_all(root.join(format!("{nfd}/tema 1"))).unwrap();
        fs::write(root.join(format!("{nfd}/tema 1/Ejercicios de {nfd}.html")), "<p>").unwrap();
        fs::write(root.join(format!("{nfd}/nota.md")), "# Nota").unwrap();
        // A link out of the vault with a decomposed name.
        std::os::unix::fs::symlink(&dir, root.join("sa\u{301}lida")).unwrap();
        let file = format!("{nfd}/tema 1/Ejercicios de {nfd}.html");
        let want = root.join(&file);
        assert_eq!(vault.resolve(&file), Some(want.clone()), "as on disk");
        assert_eq!(vault.resolve(&format!("{nfc_}/tema 1/Ejercicios de {nfc_}.html")), Some(want.clone()), "NFC");
        assert_eq!(vault.resolve(&format!("{nfc_}/nota.md")), Some(root.join(format!("{nfd}/nota.md"))), "folder NFC");
        assert_eq!(vault.resolve(&format!("{nfc_}/falta.md")), None);
        // The key of a page is the name on disk, not the one asked for.
        let (rel, _) = vault.page(&url(&format!("{nfc_}/nota.md"))).unwrap();
        assert_eq!(rel, format!("{nfd}/nota.md"));
        // The fallback finds the real folder only; it never leaves the vault.
        assert_eq!(vault.resolve(&format!("{nfc_}/../../fuera.md")), None, "dot segments");
        assert_eq!(vault.resolve("sálida/fuera.md"), None, "link out of the vault, NFC name");
        assert_eq!(vault.resolve("sa\u{301}lida/fuera.md"), None, "link out of the vault, NFD name");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn entry_and_folder() {
        let (vault, dir) = make("entry");
        let root = vault.root().to_path_buf();
        assert_eq!(vault.entry("a/nota.md"), Ok(root.join("a/nota.md")));
        assert_eq!(vault.entry("sitio"), Ok(root.join("sitio")));
        assert_eq!(vault.entry("a/enlace.md"), Ok(root.join("a/enlace.md")), "the link itself, not its target");
        for bad in ["", "../fuera.md", "/a/nota.md", "a//nota.md", ".oculta/x.md", ".visor", "a/.x", "salida/fuera.md"] {
            assert!(vault.entry(bad).is_err(), "{bad:?}");
        }
        assert!(vault.entry("a/falta.md").is_err());
        assert_eq!(vault.folder(""), Ok(root.clone()));
        assert_eq!(vault.folder("a/b"), Ok(root.join("a/b")));
        assert!(vault.folder("a/nota.md").is_err(), "not a folder");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn page_and_note() {
        bases();
        let (vault, dir) = make("page");
        let (rel, path) = vault.page(&url("a/pagina.html")).unwrap();
        assert_eq!((rel.as_str(), path), ("a/pagina.html", vault.root().join("a/pagina.html")));
        assert_eq!(vault.page(&format!("{}#x", url("a/nota.md"))).unwrap().0, "a/nota.md");
        assert_eq!(vault.note(&url("a/nota.md")).unwrap().1, vault.root().join("a/nota.md"));
        assert!(vault.note(&url("a/pagina.html")).is_err(), "html is not a note");
        fs::write(vault.root().join("a/dibujo.excalidraw.md"), "# x").unwrap();
        fs::write(vault.root().join("a/dibujo.excalidraw"), "{}").unwrap();
        assert!(vault.note(&url("a/dibujo.excalidraw.md")).is_err(), "a drawing is not a note");
        assert!(vault.drawing(&url("a/nota.md")).is_err(), "a note is not a drawing");
        assert_eq!(vault.drawing(&url("a/dibujo.excalidraw")).unwrap().1, vault.root().join("a/dibujo.excalidraw"));
        assert_eq!(vault.drawing(&url("a/dibujo.excalidraw.md")).unwrap().1, vault.root().join("a/dibujo.excalidraw.md"));
        assert!(vault.page(&url("a/falta.md")).is_err());
        assert!(vault.note(&url("a/falta.md")).is_err());
        assert!(vault.page("https://example.com/a/nota.md").is_err());
        let loose = dir.join("fuera.md");
        assert!(vault.page(&url(loose.to_str().unwrap())).is_err(), "loose page");
        assert!(vault.note(&url(loose.to_str().unwrap())).is_err(), "loose page");
        assert!(vault.page(&url("a/enlace.md")).is_err(), "link out of the vault");
        assert!(vault.page(&format!("{BASE}a/%2e%2e/a/nota.md")).is_err(), "dot segments");
        assert!(vault.page(&format!("{BASE}a/./nota.md")).is_err(), "dot segments");
        assert!(vault.note(&url(".oculta/x.md")).is_err(), "hidden folder");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn access_by_url() {
        bases();
        assert_eq!(access("https://example.com/"), Access::None);
        assert_eq!(access("about:blank"), Access::None);
        assert_eq!(access(&url("a/nota.md")), Access::Trusted);
        assert_eq!(access(&format!("{}#x", url("a/nota.md"))), Access::Trusted);
        assert_eq!(access(&url("/tmp/suelta.html")), Access::Local);
        assert_eq!(access(&format!("{LOOSE_BASE}nota.md")), Access::None, "a vault path on the loose origin");
        assert_eq!(access(&format!("{BASE}/tmp/suelta.html")), Access::None, "a loose path on the vault origin");
    }

    #[test]
    fn agents_read_only_visor_pages() {
        bases();
        assert!(agent_may_read(&url("a/nota.md")));
        assert!(agent_may_read(&url("/tmp/suelta.html")));
        assert!(!agent_may_read("https://example.com/"));
        assert!(!agent_may_read("about:blank"));
        assert!(!agent_may_read(""));
        assert!(!agent_may_read(&format!("{LOOSE_BASE}nota.md")));
    }

    #[test]
    fn new_in_checks_name_and_folder() {
        let (vault, dir) = make("new");
        let root = vault.root().to_path_buf();
        assert_eq!(vault.new_in("", " x ").unwrap(), (root.clone(), "x"));
        assert_eq!(vault.new_in("a/b", "x").unwrap(), (root.join("a/b"), "x"));
        for name in ["", "  ", ".x", "a/b", "a\\b"] {
            assert!(vault.new_in("", name).is_err(), "{name:?}");
        }
        assert!(vault.new_in("falta", "x").is_err());
        assert_eq!(vault.new_in("..", "x").unwrap_err(), "La carpeta está fuera de la bóveda.");
        assert!(vault.new_in(dir.to_str().unwrap(), "x").is_err(), "absolute folder outside");
        assert!(vault.new_in("salida", "x").is_err(), "link out of the vault");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("visor-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("n.md");
        write_atomic(&file, "uno").unwrap();
        write_atomic(&file, "dos").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "dos");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_atomic_goes_through_links_and_keeps_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir().join(format!("visor-atomic-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("otra")).unwrap();
        let real = dir.join("otra/real.md");
        fs::write(&real, "uno").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.join("enlace.md");
        symlink(&real, &link).unwrap();
        write_atomic(&link, "dos").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink(), "the link stays a link");
        assert_eq!(fs::read_to_string(&real).unwrap(), "dos");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o640);
        assert!(write_atomic(&dir.join("roto.md"), "x").is_ok());
        symlink(dir.join("no-existe"), dir.join("colgante.md")).unwrap();
        assert!(write_atomic(&dir.join("colgante.md"), "x").is_err());
        assert_eq!(fs::read_dir(dir.join("otra")).unwrap().count(), 1, "no temp left");
        fs::remove_dir_all(dir).unwrap();
    }
}
