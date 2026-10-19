//! Installs the newest GitHub release. The background check runs once a day at
//! most; the button and `visor update` check at once. Shells out to curl, so no
//! HTTPS crate is needed; SHA-256 uses sha256sum on Linux and the sha2 crate on
//! Windows, which has no sha256sum. See docs/adr/0035.
use crate::i18n::tr;
use crate::trf;
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const RELEASES: &str = "https://api.github.com/repos/ruben-salas20/visor/releases/latest";
#[cfg(not(windows))]
const ASSET: &str = "visor-x86_64-linux";
#[cfg(windows)]
const ASSET: &str = "visor-x86_64-windows.exe";
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
            Self::Latest(version) => trf!("Ya tienes la última versión ({}).", "You already have the latest version ({}).", version),
            Self::Installed(version) => trf!(
                "Instalada la versión {}. Se usará al volver a abrir visor.",
                "Version {} is installed. It will be used when visor is opened again.",
                version
            ),
            Self::Skipped(reason) | Self::Failed(reason) => reason.clone(),
        }
    }
}

pub fn spawn() {
    std::thread::spawn(|| {
        if let Outcome::Installed(version) = run(false) {
            eprintln!(
                "{}",
                trf!(
                    "visor se actualizó a la versión {}. Se usará al volver a abrirlo.",
                    "visor was updated to version {}. It will be used when it is opened again.",
                    version
                )
            );
        }
    });
}

/// `force` skips the once-a-day limit; the other refusals apply to both.
pub fn run(force: bool) -> Outcome {
    let Ok(_running) = RUNNING.try_lock() else {
        return Outcome::Skipped(tr("Ya se está buscando una actualización.", "An update check is already running.").into());
    };
    let Some(exe) = running_exe() else {
        return Outcome::Failed(tr("No se encuentra el archivo de visor.", "The visor file was not found.").into());
    };
    let exe = exe.canonicalize().unwrap_or(exe);
    let no_update = std::env::var_os("VISOR_NO_UPDATE").is_some();
    if let Some(reason) = refusal(no_update, std::env::consts::ARCH, &exe) {
        return Outcome::Skipped(reason);
    }
    let check = crate::paths::update_check();
    if !force && !due(&check) {
        return Outcome::Skipped(tr("Ya se buscó hoy.", "Already checked today.").into());
    }
    // Written before the network call, so a failure also waits a day.
    let written = check.parent().is_some_and(|dir| fs::create_dir_all(dir).is_ok()) && fs::write(&check, b"").is_ok();
    if !written {
        return Outcome::Failed(tr("No se pudo guardar la fecha de la búsqueda.", "Could not save the date of the check.").into());
    }

    let Some(json) = curl(&["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", RELEASES]) else {
        return Outcome::Failed(tr("No se pudo consultar GitHub. Revisa la conexión.", "Could not reach GitHub. Check your connection.").into());
    };
    let Ok(release) = serde_json::from_slice::<Release>(&json) else {
        return Outcome::Failed(tr("La respuesta de GitHub no se entiende.", "GitHub's response could not be understood.").into());
    };
    let installed = INSTALLED.lock().unwrap().clone();
    if let Some(known) = nothing_to_fetch(&release.tag_name, env!("CARGO_PKG_VERSION"), installed) {
        return known;
    }
    let version = release.tag_name.trim_start_matches('v').to_string();
    let Some(asset) = release.assets.into_iter().find(|a| a.name == ASSET) else {
        return Outcome::Failed(trf!("La versión {} no trae el archivo {}.", "Version {} does not include the file {}.", version, ASSET));
    };
    let Some(sha256) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) else {
        return Outcome::Failed(trf!(
            "La versión {} no trae su SHA-256; no se instala sin comprobarlo.",
            "Version {} does not include its SHA-256; it is not installed without checking it.",
            version
        ));
    };

    // Beside the binary, so the rename stays on one filesystem and is atomic.
    let Some(dir) = exe.parent() else {
        return Outcome::Failed(tr("No se encuentra la carpeta de visor.", "The visor folder was not found.").into());
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
        return Some(tr("Las actualizaciones están desactivadas con VISOR_NO_UPDATE.", "Updates are turned off with VISOR_NO_UPDATE.").into());
    }
    if arch != "x86_64" {
        let text = if cfg!(windows) {
            tr("Las versiones publicadas son solo para Windows x86_64.", "Published versions are for Windows x86_64 only.")
        } else {
            tr("Las versiones publicadas son solo para Linux x86_64.", "Published versions are for Linux x86_64 only.")
        };
        return Some(text.into());
    }
    if crate::package::managed(exe) {
        return Some(tr("Esta copia de visor la actualiza su paquete (apt o Flatpak).", "This copy of visor is updated by its package (apt or Flatpak).").into());
    }
    // A cargo build must never be replaced by a release.
    if exe.components().any(|c| c.as_os_str() == "target") {
        return Some(tr("Esta copia está en la carpeta target (compilación de desarrollo) y no se reemplaza.", "This copy is in the target folder (development build) and is not replaced.").into());
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
        return Err(tr("No se pudo descargar la nueva versión.", "Could not download the new version.").into());
    }
    let mismatch = tr("La descarga no coincide con su SHA-256. No se ha instalado nada.", "The download does not match its SHA-256. Nothing was installed.");
    if !sha256_of(tmp).is_some_and(|h| h.eq_ignore_ascii_case(sha256)) {
        return Err(mismatch.into());
    }
    crate::platform::make_executable(tmp)
        .and_then(|()| put_in_place(tmp, exe))
        .map_err(|e| trf!("No se pudo reemplazar visor: {}.", "Could not replace visor: {}.", e))
}

