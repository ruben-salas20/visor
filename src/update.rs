//! Installs the newest GitHub release. The background check runs once a day at
//! most; the button and `visor update` check at once. Shells out to curl and
//! sha256sum, so no HTTPS crate is needed. See docs/adr/0035.
use serde::Deserialize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const RELEASES: &str = "https://api.github.com/repos/ruben-salas20/visor/releases/latest";
const ASSET: &str = "visor-x86_64-linux";
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Two checks at once would share the same temporary file name.
static RUNNING: Mutex<()> = Mutex::new(());
/// The version this process put on disk, so a second check does not fetch it again.
static INSTALLED: Mutex<Option<String>> = Mutex::new(None);

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// «sha256:<hex>», as GitHub reports it.
    digest: Option<String>,
}

/// How a check ended. `message` is the text the panel and `visor update` show.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The running version, when nothing newer exists.
    Latest(String),
    /// The version now on disk; the running process still has the old one.
    Installed(String),
    /// Nothing was checked, and why.
    Skipped(String),
    Failed(String),
}

impl Outcome {
    pub fn message(&self) -> String {
        match self {
            Self::Latest(version) => format!("Ya tienes la última versión ({version})."),
            Self::Installed(version) => format!("Instalada la versión {version}. Se usará al volver a abrir visor."),
            Self::Skipped(reason) | Self::Failed(reason) => reason.clone(),
        }
    }
}

pub fn spawn() {
    std::thread::spawn(|| {
        if let Outcome::Installed(version) = run(false) {
            eprintln!("visor se actualizó a la versión {version}. Se usará al volver a abrirlo.");
        }
    });
}

/// `force` skips the once-a-day limit; the other refusals apply to both.
pub fn run(force: bool) -> Outcome {
    let Ok(_running) = RUNNING.try_lock() else {
        return Outcome::Skipped("Ya se está buscando una actualización.".into());
    };
    let Some(exe) = running_exe() else {
        return Outcome::Failed("No se encuentra el archivo de visor.".into());
    };
    let exe = exe.canonicalize().unwrap_or(exe);
    let no_update = std::env::var_os("VISOR_NO_UPDATE").is_some();
    if let Some(reason) = refusal(no_update, std::env::consts::ARCH, &exe) {
        return Outcome::Skipped(reason);
    }
    let check = crate::paths::update_check();
    if !force && !due(&check) {
        return Outcome::Skipped("Ya se buscó hoy.".into());
    }
    // Written before the network call, so a failure also waits a day.
    let written = check.parent().is_some_and(|dir| fs::create_dir_all(dir).is_ok()) && fs::write(&check, b"").is_ok();
    if !written {
        return Outcome::Failed("No se pudo guardar la fecha de la búsqueda.".into());
    }

    let Some(json) = curl(&["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", RELEASES]) else {
        return Outcome::Failed("No se pudo consultar GitHub. Revisa la conexión.".into());
    };
    let Ok(release) = serde_json::from_slice::<Release>(&json) else {
        return Outcome::Failed("La respuesta de GitHub no se entiende.".into());
    };
    let installed = INSTALLED.lock().unwrap().clone();
    if let Some(known) = nothing_to_fetch(&release.tag_name, env!("CARGO_PKG_VERSION"), installed) {
        return known;
    }
    let version = release.tag_name.trim_start_matches('v').to_string();
    let Some(asset) = release.assets.into_iter().find(|a| a.name == ASSET) else {
        return Outcome::Failed(format!("La versión {version} no trae el archivo {ASSET}."));
    };
    let Some(sha256) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) else {
        return Outcome::Failed(format!("La versión {version} no trae su SHA-256; no se instala sin comprobarlo."));
    };

    // Beside the binary, so the rename stays on one filesystem and is atomic.
    let Some(dir) = exe.parent() else {
        return Outcome::Failed("No se encuentra la carpeta de visor.".into());
    };
    let tmp = dir.join(format!(".visor-nuevo-{}", std::process::id()));
    match install(&asset.browser_download_url, sha256, &tmp, &exe) {
        Ok(()) => {
            *INSTALLED.lock().unwrap() = Some(version.clone());
            Outcome::Installed(version)
        }
        Err(reason) => {
            let _ = fs::remove_file(&tmp);
            Outcome::Failed(reason)
        }
    }
}

/// What to answer without downloading, when `remote` is not newer than what is
/// on disk: the version this process installed earlier, or else the one it runs.
fn nothing_to_fetch(remote: &str, running: &str, installed: Option<String>) -> Option<Outcome> {
    match installed {
        Some(version) if !is_newer(remote, &version) => Some(Outcome::Installed(version)),
        None if !is_newer(remote, running) => Some(Outcome::Latest(running.into())),
        _ => None,
    }
}

/// The file this process runs from. After an update Linux reports the old file
/// as "<path> (deleted)", and the new one sits at <path>; restarting needs that one.
pub fn running_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let stripped = exe.to_str().and_then(|p| p.strip_suffix(" (deleted)")).map(PathBuf::from);
    Some(stripped.unwrap_or(exe))
}

/// Why the binary at `exe` must not be replaced, if it must not.
fn refusal(no_update: bool, arch: &str, exe: &Path) -> Option<String> {
    if no_update {
        return Some("Las actualizaciones están desactivadas con VISOR_NO_UPDATE.".into());
    }
    if arch != "x86_64" {
        return Some("Las versiones publicadas son solo para Linux x86_64.".into());
    }
    // A cargo build must never be replaced by a release.
    if exe.components().any(|c| c.as_os_str() == "target") {
        return Some("Esta copia está en target/ (compilación de desarrollo) y no se reemplaza.".into());
    }
    None
}

