//! Area A, the panel: what the frame's panel asks and what it shows. Frame
//! side: `marco/src/areas/files/`. The GTK panel this replaces is
//! `ui/sidebar.rs` before the frame.

use super::{App, open_elsewhere};
use crate::i18n::tr;
use crate::session::Place;
use crate::trf;
use crate::ui::action::{SidebarMsg, UserEvent};
use crate::ui::protocol::files::{self, Form, Notice, State, Tree, VaultItem};
use crate::vault::{self, files as vault_files, links};
use std::path::Path;

/// What the panel shows besides the tree.
#[derive(Default)]
pub(super) struct Panel {
    form: Option<Form>,
    notice: Option<Notice>,
    /// Numbers the notices and the name fields, so each one is new to the frame.
    count: u64,
}

impl Panel {
    fn next(&mut self) -> u64 {
        self.count += 1;
        self.count
    }
}

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
            SidebarMsg::DragStart { path } => self.drag_start(path),
            SidebarMsg::DragEnd => self.drag_end(),
            SidebarMsg::DropHover { side, zone } => self.drop_hover(side, zone.filter(|z| *z <= 2)),
            SidebarMsg::DropOn { side, half } => {
                if half.is_none_or(|h| h <= 1) {
                    self.dropped(side, half);
                }
            }
            // The frame names a vault from the list it was sent; any other path is refused.
            SidebarMsg::SwitchVault { path } => {
                if self.cfg(|c| c.vaults.contains(&path)) {
                    self.show_vault(Some(path));
                }
            }
            SidebarMsg::Open { path, tab, beside } => {
                // A file visor does not show is listed but not opened: the notice says why.
                if let Some(v) = self.vault()
                    && v.entry(&path).is_ok_and(|p| p.is_file() && !opens(&p))
                {
                    let text = files::cannot_open(&path);
                    self.show_notice(&text, false, Some(path));
                    return;
                }
                let place = if beside { Place::Beside } else if tab { Place::NewTab } else { Place::Here };
                self.open_in(path, place);
            }
            SidebarMsg::CreateNote { dir, name } => match self.create_note(&dir, &name) {
                Ok(()) => self.note_created(),
                Err(e) => self.name_refused(e),
            },
            SidebarMsg::CreateFolder { dir, name } => match self.create_folder(&dir, &name) {
                Ok(()) => self.note_created(),
                Err(e) => self.name_refused(e),
            },
            SidebarMsg::AskName { dir, folder } => {
                if self.vault().is_none() {
                    return self.pick_vault();
                }
                self.new_note(dir, folder);
            }
            SidebarMsg::CancelName => self.note_created(),
            SidebarMsg::Width { width } => {
                self.sidebar_width = Some(files::clamp_width(width));
                self.layout_changed = true;
            }
            SidebarMsg::ShowOther { on } => {
                self.cfg_mut(|c| c.show_other_files = Some(on).filter(|on| !on));
                self.save_config_soon();
                self.refresh_tree();
            }
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
        self.show_notice(text, error, None);
    }

    /// A notice; with `elsewhere`, it offers to open that file with another application.
    fn show_notice(&mut self, text: &str, error: bool, elsewhere: Option<String>) {
        let id = self.panel.next();
        self.panel.notice = Some(Notice { id, text: text.to_string(), error, elsewhere });
        // The panel is hidden, and the notice with it.
        if !self.sidebar_shown {
            eprintln!("{text}");
        }
    }

    /// A note or folder typed in the panel was created, or the person gave up.
    fn note_created(&mut self) {
        self.panel.form = None;
    }

    /// The name typed was refused: the field stays open and says why.
    fn name_refused(&mut self, error: String) {
        match &mut self.panel.form {
            Some(form) => form.error = Some(error),
            None => self.notice(&error, true),
        }
    }

    /// Ctrl+N and the panel's buttons: asks for the name of a new note, or
    /// folder, in `dir`, or else next to the file on the focused side.
    pub(super) fn new_note(&mut self, dir: Option<String>, folder: bool) {
        let dir = dir.unwrap_or_else(|| files::folder_of(self.focused_file().as_deref()));
        let id = self.panel.next();
        self.panel.form = Some(Form { dir, folder, error: None, id });
        self.sidebar_shown = true;
        // Ctrl+N may come while a page has the keyboard.
        self.host.focus_frame();
    }

    /// The tree changed on disk, or what it lists: it is sent again after the event.
    pub(super) fn refresh_tree(&mut self) {
        self.frame.tree_stale = true;
    }

    /// The vault's tree as the panel lists it. Reads the disk.
    pub(super) fn tree(&self) -> Tree {
        let show_other = self.cfg(|c| c.show_other_files != Some(false));
        let Some(v) = self.vault() else { return Tree { vault: None, root: None, open: vec![] } };
        let open = self.cfg(|c| c.open_folders.get(v.path()).cloned().unwrap_or_default());
        Tree { vault: Some(v.path().to_string_lossy().into_owned()), root: Some(files::visible(v.tree(), show_other)), open }
    }

    pub(super) fn files_state(&self) -> State {
        let current = self.vault().map(|v| v.path().to_path_buf());
        let (list, show_other) = self.cfg(|c| (c.vaults.clone(), c.show_other_files != Some(false)));
        let vaults = list
            .iter()
            .map(|p| VaultItem { path: p.to_string_lossy().into_owned(), name: name_of(p), current: current.as_deref() == Some(p.as_path()) })
            .collect();
        State {
            shown: self.sidebar_shown,
            vault: current.as_deref().map(name_of),
            current: self.focused_file(),
            vaults,
            form: self.panel.form.clone(),
            notice: self.panel.notice.clone(),
            width: self.sidebar_width,
            show_other,
        }
    }
}

/// Whether visor shows this file: a page, a PDF or an image.
fn opens(path: &Path) -> bool {
    vault::is_page(path) || vault::is_pdf(path) || vault::is_image(path)
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

    #[test]
    fn opens_pages_pdfs_and_images_only() {
        for name in ["a.md", "a.html", "a.pdf", "a.png", "a.excalidraw.md"] {
            assert!(opens(Path::new(name)), "{name}");
        }
        for name in ["a.zip", "Makefile", "a.xlsx"] {
            assert!(!opens(Path::new(name)), "{name}");
        }
    }
}
