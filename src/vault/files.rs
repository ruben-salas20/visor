//! Moving and deleting vault files from the sidebar. A page's annotations
//! live apart from it, so they go wherever the page goes.

use crate::vault::annot;
use std::fs;
use crate::vault::Vault;
use std::path::{Path, PathBuf};

/// Annotations of a page (`<page>.json`) or of everything in a folder.
fn annotations(root: &Path, rel: &str, is_dir: bool) -> PathBuf {
    if is_dir { root.join(annot::DIR).join(rel) } else { annot::file(root, rel) }
}

/// Moves `rel` into the folder `dir` and returns its new path, or `None` if it was already there.
pub fn move_to(vault: &Vault, rel: &str, dir: &str) -> Result<Option<String>, String> {
    let from = vault.entry(rel)?;
    let to_dir = vault.folder(dir)?;
    if from.parent() == Some(to_dir.as_path()) {
        return Ok(None);
    }
    if from.is_dir() && to_dir.starts_with(&from) {
        return Err("No se puede mover una carpeta dentro de sí misma.".into());
    }
    let name = from.file_name().ok_or("Ruta no válida")?;
    let to = to_dir.join(name);
    if fs::symlink_metadata(&to).is_ok() {
        return Err(format!("Ya hay algo llamado «{}» en esa carpeta.", name.to_string_lossy()));
    }
    fs::rename(&from, &to).map_err(|e| format!("No se pudo mover: {e}"))?;
    let new_rel = vault.rel(&to);

    let is_dir = to.is_dir();
    let old_notes = annotations(vault.root(), rel, is_dir);
    if old_notes.exists() {
        let new_notes = annotations(vault.root(), &new_rel, is_dir);
        let moved = new_notes.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::rename(&old_notes, &new_notes));
        if let Err(e) = moved {
            return Err(format!("Se movió, pero sus anotaciones se quedaron en {}: {e}", old_notes.display()));
        }
    }
    Ok(Some(new_rel))
}

/// Sends `rel` and its annotations to the desktop trash, where they can be restored.
pub fn trash(vault: &Vault, rel: &str) -> Result<(), String> {
    use gtk::gio::{Cancellable, File, prelude::FileExt};
    let path = vault.entry(rel)?;
    let notes = annotations(vault.root(), rel, path.is_dir());
    File::for_path(&path).trash(Cancellable::NONE).map_err(|e| format!("No se pudo mover a la papelera: {e}"))?;
    if notes.exists() {
        let _ = File::for_path(&notes).trash(Cancellable::NONE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("visor-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join(annot::DIR).join("a")).unwrap();
        fs::write(root.join("a/nota.md"), "# Nota").unwrap();
        fs::write(root.join(annot::DIR).join("a/nota.md.json"), "{}").unwrap();
        root.canonicalize().unwrap()
    }

    #[test]
    fn moves_page_with_annotations() {
        let root = vault("page");
        assert_eq!(move_to(&Vault::open(&root), "a/nota.md", "a/b").unwrap().as_deref(), Some("a/b/nota.md"));
        assert!(root.join("a/b/nota.md").is_file());
        assert!(annot::file(&root, "a/b/nota.md").is_file());
        assert!(!annot::file(&root, "a/nota.md").exists());
        assert_eq!(move_to(&Vault::open(&root), "a/b/nota.md", "a/b").unwrap(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moves_folder_with_annotations() {
        let root = vault("folder");
        fs::create_dir_all(root.join("c")).unwrap();
        assert_eq!(move_to(&Vault::open(&root), "a", "c").unwrap().as_deref(), Some("c/a"));
        assert!(annot::file(&root, "c/a/nota.md").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_bad_moves() {
        let root = vault("bad");
        fs::write(root.join("otra.md"), "").unwrap();
        assert!(move_to(&Vault::open(&root), "a", "a/b").is_err(), "into itself");
        assert!(move_to(&Vault::open(&root), "otra.md", "a").is_ok());
        fs::write(root.join("otra.md"), "").unwrap();
        assert!(move_to(&Vault::open(&root), "otra.md", "a").is_err(), "overwrite");
        assert!(move_to(&Vault::open(&root), "../x", "").is_err());
        assert!(move_to(&Vault::open(&root), ".visor", "a").is_err());
        assert!(move_to(&Vault::open(&root), "a/nota.md", "../").is_err());
        assert!(move_to(&Vault::open(&root), "a/nota.md", "a/nota.md").is_err(), "not a folder");
        assert!(trash(&Vault::open(&root), "").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
