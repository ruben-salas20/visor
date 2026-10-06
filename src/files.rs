//! Moving and deleting vault files from the sidebar. A page's annotations
//! live apart from it, so they go wherever the page goes.

use crate::annot;
use std::fs;
use std::path::{Path, PathBuf};

/// A file or folder of the vault named by the sidebar, never the vault itself
/// nor anything outside it or hidden.
fn inside(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if rel.is_empty() || rel.split('/').any(|seg| seg.starts_with('.') || seg.is_empty()) {
        return Err(format!("Ruta no válida: {rel}"));
    }
    // The parent is canonicalized, not the item, so a link is moved and not its target.
    let path = root.join(rel);
    let parent = path.parent().and_then(|p| p.canonicalize().ok()).ok_or_else(|| format!("No existe: {rel}"))?;
    let path = parent.join(path.file_name().ok_or("Ruta no válida")?);
    if !parent.starts_with(&root) || fs::symlink_metadata(&path).is_err() {
        return Err(format!("No existe: {rel}"));
    }
    Ok(path)
}

fn folder(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let dir = if rel.is_empty() { root.canonicalize().map_err(|e| e.to_string())? } else { inside(root, rel)? };
    if dir.is_dir() { Ok(dir) } else { Err(format!("No es una carpeta: {rel}")) }
}

/// Annotations of a page (`<page>.json`) or of everything in a folder.
fn annotations(root: &Path, rel: &str, is_dir: bool) -> PathBuf {
    if is_dir { root.join(annot::DIR).join(rel) } else { annot::file(root, rel) }
}

/// Moves `rel` into the folder `dir` and returns its new path, or `None` if it was already there.
pub fn move_to(root: &Path, rel: &str, dir: &str) -> Result<Option<String>, String> {
    let from = inside(root, rel)?;
    let to_dir = folder(root, dir)?;
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
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let new_rel = crate::vault::rel(&canonical, &to);

    let is_dir = to.is_dir();
    let old_notes = annotations(root, rel, is_dir);
    if old_notes.exists() {
        let new_notes = annotations(root, &new_rel, is_dir);
        let moved = new_notes.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::rename(&old_notes, &new_notes));
        if let Err(e) = moved {
            return Err(format!("Se movió, pero sus anotaciones se quedaron en {}: {e}", old_notes.display()));
        }
    }
    Ok(Some(new_rel))
}

/// Sends `rel` and its annotations to the desktop trash, where they can be restored.
pub fn trash(root: &Path, rel: &str) -> Result<(), String> {
    use gtk::gio::{Cancellable, File, prelude::FileExt};
    let path = inside(root, rel)?;
    let notes = annotations(root, rel, path.is_dir());
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
        assert_eq!(move_to(&root, "a/nota.md", "a/b").unwrap().as_deref(), Some("a/b/nota.md"));
        assert!(root.join("a/b/nota.md").is_file());
        assert!(annot::file(&root, "a/b/nota.md").is_file());
        assert!(!annot::file(&root, "a/nota.md").exists());
        assert_eq!(move_to(&root, "a/b/nota.md", "a/b").unwrap(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moves_folder_with_annotations() {
        let root = vault("folder");
        fs::create_dir_all(root.join("c")).unwrap();
        assert_eq!(move_to(&root, "a", "c").unwrap().as_deref(), Some("c/a"));
        assert!(annot::file(&root, "c/a/nota.md").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_bad_moves() {
        let root = vault("bad");
        fs::write(root.join("otra.md"), "").unwrap();
        assert!(move_to(&root, "a", "a/b").is_err(), "into itself");
        assert!(move_to(&root, "otra.md", "a").is_ok());
        fs::write(root.join("otra.md"), "").unwrap();
        assert!(move_to(&root, "otra.md", "a").is_err(), "overwrite");
        assert!(move_to(&root, "../x", "").is_err());
        assert!(move_to(&root, ".visor", "a").is_err());
        assert!(move_to(&root, "a/nota.md", "../").is_err());
        assert!(move_to(&root, "a/nota.md", "a/nota.md").is_err(), "not a folder");
        assert!(trash(&root, "").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
