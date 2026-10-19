//! Area A, the panel: the vault's name and its tree. What the panel asks is
//! `ui::action::SidebarMsg`. Frame side: `marco/src/areas/files/`.

use crate::vault::{Dir, Kind};
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
}

/// The vault's tree, as the panel lists it.
#[derive(Serialize, Debug, PartialEq)]
pub struct Tree {
    pub root: Option<Dir>,
}

/// `dir` without the files visor does not open, unless the person wants them.
pub fn visible(dir: Dir, show_other: bool) -> Dir {
    Dir {
        name: dir.name,
        path: dir.path,
        dirs: dir.dirs.into_iter().map(|d| visible(d, show_other)).collect(),
        files: dir.files.into_iter().filter(|f| show_other || f.kind != Kind::Other).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::File;

    #[test]
    fn other_files_show_only_when_asked() {
        let file = |name: &str, kind| File { name: name.into(), path: format!("d/{name}"), kind };
        let tree = || Dir {
            name: "b".into(),
            path: String::new(),
            dirs: vec![Dir { name: "d".into(), path: "d".into(), dirs: vec![], files: vec![file("a", Kind::Page), file("x.zip", Kind::Other)] }],
            files: vec![file("y.bin", Kind::Other)],
        };
        let shown = visible(tree(), false);
        assert!(shown.files.is_empty());
        assert_eq!(shown.dirs[0].files.len(), 1);
        assert_eq!(visible(tree(), true).dirs[0].files.len(), 2);
    }
}
