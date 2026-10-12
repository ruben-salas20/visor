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
    pub bar_shown: bool,
    pub zoom: Option<f64>,
    /// «claro» or «oscuro» to fix the palette; `None`, or any other value, follows the system.
    pub theme: Option<String>,
    /// `None` means on; `Some(false)` is the only stored off.
    pub auto_update: Option<bool>,
    /// Files visor cannot open, listed in the panel. `None` means shown; `Some(false)` is the only stored off.
    pub show_other_files: Option<bool>,
    /// Engine for text in the address field that is not an address (`address::Engine::id`). `None` is the default.
    pub search_engine: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Workspace {
    /// One entry per side, left first.
    pub panes: Vec<PaneState>,
    pub focused: usize,
}

#[derive(Serialize, Deserialize, Default, Clone)]
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
                Ok(()) => eprintln!("La configuración {} no se puede leer ({e}). Se guardó una copia en {}.", path.display(), copy.display()),
                Err(w) => eprintln!("La configuración {} no se puede leer ({e}) y no se pudo guardar una copia ({w}).", path.display()),
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
            Err(e) => eprintln!("No se pudo guardar la configuración: {e}"),
        }
    }

    /// Refuses a folder that the configuration could not keep.
    pub fn check_vault(path: &Path) -> Result<(), String> {
        match path.to_str() {
            Some(_) => Ok(()),
            None => Err(format!("El nombre de la carpeta no es UTF-8 y no se puede abrir como bóveda: {}", path.to_string_lossy())),
        }
    }

    /// Refuses to make the home folder or `/` a vault just because a note or a
    /// drawing was opened from it: that would list and watch all of it. Opening
    /// the folder itself is still allowed.
    pub fn check_folder_of_file(dir: &Path, home: Option<&Path>) -> Result<(), String> {
        let home = home.and_then(|h| h.canonicalize().ok());
        if home.as_deref() == Some(dir) {
            return Err("Abrir este archivo convertiría tu carpeta personal en bóveda. Ábrela a propósito con `visor open ~` o mueve el archivo a una carpeta.".into());
        }
        if dir.parent().is_none() {
            return Err("Abrir este archivo convertiría la raíz del disco (/) en bóveda. Mueve el archivo a una carpeta.".into());
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
        config.bar_shown = true;
        config.search_engine = Some("google".into());
        config.workspaces.insert(
            "/a/b".into(),
            Workspace { panes: vec![PaneState { tabs: vec!["x.md".into()], active: 0 }], focused: 0 },
        );
        config.save_to(&path);
        let loaded = Config::load_from(&path);
        assert_eq!(loaded.vaults, [PathBuf::from("/a/b")]);
        assert_eq!(loaded.current.as_deref(), Some(Path::new("/a/b")));
        assert_eq!(loaded.port, Some(4242));
        assert!(loaded.bar_shown);
        assert_eq!(loaded.search_engine.as_deref(), Some("google"));
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
        let home = home.canonicalize().unwrap();
        assert!(Config::check_folder_of_file(&home, Some(&home)).is_err());
        assert!(Config::check_folder_of_file(Path::new("/"), Some(&home)).is_err());
        assert!(Config::check_folder_of_file(&home.join("apuntes"), Some(&home)).is_ok());
        assert!(Config::check_folder_of_file(&home, None).is_ok());
        fs::remove_dir_all(home).unwrap();
    }

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
}
