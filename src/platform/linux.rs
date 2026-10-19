use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

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
