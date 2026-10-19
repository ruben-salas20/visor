//! Moving and deleting vault files from the sidebar. A page's annotations
//! live apart from it, so they go wherever the page goes.

use crate::i18n::tr;
use crate::platform;
use crate::trf;
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
        return Err(tr("No se puede mover una carpeta dentro de sí misma.", "A folder cannot be moved into itself.").into());
    }
    let name = from.file_name().ok_or(tr("Ruta no válida", "Invalid path"))?;
    let to = to_dir.join(name);
    if fs::symlink_metadata(&to).is_ok() {
        return Err(trf!(
            "Ya hay algo llamado «{}» en esa carpeta.",
            "Something named «{}» already exists in that folder.",
            name.to_string_lossy()
        ));
    }
    fs::rename(&from, &to).map_err(|e| trf!("No se pudo mover: {e}", "Could not move: {e}"))?;
    let new_rel = vault.rel(&to);

    let is_dir = to.is_dir();
    let old_notes = annotations(vault.root(), rel, is_dir);
    if old_notes.exists() {
        let new_notes = annotations(vault.root(), &new_rel, is_dir);
        let moved = new_notes.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::rename(&old_notes, &new_notes));
        if let Err(e) = moved {
            return Err(trf!(
                "Se movió, pero sus anotaciones se quedaron en {}: {e}",
                "It was moved, but its annotations stayed in {}: {e}",
                old_notes.display()
            ));
        }
    }
    Ok(Some(new_rel))
}

/// Sends `rel` and its annotations to the desktop trash, where they can be restored.
pub fn trash(vault: &Vault, rel: &str) -> Result<(), String> {
    let path = vault.entry(rel)?;
    let notes = annotations(vault.root(), rel, path.is_dir());
    platform::trash(&path).map_err(|e| trf!("No se pudo mover a la papelera: {e}", "Could not move to the trash: {e}"))?;
    if notes.exists() {
        let _ = platform::trash(&notes);
    }
    Ok(())
}

/// The note a `[[name#Section|alias]]` link names: without the section or the alias.
pub fn wiki_note_name(target: &str) -> &str {
    target.split(['#', '|']).next().unwrap_or_default().trim()
}

/// Folder and file name for a new note called `name`. A plain name goes in `dir`.
/// A name with «/» is a path from the vault root, as in Obsidian's `[[carpeta/nota]]`:
/// missing folders are created, and none of it may leave the vault.
pub fn note_target(vault: &Vault, dir: &str, name: &str) -> Result<(PathBuf, String), String> {
    if !name.contains('/') {
        return vault.new_in(dir, name).map(|(folder, name)| (folder, name.to_string()));
    }
    let bad = || tr("La ruta no puede tener partes vacías, empezar por punto ni usar «\\».", "The path cannot have empty parts, start a part with a dot or use «\\».").to_string();
    let parts: Vec<&str> = name.split('/').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty() || p.starts_with('.') || p.contains('\\')) {
        return Err(bad());
    }
    let (file, folders) = parts.split_last().ok_or_else(bad)?;
    // Nothing is created until the whole path is known to be valid.
    let mut folder = vault.root().to_path_buf();
    let mut missing = false;
    for part in folders {
        folder.push(part);
        if missing {
            continue;
        }
        match fs::symlink_metadata(&folder) {
            Ok(_) => {
                let real = platform::canonicalize(&folder).map_err(|_| trf!("No existe la carpeta {part}", "The folder {part} does not exist"))?;
                if !real.is_dir() {
                    return Err(trf!("«{part}» no es una carpeta.", "«{part}» is not a folder."));
                }
                if !real.starts_with(vault.root()) {
                    return Err(tr("La carpeta está fuera de la bóveda.", "The folder is outside the vault.").into());
                }
                folder = real;
            }
            Err(_) => missing = true,
        }
    }
    if missing {
        fs::create_dir_all(&folder).map_err(|e| trf!("No se pudo crear la carpeta: {e}", "Could not create the folder: {e}"))?;
    }
    Ok((folder, (*file).to_string()))
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
        platform::canonicalize(&root).unwrap()
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
    fn note_target_creates_folders_from_the_root() {
        let root = vault("target");
        let v = Vault::open(&root);
        assert_eq!(note_target(&v, "a", "x").unwrap(), (root.join("a"), "x".to_string()), "plain name: in dir");
        assert_eq!(note_target(&v, "a", "a/b/x").unwrap(), (root.join("a/b"), "x".to_string()), "existing folders");
        assert_eq!(note_target(&v, "a", "n/m/x").unwrap(), (root.join("n/m"), "x".to_string()), "relative to the root, not to dir");
        assert!(root.join("n/m").is_dir());
        assert_eq!(note_target(&v, "", " p / q ").unwrap(), (root.join("p"), "q".to_string()));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn note_target_refuses_bad_paths_and_creates_nothing() {
        use std::os::unix::fs::symlink;
        let root = vault("targetbad");
        let out = root.with_file_name(format!("visor-files-out-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        symlink(&out, root.join("salida")).unwrap();
        let v = Vault::open(&root);
        for name in ["a//x", "/x", "a/", "../x", "a/../x", ".visor/x", "a/.o/x", "n/.x", "a\\b/x", " /x"] {
            assert!(note_target(&v, "", name).is_err(), "{name:?}");
        }
        assert!(note_target(&v, "", "salida/x").is_err(), "link out of the vault");
        assert!(note_target(&v, "", "a/nota.md/x").is_err(), "a file is not a folder");
        assert!(!root.join("n").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(out).unwrap();
    }

    #[test]
    fn wiki_note_name_drops_section_and_alias() {
        assert_eq!(wiki_note_name("nota#Sección"), "nota");
        assert_eq!(wiki_note_name("a/nota|texto"), "a/nota");
        assert_eq!(wiki_note_name(" nota#S|t "), "nota");
        assert_eq!(wiki_note_name("nota"), "nota");
        assert_eq!(wiki_note_name("#Sección"), "");
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
