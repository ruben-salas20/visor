use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The characters that separate the parts of a path.
pub const SEPARATORS: &[char] = &['/'];

/// The real absolute path of `path`, with links followed.
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

/// True if `text` is an absolute path, as pages and the address bar write it.
pub fn is_absolute_text(text: &str) -> bool {
    text.starts_with('/')
}

/// The text a loose page's address carries for the absolute path `path`.
pub fn loose_text(path: &str) -> String {
    path.to_string()
}

/// Why `name` cannot be stored on this system, in the user's language; `None` if it can.
pub fn name_problem(_name: &str) -> Option<&'static str> {
    None
}

/// The name of the item `path` leads to, as it is stored on disk.
pub fn real_name(path: &Path) -> Option<OsString> {
    path.file_name().map(OsString::from)
}

/// The folder where the app keeps its state (logs, last update check).
pub fn state_dir() -> Option<PathBuf> {
    dirs::state_dir()
}

/// Where the agent socket lives when `VISOR_SOCKET` is not set.
pub fn default_socket() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        // Each `flatpak run` gets its own runtime folder; only app/<id> is shared
        // between them, and with the host.
        Some(dir) => match std::env::var_os("FLATPAK_ID") {
            Some(id) => PathBuf::from(dir).join("app").join(id).join("visor.sock"),
            None => PathBuf::from(dir).join("visor.sock"),
        },
        None => std::env::temp_dir().join(format!("visor-{}.sock", std::env::var("USER").unwrap_or_default())),
    }
}

/// What tells one file from another, whatever path reaches it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId(u64, u64);

/// The file `path` leads to, following links.
pub fn file_id(path: &Path) -> Option<FileId> {
    fs::metadata(path).ok().map(|m| FileId(m.dev(), m.ino()))
}

/// Fills `bytes` from the system's secure random source.
pub fn random(bytes: &mut [u8]) -> io::Result<()> {
    fs::File::open("/dev/urandom")?.read_exact(bytes)
}

/// Lets `path` run as a program.
pub fn make_executable(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

/// Moves `path` to the desktop trash, where it can be restored.
pub fn trash(path: &Path) -> Result<(), String> {
    use gtk::gio::{Cancellable, File, prelude::FileExt};
    File::for_path(path).trash(Cancellable::NONE).map_err(|e| e.to_string())
}

/// Detaches `command` from this terminal, so closing it does not close the child.
pub fn detach(command: &mut Command) -> &mut Command {
    command.process_group(0)
}

/// Replaces this process with `command`. Returns only if that fails.
pub fn restart(command: &mut Command) -> io::Error {
    command.exec()
}
