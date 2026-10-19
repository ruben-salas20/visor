use crate::i18n::tr;
use crate::platform;
use crate::trf;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Folders open in the panel's tree, per vault.
    pub open_folders: HashMap<PathBuf, Vec<String>>,
    /// Where the divider between the two sides sits, as a share of their width.
    pub split_ratio: Option<f64>,
    /// Port of the local server, kept so pages keep their origin and storage.
    pub port: Option<u16>,
    pub zoom: Option<f64>,
    /// «claro» or «oscuro» to fix the palette; `None`, or any other value, follows the system.
    pub theme: Option<String>,
    /// `None` means on; `Some(false)` is the only stored off.
    pub auto_update: Option<bool>,
    /// Files visor cannot open, listed in the panel. `None` means shown; `Some(false)` is the only stored off.
    pub show_other_files: Option<bool>,
    /// Engine for text in the address field that is not an address (`address::Engine::id`). `None` is the default.
    pub search_engine: Option<String>,
    /// `"es"` or `"en"`; `None` follows the system's `LANG`. Read at startup, so a change applies on restart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Every open window, reopened at startup.
    pub windows: Vec<WindowState>,
    /// Index in `windows` of the window that had focus.
    pub focused_window: usize,
    /// Last octet of the loopback host each vault is served on, so a vault keeps
    /// its origin and the storage of its pages. 1 is the vault that was `current`
    /// when origins began; 2 is reserved for loose pages.
    pub origins: HashMap<PathBuf, u8>,
}

