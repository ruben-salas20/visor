//! Area A, the panel: what the frame's panel asks and what it shows. Frame
//! side: `marco/src/areas/files/`. The GTK panel this replaces is
//! `ui/sidebar.rs` before the frame (its pure helpers too: `placeholder`,
//! `cannot_open`, `reveal_on_arrival`).

use super::{App, open_elsewhere};
use crate::i18n::tr;
use crate::session::Place;
use crate::trf;
use crate::ui::action::{SidebarMsg, UserEvent};
use crate::ui::protocol::files::{self, State, Tree};
use crate::vault::{files as vault_files, links};
use std::path::Path;

impl App {
    pub(super) fn pick_vault(&self) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().set_title(tr("Abrir carpeta como bóveda", "Open folder as vault")).pick_folder();
            let picked = picked.and_then(|p| p.canonicalize().ok());
            proxy.send(UserEvent::Picked(picked));
        });
    }

    pub(super) fn on_sidebar(&mut self, msg: SidebarMsg) {
        match msg {
            SidebarMsg::OpenFolders { folders } => {
                if let Some(vault) = self.vault() {
                    let path = vault.path().to_path_buf();
                    self.cfg_mut(|c| c.open_folders.insert(path, folders));
                    self.save_config_soon();
                }
            }
            SidebarMsg::PickVault => self.pick_vault(),
            // The frame names a vault from the list it was sent; any other path is refused.
            SidebarMsg::SwitchVault { path } => {
                if self.cfg(|c| c.vaults.contains(&path)) {
                    self.show_vault(Some(path));
                }
            }
            SidebarMsg::Open { path, tab, beside } => {
                let place = if beside { Place::Beside } else if tab { Place::NewTab } else { Place::Here };
                self.open_in(path, place);
            }
            SidebarMsg::CreateNote { dir, name } => match self.create_note(&dir, &name) {
                Ok(()) => self.note_created(),
                Err(e) => self.notice(&e, true),
            },
            SidebarMsg::CreateFolder { dir, name } => match self.create_folder(&dir, &name) {
                Ok(()) => self.note_created(),
                Err(e) => self.notice(&e, true),
            },
            SidebarMsg::Trash { path } => {
                let result = self.open_vault().and_then(|v| vault_files::trash(&v, &path));
                match result {
                    Ok(()) => {
                        self.notice(&trf!("«{path}» está en la papelera.", "«{path}» is in the trash."), false);
                        self.retarget(&path, None);
                    }
                    Err(e) => self.notice(&e, true),
                }
            }
            SidebarMsg::OpenElsewhere { path } => {
                // `resolve` also refuses a link that points out of the vault.
                let file = self.open_vault().and_then(|v| {
                    if v.entry(&path)?.is_dir() {
                        return Err(trf!("«{path}» es una carpeta, no un archivo.", "«{path}» is a folder, not a file."));
                    }
                    v.resolve(&path).ok_or_else(|| trf!("No existe: {path}", "Does not exist: {path}"))
                });
                match file {
                    Ok(file) => {
                        if !open_elsewhere(&file) {
                            self.notice(&no_application(&path), true);
                        }
                    }
                    Err(e) => self.notice(&e, true),
                }
            }
            SidebarMsg::Move { path, dir } => {
                let result = self.open_vault().and_then(|v| vault_files::move_to(&v, &path, &dir));
                match result {
                    Ok(Some(new)) => {
                        let root = self.vault().map(|v| v.path().to_path_buf()).unwrap_or_default();
                        match links::relink(&root, &path, &new) {
                            Ok(changed) if !changed.is_empty() => {
                                let n = changed.len();
                                let text = if n == 1 {
                                    tr("Se actualizaron los enlaces de 1 nota.", "Updated the links in 1 note.").to_string()
                                } else {
                                    trf!("Se actualizaron los enlaces de {n} notas.", "Updated the links in {n} notes.")
                                };
                                self.notice(&text, false);
                            }
                            Ok(_) => {}
                            Err(e) => self.notice(&e, true),
                        }
                        self.retarget(&path, Some(&new));
                    }
                    Ok(None) => {}
                    Err(e) => self.notice(&e, true),
                }
            }
            SidebarMsg::RemoveVault { path } => {
                self.cfg_mut(|c| {
                    c.vaults.retain(|v| *v != path);
                    c.last.remove(&path);
                    c.workspaces.remove(&path);
                    c.open_folders.remove(&path);
                });
                if self.vault().is_some_and(|v| v.path() == path) {
                    self.cfg_mut(|c| c.current = c.vaults.first().cloned());
                    self.save_config_soon();
                    self.leave_removed_vault();
                } else {
                    self.save_config_soon();
                    self.refresh_tree();
                }
            }
        }
    }

    /// A message for the person about something done in the panel.
    pub(super) fn notice(&mut self, text: &str, error: bool) {
        // Phase 1 shows it in the panel.
        let _ = error;
        eprintln!("{text}");
    }

    /// A note or folder typed in the panel was created.
    fn note_created(&mut self) {
        // Phase 1 closes the panel's name field.
    }

    /// Ctrl+N: asks for the name of a new note in `dir`.
    pub(super) fn new_note(&mut self, dir: &str) {
        // Phase 1 opens the panel's name field.
        let _ = dir;
        self.sidebar_shown = true;
    }

    /// The tree changed on disk, or what it lists: it is sent again after the event.
    pub(super) fn refresh_tree(&mut self) {
        self.frame.tree_stale = true;
    }

    /// The vault's tree as the panel lists it. Reads the disk.
    pub(super) fn tree(&self) -> Tree {
        let show_other = self.cfg(|c| c.show_other_files != Some(false));
        Tree { root: self.vault().map(|v| files::visible(v.tree(), show_other)) }
    }

    pub(super) fn files_state(&self) -> State {
        let vault = self.vault().map(|v| name_of(v.path()));
        State { shown: self.sidebar_shown, vault, current: self.focused_file() }
    }
}

pub(super) fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
}

pub(super) fn no_application(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    trf!("No hay ninguna aplicación para abrir «{name}».", "No application can open «{name}».")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_file_no_program_opens() {
        assert_eq!(no_application("datos/Libro.xlsx"), "No hay ninguna aplicación para abrir «Libro.xlsx».");
        assert_eq!(no_application("Makefile"), "No hay ninguna aplicación para abrir «Makefile».");
    }
}
