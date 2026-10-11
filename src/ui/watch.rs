//! File watching: the watchers themselves, and the decision of what a batch
//! of changes means for the pages on screen. See docs/adr/0021.

use crate::ui::action::UserEvent;
use crate::vault::{self, annot};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

/// A batch goes out at the latest this long after its first change, even if
/// changes keep coming, so a process that writes nonstop cannot starve the window.
const MAX_WAIT: Duration = Duration::from_secs(1);
/// And at the latest with this many different paths, so one batch stays small.
const MAX_PATHS: usize = 5000;

/// Watches `path` and forwards changes in batches once they settle.
/// Access events are dropped: serving a page opens it, which would reload it forever.
pub fn watch(
    path: &Path,
    mode: RecursiveMode,
    settle_ms: u64,
    proxy: EventLoopProxy<UserEvent>,
    wrap: fn(Vec<PathBuf>) -> UserEvent,
) -> Option<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && !matches!(event.kind, EventKind::Access(_))
        {
            let _ = tx.send(event.paths);
        }
    })
    .ok()?;
    watcher.watch(path, mode).ok()?;
    // Ends on its own when the watcher, and with it the sender, is dropped.
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let batch = collect_batch(&rx, first, Duration::from_millis(settle_ms), MAX_WAIT, MAX_PATHS);
            let _ = proxy.send_event(wrap(batch));
        }
    });
    Some(watcher)
}

/// Gathers `first` and what follows until `settle` passes without changes,
/// `max_wait` passes since the start, or there are `max_paths` different paths.
/// Sorted, without repeats.
fn collect_batch(
    rx: &mpsc::Receiver<Vec<PathBuf>>,
    mut batch: Vec<PathBuf>,
    settle: Duration,
    max_wait: Duration,
    max_paths: usize,
) -> Vec<PathBuf> {
    let start = Instant::now();
    while let Some(left) = max_wait.checked_sub(start.elapsed()) {
        match rx.recv_timeout(settle.min(left)) {
            Ok(more) => batch.extend(more),
            Err(_) => break,
        }
        if batch.len() >= max_paths {
            batch.sort();
            batch.dedup();
            if batch.len() >= max_paths {
                return batch;
            }
        }
    }
    batch.sort();
    batch.dedup();
    batch
}

/// One watcher per folder of the loose pages in the tabs, without subfolders.
#[derive(Default)]
pub struct LooseWatchers(Vec<(PathBuf, RecommendedWatcher)>);

impl LooseWatchers {
    /// Watches exactly `dirs`: new folders start, the ones no tab needs stop.
    pub fn sync(&mut self, dirs: &[PathBuf], proxy: &EventLoopProxy<UserEvent>) {
        self.0.retain(|(dir, _)| dirs.contains(dir));
        for dir in dirs {
            if self.0.iter().any(|(d, _)| d == dir) {
                continue;
            }
            if let Some(w) = watch(dir, RecursiveMode::NonRecursive, 150, proxy.clone(), UserEvent::LooseChanged) {
                self.0.push((dir.clone(), w));
            }
        }
    }
}

pub fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// The last file the app wrote and a hash of what it wrote. A change on disk
/// that still matches is the app's own; anything else came from outside.
pub struct OwnWrite {
    pub path: PathBuf,
    pub hash: u64,
}

impl OwnWrite {
    pub fn new(path: &Path, text: &str) -> OwnWrite {
        OwnWrite { path: path.to_path_buf(), hash: hash(text.as_bytes()) }
    }
}

/// What the decision reads from disk, so tests need none.
pub trait Disk {
    /// The hash of the file's content, if it can be read.
    fn hash_of(&self, path: &Path) -> Option<u64>;
    fn is_dir(&self, path: &Path) -> bool;
}

pub struct RealDisk;

