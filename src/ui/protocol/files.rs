//! Area A, the panel: the vault's name and its tree. What the panel asks is
//! `ui::action::SidebarMsg`. Frame side: `marco/src/areas/files/`.

use crate::i18n::tr;
use crate::trf;
use crate::vault::{Dir, File, Kind};
use serde::Serialize;

/// The panel, without the tree.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct State {
    /// Hidden for a loose page started alone, and by Ctrl+B.
    pub shown: bool,
    /// The vault's folder name, or none.
    pub vault: Option<String>,
    /// The file on the focused side, to mark it in the tree.
    pub current: Option<String>,
    /// The vaults in the settings, for the vault menu.
    pub vaults: Vec<VaultItem>,
    /// The field that asks for the name of a new note or folder, if open.
    pub form: Option<Form>,
    /// The last message about something done in the panel.
    pub notice: Option<Notice>,
    /// The panel's width in CSS pixels, if the person changed it.
    pub width: Option<i32>,
    /// Whether files visor does not open are listed.
    pub show_other: bool,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct VaultItem {
    /// The whole path: the frame sends it back to switch or remove.
    pub path: String,
    pub name: String,
    pub current: bool,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Form {
    /// The folder the new note or folder goes in; empty for the vault root.
    pub dir: String,
    pub folder: bool,
    /// Why the last name typed was refused.
    pub error: Option<String>,
    /// Changes each time the field opens, so the frame empties and focuses it.
    pub id: u64,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Notice {
    /// Changes with every notice, so the same text shown twice shows again.
    pub id: u64,
    pub text: String,
    pub error: bool,
    /// A file visor does not show: the notice offers to open it with another application.
    pub elsewhere: Option<String>,
}

/// The vault's tree, as the panel lists it.
#[derive(Serialize, Debug, PartialEq)]
pub struct Tree {
    /// The vault's whole path: when it changes, the frame restores `open`.
    pub vault: Option<String>,
    pub root: Option<Folder>,
    /// The folders the person left open in this vault.
    pub open: Vec<String>,
}

/// A folder as the panel lists it.
#[derive(Serialize, Debug, PartialEq)]
pub struct Folder {
    pub name: String,
    pub path: String,
    /// How many files it lists, counting its subfolders.
    pub count: usize,
    /// What the folder says when it lists nothing.
    pub empty: Option<String>,
    pub dirs: Vec<Folder>,
    pub files: Vec<File>,
}

/// `dir` without the files visor does not open, unless the person wants them.
pub fn visible(dir: Dir, show_other: bool) -> Folder {
    let empty = placeholder(&dir, show_other).map(str::to_string);
    let dirs: Vec<Folder> = dir.dirs.into_iter().map(|d| visible(d, show_other)).collect();
    let files: Vec<File> = dir.files.into_iter().filter(|f| show_other || f.kind != Kind::Other).collect();
    let count = files.len() + dirs.iter().map(|d| d.count).sum::<usize>();
    Folder { name: dir.name, path: dir.path, count, empty, dirs, files }
}

/// The line inside a folder that would otherwise show nothing.
fn placeholder(dir: &Dir, show_other: bool) -> Option<&'static str> {
    if !dir.dirs.is_empty() {
        None
    } else if dir.files.is_empty() {
        Some(tr("Carpeta vacía", "Empty folder"))
    } else if !show_other && dir.files.iter().all(|f| f.kind == Kind::Other) {
        Some(tr("Sin archivos que visor abra", "No files that visor opens"))
    } else {
        None
    }
}

/// What the panel says when the person opens a file visor does not show.
pub fn cannot_open(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => trf!("visor no abre archivos .{}.", "visor does not open .{} files.", ext.to_lowercase()),
        _ => tr("visor no abre este archivo.", "visor does not open this file.").into(),
    }
}

/// The folder of `file`, where Ctrl+N and the panel's buttons put a new note.
pub fn folder_of(file: Option<&str>) -> String {
    file.and_then(|f| f.rsplit_once('/')).map_or("", |(d, _)| d).to_string()
}

/// The panel's width, kept within what leaves room for it and for the pages.
pub fn clamp_width(width: i32) -> i32 {
    width.clamp(180, 560)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, kind: Kind) -> File {
        File { name: name.into(), path: format!("d/{name}"), kind }
    }

    fn tree() -> Dir {
        Dir {
            name: "b".into(),
            path: String::new(),
            dirs: vec![
                Dir { name: "d".into(), path: "d".into(), dirs: vec![], files: vec![file("a", Kind::Page), file("x.zip", Kind::Other)] },
                Dir { name: "e".into(), path: "e".into(), dirs: vec![], files: vec![] },
                Dir { name: "f".into(), path: "f".into(), dirs: vec![], files: vec![file("y.bin", Kind::Other)] },
            ],
            files: vec![file("y.bin", Kind::Other), file("z", Kind::Pdf)],
        }
    }

    #[test]
    fn other_files_show_only_when_asked() {
        let shown = visible(tree(), false);
        assert_eq!(shown.files.len(), 1);
        assert_eq!(shown.dirs[0].files.len(), 1);
        assert_eq!(visible(tree(), true).dirs[0].files.len(), 2);
    }

    #[test]
    fn folders_count_the_files_they_list() {
        let hidden = visible(tree(), false);
        assert_eq!((hidden.count, hidden.dirs[0].count, hidden.dirs[1].count), (2, 1, 0));
        let all = visible(tree(), true);
        assert_eq!((all.count, all.dirs[0].count, all.dirs[2].count), (5, 2, 1));
    }

    #[test]
    fn says_why_a_folder_shows_nothing() {
        let hidden = visible(tree(), false);
        assert_eq!(hidden.empty, None, "the root has folders");
        assert_eq!(hidden.dirs[0].empty, None);
        assert_eq!(hidden.dirs[1].empty.as_deref(), Some("Carpeta vacía"));
        assert_eq!(hidden.dirs[2].empty.as_deref(), Some("Sin archivos que visor abra"));
        assert_eq!(visible(tree(), true).dirs[2].empty, None);
    }

    #[test]
    fn names_the_kind_of_file_visor_cannot_open() {
        assert_eq!(cannot_open("datos/Libro.XLSX"), "visor no abre archivos .xlsx.");
        assert_eq!(cannot_open("Makefile"), "visor no abre este archivo.");
        assert_eq!(cannot_open(".env"), "visor no abre este archivo.");
    }

    #[test]
    fn a_new_note_goes_next_to_the_current_file() {
        assert_eq!(folder_of(Some("a/b/c.md")), "a/b");
        assert_eq!(folder_of(Some("c.md")), "");
        assert_eq!(folder_of(None), "");
    }

    #[test]
    fn the_width_stays_usable() {
        assert_eq!(clamp_width(10), 180);
        assert_eq!(clamp_width(300), 300);
        assert_eq!(clamp_width(4000), 560);
    }
}