/// True when the last check is older than a day, or cannot be read.
fn due(check: &Path) -> bool {
    let Ok(modified) = fs::metadata(check).and_then(|m| m.modified()) else { return true };
    SystemTime::now().duration_since(modified).map_or(true, |age| age >= EVERY)
}

fn install(url: &str, sha256: &str, tmp: &Path, exe: &Path) -> Result<(), String> {
    let downloaded = Command::new("curl")
        .args(["-fsSL", "--max-time", "300", "-o"])
        .arg(tmp)
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !downloaded.is_ok_and(|s| s.success()) {
        return Err("No se pudo descargar la nueva versión.".into());
    }
    let mismatch = "La descarga no coincide con su SHA-256. No se ha instalado nada.";
    let out = Command::new("sha256sum").arg(tmp).stderr(Stdio::null()).output().map_err(|_| mismatch)?;
    let got = String::from_utf8(out.stdout).map_err(|_| mismatch)?;
    let same = got.split_whitespace().next().is_some_and(|h| h.eq_ignore_ascii_case(sha256));
    if !out.status.success() || !same {
        return Err(mismatch.into());
    }
    fs::set_permissions(tmp, fs::Permissions::from_mode(0o755))
        .and_then(|()| fs::rename(tmp, exe))
        .map_err(|e| format!("No se pudo reemplazar visor: {e}."))
}

fn curl(args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("curl").args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// Dotted numbers after an optional «v», compared part by part: 0.1.10 > 0.1.9.
/// Anything else is never newer.
fn is_newer(remote: &str, local: &str) -> bool {
    fn numbers(v: &str) -> Option<Vec<u64>> {
        v.strip_prefix('v').unwrap_or(v).split('.').map(|p| p.parse().ok()).collect()
    }
    match (numbers(remote), numbers(local)) {
        (Some(remote), Some(local)) => remote > local,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_by_parts() {
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(is_newer("0.2", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.9", "0.1.9"));
        assert!(!is_newer("v0.1.8", "0.1.9"));
        assert!(!is_newer("nightly", "0.1.9"));
        assert!(!is_newer("0.2.x", "0.1.9"));
        assert!(!is_newer("", "0.1.9"));
    }

    #[test]
    fn refuses_to_replace_a_build_or_an_unsupported_system() {
        let installed = Path::new("/home/ruben/.local/bin/visor");
        let build = Path::new("/home/ruben/Projects/visor/target/release/visor");
        assert_eq!(refusal(false, "x86_64", installed), None);
        assert!(refusal(false, "x86_64", build).unwrap().contains("target/"));
        assert!(refusal(false, "aarch64", installed).unwrap().contains("x86_64"));
        assert!(refusal(true, "x86_64", installed).unwrap().contains("VISOR_NO_UPDATE"));
    }

    #[test]
    fn a_version_installed_earlier_is_not_fetched_again() {
        assert_eq!(nothing_to_fetch("v0.1.0", "0.1.0", None), Some(Outcome::Latest("0.1.0".into())));
        assert_eq!(nothing_to_fetch("v0.2.0", "0.1.0", None), None);
        let installed = Some("0.2.0".to_string());
        assert_eq!(nothing_to_fetch("v0.2.0", "0.1.0", installed.clone()), Some(Outcome::Installed("0.2.0".into())));
        assert_eq!(nothing_to_fetch("v0.3.0", "0.1.0", installed), None);
    }

    #[test]
    fn says_what_happened() {
        assert_eq!(Outcome::Latest("0.1.9".into()).message(), "Ya tienes la última versión (0.1.9).");
        assert_eq!(
            Outcome::Installed("0.2.0".into()).message(),
            "Instalada la versión 0.2.0. Se usará al volver a abrir visor."
        );
        assert_eq!(Outcome::Failed("Sin red.".into()).message(), "Sin red.");
        assert_eq!(Outcome::Skipped("Ya se buscó hoy.".into()).message(), "Ya se buscó hoy.");
    }

    #[test]
    fn a_missing_download_is_a_failure_with_a_reason() {
        let dir = std::env::temp_dir().join(format!("visor-update-miss-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (exe, tmp) = (dir.join("visor"), dir.join(".visor-nuevo"));
        fs::write(&exe, b"vieja").unwrap();
        let url = format!("file://{}", dir.join("no-existe").display());
        let err = install(&url, "00", &tmp, &exe).unwrap_err();
        assert!(err.contains("descargar"), "{err}");
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn installs_only_a_matching_download() {
        let dir = std::env::temp_dir().join(format!("visor-update-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (new, exe, tmp) = (dir.join("new"), dir.join("visor"), dir.join(".visor-nuevo"));
        fs::write(&new, b"nueva").unwrap();
        fs::write(&exe, b"vieja").unwrap();
        let url = format!("file://{}", new.display());
        let sha = "8e8a2d1b5c5a2a6f0f0c1e1f63ee2b7ca5ba8b3a4b0e35a45dd2d8f8b6a0b0e3";
        assert!(install(&url, sha, &tmp, &exe).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        let out = Command::new("sha256sum").arg(&new).output().unwrap();
        let sha = String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_string();
        assert!(install(&url, &sha, &tmp, &exe).is_ok());
        assert_eq!(fs::read(&exe).unwrap(), b"nueva");
        assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
        let _ = fs::remove_dir_all(&dir);
    }
}
