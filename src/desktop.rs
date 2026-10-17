//! The `visor.desktop` entry that lets the system offer visor for `.md` and
//! `.html` files, and the icon it names. Written at startup, from a background thread.

use crate::vault;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The program a `.desktop` entry runs. Linux appends `(deleted)` to the path
/// of a binary replaced while it runs; the new one sits at the path without it.
pub fn program(current: &Path) -> PathBuf {
    match current.to_str().and_then(|p| p.strip_suffix(" (deleted)")) {
        Some(path) => PathBuf::from(path),
        None => current.to_path_buf(),
    }
}

/// The text of the entry, for the binary at `exe`.
pub fn entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=visor\n\
         Icon=visor\n\
         Exec={} open --loose %f\n\
         Terminal=false\n\
         MimeType=text/markdown;text/html;\n",
        exec_arg(&exe.to_string_lossy())
    )
}

/// `Exec` takes its program quoted when it has spaces or reserved characters,
/// with `"`, `` ` ``, `$` and `\` escaped inside; a literal `%` is written `%%`.
fn exec_arg(path: &str) -> String {
    let reserved = |c: char| c.is_whitespace() || "\"'\\><~|&;*?#()$`".contains(c);
    let escaped = path.replace('%', "%%");
    if !escaped.chars().any(reserved) {
        return escaped;
    }
    let inside: String = escaped
        .chars()
        .flat_map(|c| if "\"`$\\".contains(c) { vec!['\\', c] } else { vec![c] })
        .collect();
    format!("\"{inside}\"")
}

/// visor's symbol, which the entry names as `Icon=visor`.
pub const ICON: &str = include_str!("../docs/img/logo/simbolo.svg");

/// Whether the file that holds `current` must be written again to hold `wanted`.
pub fn needs_write(current: Option<&str>, wanted: &str) -> bool {
    current != Some(wanted)
}

/// Brings the entry at `file` up to date with the binary `exe`. Returns whether
/// it wrote the file. Does nothing if `exe` is not a file (a binary removed
/// and not yet replaced).
pub fn refresh(exe: &Path, file: &Path) -> io::Result<bool> {
    let exe = program(exe);
    if !exe.is_file() {
        return Ok(false);
    }
    write_if_changed(file, &entry(&exe))
}

/// Writes `wanted` to `file` unless it already holds it. Returns whether it wrote.
pub fn write_if_changed(file: &Path, wanted: &str) -> io::Result<bool> {
    let current = fs::read_to_string(file).ok();
    if !needs_write(current.as_deref(), wanted) {
        return Ok(false);
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    vault::write_atomic(file, wanted)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-desktop-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn entry_opens_loose_files_for_markdown_and_html() {
        let text = entry(Path::new("/home/ana/bin/visor"));
        assert!(text.starts_with("[Desktop Entry]\n"));
        assert!(text.contains("\nExec=/home/ana/bin/visor open --loose %f\n"), "{text}");
        assert!(text.contains("\nMimeType=text/markdown;text/html;\n"));
        assert!(text.contains("\nIcon=visor\n"), "the menu and «Open with» show visor's symbol");
        assert!(text.contains("\nTerminal=false\n") && text.contains("\nType=Application\n") && text.contains("\nName=visor\n"));
    }

    #[test]
    fn exec_quotes_paths_with_spaces_and_escapes_inside() {
        assert_eq!(exec_arg("/opt/visor"), "/opt/visor");
        assert_eq!(exec_arg("/opt/mis apps/visor"), "\"/opt/mis apps/visor\"");
        assert_eq!(exec_arg("/x/a$b\"c\\d`e"), "\"/x/a\\$b\\\"c\\\\d\\`e\"");
        assert_eq!(exec_arg("/x/100%"), "/x/100%%", "a percent sign is doubled");
    }

    #[test]
    fn deleted_binary_runs_from_its_path_again() {
        assert_eq!(program(Path::new("/home/ana/bin/visor (deleted)")), PathBuf::from("/home/ana/bin/visor"));
        assert_eq!(program(Path::new("/home/ana/bin/visor")), PathBuf::from("/home/ana/bin/visor"));
    }

    #[test]
    fn writes_only_when_the_content_changes() {
        assert!(needs_write(None, "x"), "no file yet");
        assert!(needs_write(Some("y"), "x"), "other content");
        assert!(!needs_write(Some("x"), "x"), "same content");
    }

    #[test]
    fn refresh_writes_once_then_leaves_the_file_alone() {
        let dir = scratch("once");
        let exe = dir.join("visor");
        fs::write(&exe, "").unwrap();
        let file = dir.join("applications/visor.desktop");
        assert!(refresh(&exe, &file).unwrap(), "first run writes");
        assert_eq!(fs::read_to_string(&file).unwrap(), entry(&exe));
        assert!(!refresh(&exe, &file).unwrap(), "second run does not write");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_follows_a_moved_binary() {
        let dir = scratch("moved");
        let old = dir.join("visor-old");
        let new = dir.join("visor-new");
        fs::write(&old, "").unwrap();
        fs::write(&new, "").unwrap();
        let file = dir.join("visor.desktop");
        refresh(&old, &file).unwrap();
        assert!(refresh(&new, &file).unwrap());
        assert!(fs::read_to_string(&file).unwrap().contains("visor-new"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_skips_a_missing_binary() {
        let dir = scratch("missing");
        let file = dir.join("visor.desktop");
        assert!(!refresh(&dir.join("gone"), &file).unwrap());
        assert!(!file.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn icon_is_written_once() {
        let dir = scratch("icon");
        let file = dir.join("icons/hicolor/scalable/apps/visor.svg");
        assert!(write_if_changed(&file, ICON).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), ICON);
        assert!(!write_if_changed(&file, ICON).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