/// The SHA-256 of the file at `path`, in lowercase hex.
#[cfg(not(windows))]
fn sha256_of(path: &Path) -> Option<String> {
    let out = Command::new("sha256sum").arg(path).stderr(Stdio::null()).output().ok()?;
    let hash = String::from_utf8(out.stdout).ok()?;
    let hash = hash.split_whitespace().next()?.to_ascii_lowercase();
    out.status.success().then_some(hash)
}

#[cfg(windows)]
fn sha256_of(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let data = fs::read(path).ok()?;
    let hash = Sha256::digest(&data);
    Some(hash.iter().map(|b| format!("{b:02x}")).collect())
}

/// Puts the downloaded file at `exe`. Linux replaces it in one atomic rename,
/// even while it runs.
#[cfg(not(windows))]
fn put_in_place(tmp: &Path, exe: &Path) -> io::Result<()> {
    fs::rename(tmp, exe)
}

/// Windows cannot overwrite or delete a running .exe, but it can rename one. So
/// the current file moves aside to `.old`, and the new one takes its place.
#[cfg(windows)]
fn put_in_place(tmp: &Path, exe: &Path) -> io::Result<()> {
    let old = old_copy(exe);
    if old.exists() && fs::remove_file(&old).is_err() {
        return Err(io::Error::other(tr(
            "la copia anterior sigue en uso; cierra visor y vuelve a intentarlo",
            "the previous copy is still in use; close visor and try again",
        )));
    }
    fs::rename(exe, &old)?;
    if let Err(e) = fs::rename(tmp, exe) {
        // Put the current file back, so that visor.exe is never missing.
        let _ = fs::rename(&old, exe);
        return Err(e);
    }
    Ok(())
}

/// Where Windows keeps the previous visor.exe after an update.
#[cfg(windows)]
fn old_copy(exe: &Path) -> PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

/// Deletes the `.old` file that the last update left beside visor. The process
/// that ran the update keeps it locked, so the next start usually deletes it. If
/// it is still locked, it stays for the start after that.
#[cfg(windows)]
pub fn remove_leftover() {
    if let Some(exe) = running_exe() {
        remove_old_copy(&exe);
    }
}

#[cfg(windows)]
fn remove_old_copy(exe: &Path) {
    let _ = fs::remove_file(old_copy(exe));
}

