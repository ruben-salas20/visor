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
    /// answer with an empty list of vaults.
    fn save_to(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = crate::vault::write_atomic(path, serde_json::to_vec_pretty(self).unwrap());
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
}