impl Disk for RealDisk {
    fn hash_of(&self, path: &Path) -> Option<u64> {
        std::fs::read(path).ok().map(|b| hash(&b))
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

/// A live page view, as the decision needs to know it.
pub struct LivePage<'a> {
    pub view: u64,
    /// The vault file the view's URL shows, if any.
    pub file: Option<&'a str>,
    /// What its tab holds: a vault path, a web address, an absolute path, or empty.
    pub target: &'a str,
}

/// What to do with one live page.
#[derive(Debug, PartialEq)]
pub enum PageAction {
    /// The start page: its message depends on whether the vault has files.
    ReloadEmpty,
    Reload,
    /// A note or a drawing: it tells the editor, which keeps its text, or reloads if it has none.
    NoteChanged,
}

/// What a batch of changes in the vault asks of the window.
#[derive(Debug, Default, PartialEq)]
pub struct VaultReaction {
    /// Views whose annotations changed on disk: their layer re-reads them.
    pub reload_annotations: Vec<u64>,
    pub refresh_tree: bool,
    pub pages: Vec<(u64, PageAction)>,
}

pub fn vault_reaction(
    root: &Path,
    changed: &[PathBuf],
    own: Option<&OwnWrite>,
    pages: &[LivePage],
    disk: &impl Disk,
) -> VaultReaction {
    let mut reaction = VaultReaction::default();
    // An agent replied or the page saved: the layer re-reads its notes.
    for page in pages {
        if page.file.is_some() && changed.iter().any(|p| annot::page_of(root, p).as_deref() == page.file) {
            reaction.reload_annotations.push(page.view);
        }
    }
    let changed: Vec<&PathBuf> = changed.iter().filter(|p| !vault::ignored(root, p)).collect();
    if changed.is_empty() {
        return reaction;
    }
    reaction.refresh_tree = true;
    // What the app just saved itself is already on screen.
    let changed: Vec<&PathBuf> = changed
        .into_iter()
        .filter(|p| !own.is_some_and(|o| o.path == **p && disk.hash_of(p) == Some(o.hash)))
        .collect();
    // Pages may load sibling CSS, JS or data, so any change outside pages reloads too.
    let other = changed.iter().any(|p| !vault::is_page(p) && !disk.is_dir(p));
    for page in pages {
        if page.target.is_empty() {
            reaction.pages.push((page.view, PageAction::ReloadEmpty));
            continue;
        }
        if page.target.starts_with("http://") || page.target.starts_with("https://") || vault::is_loose(page.target) {
            continue;
        }
        let current = root.join(page.target);
        if !(other || changed.iter().any(|p| **p == current)) {
            continue;
        }
        // Notes and drawings keep their editor's text; other pages reload.
        let action = if vault::is_markdown(&current) || vault::is_drawing(&current) { PageAction::NoteChanged } else { PageAction::Reload };
        reaction.pages.push((page.view, action));
    }
    reaction
}

/// The loose pages to reload: the ones that changed, or whose folder had
/// another file change, which may be their CSS or data.
pub fn loose_reaction(changed: &[PathBuf], pages: &[LivePage]) -> Vec<u64> {
    pages
        .iter()
        .filter(|page| vault::is_loose(page.target))
        .filter(|page| {
            let file = Path::new(page.target);
            changed.iter().any(|p| p == file || (p.parent() == file.parent() && !vault::is_page(p)))
        })
        .map(|page| page.view)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeDisk {
        hashes: HashMap<PathBuf, u64>,
        dirs: Vec<PathBuf>,
    }

    impl Disk for FakeDisk {
        fn hash_of(&self, path: &Path) -> Option<u64> {
            self.hashes.get(path).copied()
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.iter().any(|d| d == path)
        }
    }

    fn feed(every_ms: u64, times: u64) -> mpsc::Receiver<Vec<PathBuf>> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for i in 0..times {
                if tx.send(vec![PathBuf::from(format!("/v/{}", i % 3))]).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(every_ms));
            }
        });
        rx
    }

    #[test]
    fn a_quiet_moment_closes_the_batch() {
        let rx = feed(5, 4);
        let batch = collect_batch(&rx, vec!["/v/x".into()], Duration::from_millis(100), Duration::from_secs(5), 100);
        assert_eq!(batch.len(), 4, "x plus the three different paths, sorted and without repeats");
    }

    #[test]
    fn nonstop_changes_still_close_the_batch_after_the_wait() {
        // A change every 20 ms for 3 s never leaves 100 ms of quiet.
        let rx = feed(20, 150);
        let start = Instant::now();
        let batch = collect_batch(&rx, vec![], Duration::from_millis(100), Duration::from_millis(400), 100);
        let took = start.elapsed();
        assert!(took >= Duration::from_millis(400) && took < Duration::from_millis(1000), "{took:?}");
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn a_batch_closes_at_the_path_limit() {
        let (tx, rx) = mpsc::channel();
        for i in 0..50 {
            tx.send(vec![PathBuf::from(format!("/v/{i}"))]).unwrap();
        }
        let batch = collect_batch(&rx, vec![], Duration::from_secs(5), Duration::from_secs(5), 10);
        assert_eq!(batch.len(), 10);
        // The rest waits for the next batch.
        assert_eq!(rx.try_iter().count(), 40);
    }

    const ROOT: &str = "/v";

    fn page<'a>(view: u64, file: Option<&'a str>, target: &'a str) -> LivePage<'a> {
        LivePage { view, file, target }
    }

    fn react(changed: &[&str], own: Option<&OwnWrite>, pages: &[LivePage], disk: &FakeDisk) -> VaultReaction {
        let changed: Vec<PathBuf> = changed.iter().map(PathBuf::from).collect();
        vault_reaction(Path::new(ROOT), &changed, own, pages, disk)
    }

    #[test]
    fn a_changed_page_reloads_and_the_tree_refreshes() {
        let pages = [page(1, Some("a.html"), "a.html"), page(2, Some("b.html"), "b.html")];
        let r = react(&["/v/a.html"], None, &pages, &FakeDisk::default());
        assert!(r.refresh_tree);
        assert_eq!(r.pages, [(1, PageAction::Reload)]);
        assert!(r.reload_annotations.is_empty());
    }

    #[test]
    fn hidden_and_outside_files_are_ignored() {
        let pages = [page(1, Some("a.html"), "a.html")];
        for changed in ["/v/.git/index", "/v/node_modules/x.js", "/v/.visor/config", "/elsewhere/a.html"] {
            assert_eq!(react(&[changed], None, &pages, &FakeDisk::default()), VaultReaction::default(), "{changed}");
        }
    }

    #[test]
    fn own_write_with_the_same_hash_does_not_reload_but_refreshes_the_tree() {
        let pages = [page(1, Some("n.md"), "n.md")];
        let own = OwnWrite { path: "/v/n.md".into(), hash: 7 };
        let disk = FakeDisk { hashes: HashMap::from([("/v/n.md".into(), 7)]), ..Default::default() };
        let r = react(&["/v/n.md"], Some(&own), &pages, &disk);
        assert!(r.refresh_tree);
        assert!(r.pages.is_empty());
    }

    #[test]
    fn own_write_with_another_hash_is_an_outside_change() {
        let pages = [page(1, Some("n.md"), "n.md")];
        let own = OwnWrite { path: "/v/n.md".into(), hash: 7 };
        let disk = FakeDisk { hashes: HashMap::from([("/v/n.md".into(), 8)]), ..Default::default() };
        let r = react(&["/v/n.md"], Some(&own), &pages, &disk);
        assert_eq!(r.pages, [(1, PageAction::NoteChanged)]);
        // A file that cannot be read cannot match either.
        let r = react(&["/v/n.md"], Some(&own), &pages, &FakeDisk::default());
        assert_eq!(r.pages, [(1, PageAction::NoteChanged)]);
    }

    #[test]
    fn own_write_only_covers_its_own_file() {
        let pages = [page(1, Some("a.md"), "a.md")];
        let own = OwnWrite { path: "/v/n.md".into(), hash: 7 };
        let disk = FakeDisk { hashes: HashMap::from([("/v/n.md".into(), 7), ("/v/a.md".into(), 7)]), ..Default::default() };
        let r = react(&["/v/a.md"], Some(&own), &pages, &disk);
        assert_eq!(r.pages, [(1, PageAction::NoteChanged)]);
    }

    #[test]
    fn annotations_of_a_live_page_reload_its_layer_only() {
        let pages = [page(1, Some("a.html"), "a.html"), page(2, Some("sub/b.html"), "sub/b.html")];
        let r = react(&["/v/.visor/anotaciones/sub/b.html.json"], None, &pages, &FakeDisk::default());
        assert_eq!(r.reload_annotations, [2]);
        // The folder is hidden, so nothing else happens.
        assert!(!r.refresh_tree);
        assert!(r.pages.is_empty());
    }

    #[test]
    fn annotations_of_a_page_that_is_not_live_do_nothing() {
        let pages = [page(1, Some("a.html"), "a.html")];
        let r = react(&["/v/.visor/anotaciones/z.html.json"], None, &pages, &FakeDisk::default());
        assert_eq!(r, VaultReaction::default());
    }

    #[test]
    fn a_note_asks_the_editor_instead_of_reloading() {
        let pages = [page(1, Some("n.md"), "n.md"), page(2, Some("a.html"), "a.html")];
        let r = react(&["/v/n.md"], None, &pages, &FakeDisk::default());
        assert_eq!(r.pages, [(1, PageAction::NoteChanged)]);
    }

    #[test]
    fn a_change_outside_pages_reloads_every_vault_page() {
        let pages = [page(1, Some("n.md"), "n.md"), page(2, Some("a.html"), "a.html"), page(3, None, "https://x.org"), page(4, None, "/tmp/l.html")];
        let r = react(&["/v/style.css"], None, &pages, &FakeDisk::default());
        assert_eq!(r.pages, [(1, PageAction::NoteChanged), (2, PageAction::Reload)]);
    }

    #[test]
    fn a_folder_change_alone_reloads_no_page() {
        let pages = [page(1, Some("a.html"), "a.html")];
        let disk = FakeDisk { dirs: vec!["/v/sub".into()], ..Default::default() };
        let r = react(&["/v/sub"], None, &pages, &disk);
        assert!(r.refresh_tree);
        assert!(r.pages.is_empty());
    }

    #[test]
    fn the_start_page_reloads_on_any_visible_change() {
        let pages = [page(1, None, "")];
        let r = react(&["/v/other.html"], None, &pages, &FakeDisk::default());
        assert_eq!(r.pages, [(1, PageAction::ReloadEmpty)]);
    }

    #[test]
    fn loose_pages_reload_on_their_own_change_or_a_sibling_asset() {
        let pages = [page(1, None, "/tmp/a/x.html"), page(2, None, "/tmp/b/y.html"), page(3, Some("n.md"), "n.md")];
        let changed = |paths: &[&str]| paths.iter().map(PathBuf::from).collect::<Vec<_>>();
        assert_eq!(loose_reaction(&changed(&["/tmp/a/x.html"]), &pages), [1]);
        assert_eq!(loose_reaction(&changed(&["/tmp/a/style.css"]), &pages), [1]);
        // Another page of the folder is not an asset of this one.
        assert!(loose_reaction(&changed(&["/tmp/a/other.html"]), &pages).is_empty());
        assert!(loose_reaction(&changed(&["/tmp/c/style.css"]), &pages).is_empty());
    }
}