/// Only Windows leaves a `.old` file. On Linux, `visor.old` could be a user's file.
#[cfg(not(windows))]
pub fn remove_leftover() {}

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
        assert!(refusal(false, "x86_64", build).unwrap().contains("target"));
        assert!(refusal(false, "aarch64", installed).unwrap().contains("x86_64"));
        assert!(refusal(true, "x86_64", installed).unwrap().contains("VISOR_NO_UPDATE"));
        assert!(refusal(false, "x86_64", Path::new("/usr/bin/visor")).unwrap().contains("paquete"));
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

    /// A file:// URL that curl reads on this system. Windows paths need file:///C:/…
    fn file_url(path: &Path) -> String {
        let p = path.display().to_string().replace('\\', "/");
        if p.starts_with('/') { format!("file://{p}") } else { format!("file:///{p}") }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_download_is_a_failure_with_a_reason() {
        let dir = scratch("miss");
        let (exe, tmp) = (dir.join("visor"), dir.join(".visor-nuevo"));
        fs::write(&exe, b"vieja").unwrap();
        let err = install(&file_url(&dir.join("no-existe")), "00", &tmp, &exe).unwrap_err();
        assert!(err.contains("descargar"), "{err}");
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn installs_only_a_matching_download() {
        let dir = scratch("install");
        let (new, exe, tmp) = (dir.join("new"), dir.join("visor"), dir.join(".visor-nuevo"));
        fs::write(&new, b"nueva").unwrap();
        fs::write(&exe, b"vieja").unwrap();
        let url = file_url(&new);
        let sha = "8e8a2d1b5c5a2a6f0f0c1e1f63ee2b7ca5ba8b3a4b0e35a45dd2d8f8b6a0b0e3";
        assert!(install(&url, sha, &tmp, &exe).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        let sha = sha256_of(&new).unwrap();
        assert!(install(&url, &sha, &tmp, &exe).is_ok());
        assert_eq!(fs::read(&exe).unwrap(), b"nueva");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hashes_a_file_like_sha256sum() {
        // The SHA-256 of "abc" (FIPS 180-2 test vector).
        let dir = scratch("hash");
        let file = dir.join("abc");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(sha256_of(&file).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_moves_the_running_exe_aside_and_removes_an_old_copy() {
        let dir = scratch("swap");
        let (exe, tmp) = (dir.join("visor.exe"), dir.join(".visor-nuevo"));
        fs::write(&exe, b"vieja").unwrap();
        fs::write(&tmp, b"nueva").unwrap();
        fs::write(old_copy(&exe), b"anterior").unwrap();
        put_in_place(&tmp, &exe).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"nueva");
        assert_eq!(fs::read(old_copy(&exe)).unwrap(), b"vieja");
        assert!(!tmp.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_puts_the_old_exe_back_when_the_new_one_cannot_move_in() {
        let dir = scratch("rollback");
        let (exe, tmp) = (dir.join("visor.exe"), dir.join("no-existe"));
        fs::write(&exe, b"vieja").unwrap();
        assert!(put_in_place(&tmp, &exe).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        assert!(!old_copy(&exe).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_an_old_copy_that_is_still_in_use_and_refuses_to_replace_it() {
        let dir = scratch("locked");
        let (exe, tmp) = (dir.join("visor.exe"), dir.join(".visor-nuevo"));
        fs::write(&exe, b"vieja").unwrap();
        fs::write(&tmp, b"nueva").unwrap();
        fs::write(old_copy(&exe), b"anterior").unwrap();
        // Read sharing only: a running program keeps its file this way.
        // File::open would also share deletes, and then the copy could be removed.
        let held = {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_READ: u32 = 0x1;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .open(old_copy(&exe))
                .unwrap()
        };
        assert!(put_in_place(&tmp, &exe).is_err());
        remove_old_copy(&exe);
        assert!(old_copy(&exe).exists());
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        drop(held);
        remove_old_copy(&exe);
        assert!(!old_copy(&exe).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_refuses_a_cargo_build_by_its_path() {
        let build = Path::new(r"C:\Users\ruben\Projects\visor\target\release\visor.exe");
        let installed = Path::new(r"C:\Users\ruben\AppData\Local\Programs\visor\visor.exe");
        assert!(refusal(false, "x86_64", build).unwrap().contains("target"));
        assert_eq!(refusal(false, "x86_64", installed), None);
    }
}