/// One open window, as it is restored at startup.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WindowState {
    pub vault: Option<PathBuf>,
    pub workspace: Workspace,
    /// Logical size of the window.
    pub size: Option<(u32, u32)>,
    pub sidebar_width: Option<i32>,
    pub split_ratio: Option<f64>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Workspace {
    /// One entry per side, left first.
    pub panes: Vec<PaneState>,
    pub focused: usize,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct PaneState {
    /// What each tab shows: a vault path, a web address, or empty.
    pub tabs: Vec<String>,
    pub active: usize,
}

/// `config.json.bak`, or with the time added if that copy exists: an older
/// copy is never overwritten, since it may hold the only good data.
fn backup_name(path: &Path) -> PathBuf {
    let name = path.file_name().map_or("config.json".into(), |n| n.to_string_lossy().into_owned());
    let first = path.with_file_name(format!("{name}.bak"));
    if first.exists() {
        path.with_file_name(format!("{name}.{}.bak", crate::session::now()))
    } else {
        first
    }
}

impl Config {
    pub fn load()-> Self {
        Self::load_from(&crate::paths::config())
    }

    pub fn save(&self) {
        self.save_to(&crate::paths::config());
    }

    /// A file that exists but cannot be parsed is copied aside before it is
    /// treated as empty, because the next save would overwrite it.
    fn load_from(path: &Path) -> Self {
        let Ok(bytes) = fs::read(path) else { return Self::default() };
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            let copy = backup_name(path);
            match fs::write(&copy, &bytes) {
                Ok(()) => eprintln!(
                    "{}",
                    trf!(
                        "La configuración {} no se puede leer ({e}). Se guardó una copia en {}.",
                        "The settings file {} cannot be read ({e}). A copy was saved to {}.",
                        path.display(),
                        copy.display()
                    )
                ),
                Err(w) => eprintln!(
                    "{}",
                    trf!(
                        "La configuración {} no se puede leer ({e}) y no se pudo guardar una copia ({w}).",
                        "The settings file {} cannot be read ({e}), and a copy could not be saved ({w}).",
                        path.display()
                    )
                ),
            }
            Self::default()
        })
    }

    /// Atomic: a cut mid-write must not leave broken JSON, which `load` would
    /// answer with an empty list of vaults. A vault path that is not UTF-8
    /// cannot be written as JSON; `check_vault` stops those earlier.
    fn save_to(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(self) {
            Ok(json) => {
                let _ = crate::vault::write_atomic(path, json);
            }
            Err(e) => eprintln!("{}", trf!("No se pudo guardar la configuración: {e}", "Could not save the settings: {e}")),
        }
    }

    /// Refuses a folder that the configuration could not keep.
    pub fn check_vault(path: &Path) -> Result<(), String> {
        match path.to_str() {
            Some(_) => Ok(()),
            None => Err(trf!(
                "El nombre de la carpeta no es UTF-8 y no se puede abrir como bóveda: {}",
                "The folder name is not UTF-8 and cannot be opened as a vault: {}",
                path.to_string_lossy()
            )),
        }
    }

    /// Refuses to make the home folder or `/` a vault just because a note or a
    /// drawing was opened from it: that would list and watch all of it. Opening
    /// the folder itself is still allowed.
    pub fn check_folder_of_file(dir: &Path, home: Option<&Path>) -> Result<(), String> {
        let home = home.and_then(|h| platform::canonicalize(h).ok());
        if home.as_deref() == Some(dir) {
            return Err(tr(
                "Abrir este archivo convertiría tu carpeta personal en bóveda. Ábrela a propósito con `visor open ~` o mueve el archivo a una carpeta.",
                "Opening this file would make your home folder a vault. Open the folder on purpose with `visor open ~`, or move the file to a folder.",
            )
            .into());
        }
        if dir.parent().is_none() {
            return Err(tr(
                "Abrir este archivo convertiría la raíz del disco (/) en bóveda. Mueve el archivo a una carpeta.",
                "Opening this file would make the root of the disk (/) a vault. Move the file to a folder.",
            )
            .into());
        }
        Ok(())
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

    /// The windows to open at startup, and which one gets focus. A config from
    /// before several windows has no `windows`: its `current` vault becomes the
    /// one window.
    pub fn restore_windows(&self) -> (Vec<WindowState>, usize) {
        if self.windows.is_empty() {
            let vault = self.current.clone().filter(|c| c.is_dir());
            let workspace = vault.as_ref().and_then(|c| self.workspaces.get(c)).cloned().unwrap_or_default();
            let window = WindowState {
                vault,
                workspace,
                size: None,
                sidebar_width: self.sidebar_width,
                split_ratio: self.split_ratio,
            };
            return (vec![window], 0);
        }
        let windows = self
            .windows
            .iter()
            .map(|w| {
                let mut w = w.clone();
                // A vault that was moved or deleted reopens as an empty window.
                if w.vault.as_ref().is_some_and(|v| !v.is_dir()) {
                    w.vault = None;
                }
                w
            })
            .collect();
        (windows, self.focused_window.min(self.windows.len() - 1))
    }

    /// The loopback host octet of `vault`, given to it the first time it is asked.
    /// Octets 1 and 2 are not handed out: 1 belongs to the old `current` vault and 2 to loose pages.
    /// The caller saves the config.
    pub fn origin_for(&mut self, vault: &Path) -> Option<u8> {
        // Configs from before origins were stored: the vault that was served on 127.0.0.1 keeps it.
        if self.origins.is_empty()
            && let Some(current) = &self.current
        {
            self.origins.insert(current.clone(), 1);
        }
        if let Some(&octet) = self.origins.get(vault) {
            return Some(octet);
        }
        let free = (3..=254u8).find(|o| !self.origins.values().any(|used| used == o))?;
        self.origins.insert(vault.to_path_buf(), free);
        Some(free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("visor-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("nueva/config.json");
        let mut config = Config::default();
        config.add(Path::new("/a/b"));
        config.port = Some(4242);
        config.search_engine = Some("google".into());
        config.language = Some("en".into());
        config.workspaces.insert(
            "/a/b".into(),
            Workspace { panes: vec![PaneState { tabs: vec!["x.md".into()], active: 0 }], focused: 0 },
        );
        config.save_to(&path);
        let loaded = Config::load_from(&path);
        assert_eq!(loaded.vaults, [PathBuf::from("/a/b")]);
        assert_eq!(loaded.current.as_deref(), Some(Path::new("/a/b")));
        assert_eq!(loaded.port, Some(4242));
        assert_eq!(loaded.search_engine.as_deref(), Some("google"));
        assert_eq!(loaded.language.as_deref(), Some("en"));
        assert_eq!(loaded.workspaces[Path::new("/a/b")].panes[0].tabs, ["x.md"]);
        assert!(!dir.join("nueva/.config.json.visor-tmp").exists());
        assert!(Config::load_from(&dir.join("no-existe.json")).vaults.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_config_is_copied_before_it_is_replaced() {
        let dir = std::env::temp_dir().join(format!("visor-config-bak-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{ \"vaults\": [").unwrap();
        assert!(Config::load_from(&path).vaults.is_empty());
        assert_eq!(fs::read_to_string(dir.join("config.json.bak")).unwrap(), "{ \"vaults\": [");
        // A second broken file does not replace the first copy.
        fs::write(&path, "otro").unwrap();
        Config::load_from(&path);
        assert_eq!(fs::read_to_string(dir.join("config.json.bak")).unwrap(), "{ \"vaults\": [");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 3);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_without_search_engine_loads_as_duckduckgo() {
        let dir = std::env::temp_dir().join(format!("visor-config-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, r#"{"vaults": [], "bar_shown": true}"#).unwrap();
        assert!(Config::load_from(&path).search_engine.is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn home_and_root_are_not_made_vaults_by_a_file() {
        let home = std::env::temp_dir().join(format!("visor-config-home-{}", std::process::id()));
        fs::create_dir_all(home.join("apuntes")).unwrap();
        let home = platform::canonicalize(&home).unwrap();
        assert!(Config::check_folder_of_file(&home, Some(&home)).is_err());
        assert!(Config::check_folder_of_file(Path::new("/"), Some(&home)).is_err());
        assert!(Config::check_folder_of_file(&home.join("apuntes"), Some(&home)).is_ok());
        assert!(Config::check_folder_of_file(&home, None).is_ok());
        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_vault_is_refused_and_saving_does_not_panic() {
        use std::os::unix::ffi::OsStringExt;
        let bad = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
        assert!(Config::check_vault(&bad).is_err());
        assert!(Config::check_vault(Path::new("/tmp/ok")).is_ok());
        let dir = std::env::temp_dir().join(format!("visor-config-utf8-{}", std::process::id()));
        let mut config = Config::default();
        config.add(&bad);
        config.save_to(&dir.join("config.json"));
        assert!(!dir.join("config.json").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn config_without_windows_loads_with_none() {
        let dir = std::env::temp_dir().join(format!("visor-config-windows-old-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, r#"{"vaults": ["/a"], "current": "/a", "bar_shown": true}"#).unwrap();
        let loaded = Config::load_from(&path);
        assert!(loaded.windows.is_empty());
        assert_eq!(loaded.focused_window, 0);
        assert!(loaded.origins.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_windows_migrates_the_current_vault() {
        let vault = std::env::temp_dir().join(format!("visor-config-restore-old-{}", std::process::id()));
        fs::create_dir_all(&vault).unwrap();
        let workspace = Workspace { panes: vec![PaneState { tabs: vec!["x.md".into()], active: 0 }], focused: 0 };
        let mut config = Config::default();
        config.add(&vault);
        config.workspaces.insert(vault.clone(), workspace.clone());
        config.sidebar_width = Some(260);
        config.split_ratio = Some(0.4);
        let (windows, focus) = config.restore_windows();
        assert_eq!(focus, 0);
        assert_eq!(
            windows,
            [WindowState {
                vault: Some(vault.clone()),
                workspace,
                size: None,
                sidebar_width: Some(260),
                split_ratio: Some(0.4),
            }]
        );
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn restore_windows_migration_drops_a_missing_current_vault() {
        let config = Config { current: Some(PathBuf::from("/no/existe/visor-config-test")), ..Default::default() };
        let (windows, focus) = config.restore_windows();
        assert_eq!(focus, 0);
        assert_eq!(windows.len(), 1);
        assert!(windows[0].vault.is_none());
    }

    #[test]
    fn restore_windows_uses_saved_windows_and_clamps_focus() {
        let a = std::env::temp_dir().join(format!("visor-config-restore-a-{}", std::process::id()));
        let b = std::env::temp_dir().join(format!("visor-config-restore-b-{}", std::process::id()));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let gone = PathBuf::from("/no/existe/visor-config-gone");
        let mut config = Config {
            windows: vec![
                WindowState { vault: Some(a.clone()), size: Some((1200, 800)), ..Default::default() },
                WindowState { vault: Some(gone.clone()), sidebar_width: Some(300), ..Default::default() },
                WindowState { vault: Some(b.clone()), ..Default::default() },
            ],
            focused_window: 1,
            ..Default::default()
        };
        let (windows, focus) = config.restore_windows();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].vault.as_ref(), Some(&a));
        assert_eq!(windows[0].size, Some((1200, 800)));
        // A missing vault is dropped, but the window and the rest of its state stay.
        assert!(windows[1].vault.is_none());
        assert_eq!(windows[1].sidebar_width, Some(300));
        assert_eq!(windows[2].vault.as_ref(), Some(&b));
        assert_eq!(focus, 1);

        config.focused_window = 99;
        assert_eq!(config.restore_windows().1, 2);
        fs::remove_dir_all(a).unwrap();
        fs::remove_dir_all(b).unwrap();
    }

    #[test]
    fn origin_for_keeps_the_old_current_on_127_0_0_1_and_never_uses_1_or_2() {
        let mut config = Config { current: Some(PathBuf::from("/vaults/a")), ..Default::default() };
        assert_eq!(config.origin_for(Path::new("/vaults/a")), Some(1));
        assert_eq!(config.origin_for(Path::new("/vaults/b")), Some(3));
        assert_eq!(config.origin_for(Path::new("/vaults/c")), Some(4));
        // Asking again gives the same answer and does not take a new octet.
        assert_eq!(config.origin_for(Path::new("/vaults/b")), Some(3));
        assert_eq!(config.origin_for(Path::new("/vaults/a")), Some(1));
        assert_eq!(config.origins.len(), 3);
    }

    #[test]
    fn origin_for_returns_none_when_all_octets_are_taken() {
        let mut config = Config::default();
        for octet in 3..=254u8 {
            config.origins.insert(PathBuf::from(format!("/vaults/{octet}")), octet);
        }
        assert_eq!(config.origin_for(Path::new("/vaults/nueva")), None);
        assert!(!config.origins.contains_key(Path::new("/vaults/nueva")));
        // A vault that already has an octet still gets it.
        assert_eq!(config.origin_for(Path::new("/vaults/254")), Some(254));
    }

    #[test]
    fn windows_and_origins_survive_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("visor-config-windows-rt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("config.json");
        let mut config = Config {
            windows: vec![WindowState {
                vault: Some(PathBuf::from("/a")),
                workspace: Workspace { panes: vec![PaneState { tabs: vec!["n.md".into()], active: 0 }], focused: 0 },
                size: Some((900, 700)),
                sidebar_width: Some(250),
                split_ratio: Some(0.55),
            }],
            ..Default::default()
        };
        config.origins.insert(PathBuf::from("/a"), 1);
        config.save_to(&path);
        let loaded = Config::load_from(&path);
        assert_eq!(loaded.windows, config.windows);
        assert_eq!(loaded.focused_window, 0);
        assert_eq!(loaded.origins, config.origins);
        fs::remove_dir_all(dir).unwrap();
    }
}
