//! Installs the newest GitHub release in the background, once a day at most.
//! Shells out to curl and sha256sum, so no HTTPS crate is needed. See docs/adr/0035.
use serde::Deserialize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

const RELEASES: &str = "https://api.github.com/repos/ruben-salas20/visor/releases/latest";
const ASSET: &str = "visor-x86_64-linux";
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

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

pub fn spawn() {
    std::thread::spawn(|| {
        if let Some(version) = run() {
            eprintln!("visor se actualizó a la versión {version}. Se usará al volver a abrirlo.");
        }
    });
}

/// The version installed, or `None` for every reason to do nothing.
fn run() -> Option<String> {
    if std::env::var_os("VISOR_NO_UPDATE").is_some() || std::env::consts::ARCH != "x86_64" {
        return None;
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    // A cargo build must never be replaced by a release.
    if exe.components().any(|c| c.as_os_str() == "target") {
        return None;
    }
    let check = crate::paths::update_check();
    if !due(&check) {
        return None;
    }
    // Written before the network call, so a failure also waits a day.
    fs::create_dir_all(check.parent()?).ok()?;
    fs::write(&check, b"").ok()?;

    let json = curl(&["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", RELEASES])?;
    let release: Release = serde_json::from_slice(&json).ok()?;
    if !is_newer(&release.tag_name, env!("CARGO_PKG_VERSION")) {
        return None;
    }
    let asset = release.assets.into_iter().find(|a| a.name == ASSET)?;
    let sha256 = asset.digest?.strip_prefix("sha256:")?.to_string();

    // Beside the binary, so the rename stays on one filesystem and is atomic.
    let tmp = exe.parent()?.join(format!(".visor-nuevo-{}", std::process::id()));
    let installed = install(&asset.browser_download_url, &sha256, &tmp, &exe);
    if installed.is_none() {
        let _ = fs::remove_file(&tmp);
    }
    installed.map(|()| release.tag_name.trim_start_matches('v').to_string())
}

/// True when the last check is older than a day, or cannot be read.
fn due(check: &Path) -> bool {
    let Ok(modified) = fs::metadata(check).and_then(|m| m.modified()) else { return true };
    SystemTime::now().duration_since(modified).map_or(true, |age| age >= EVERY)
}

fn install(url: &str, sha256: &str, tmp: &Path, exe: &Path) -> Option<()> {
    let downloaded = Command::new("curl")
        .args(["-fsSL", "--max-time", "300", "-o"])
        .arg(tmp)
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !downloaded.success() {
        return None;
    }
    let out = Command::new("sha256sum").arg(tmp).stderr(Stdio::null()).output().ok()?;
    let got = String::from_utf8(out.stdout).ok()?;
    if !out.status.success() || !got.split_whitespace().next()?.eq_ignore_ascii_case(sha256) {
        return None;
    }
    fs::set_permissions(tmp, fs::Permissions::from_mode(0o755)).ok()?;
    fs::rename(tmp, exe).ok()
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
    fn installs_only_a_matching_download() {
        let dir = std::env::temp_dir().join(format!("visor-update-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (new, exe, tmp) = (dir.join("new"), dir.join("visor"), dir.join(".visor-nuevo"));
        fs::write(&new, b"nueva").unwrap();
        fs::write(&exe, b"vieja").unwrap();
        let url = format!("file://{}", new.display());
        let sha = "8e8a2d1b5c5a2a6f0f0c1e1f63ee2b7ca5ba8b3a4b0e35a45dd2d8f8b6a0b0e3";
        assert!(install(&url, sha, &tmp, &exe).is_none());
        assert_eq!(fs::read(&exe).unwrap(), b"vieja");
        let out = Command::new("sha256sum").arg(&new).output().unwrap();
        let sha = String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_string();
        assert!(install(&url, &sha, &tmp, &exe).is_some());
        assert_eq!(fs::read(&exe).unwrap(), b"nueva");
        assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
        let _ = fs::remove_dir_all(&dir);
    }
}
