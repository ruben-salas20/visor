use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FileId;

pub fn file_id(_path: &Path) -> Option<FileId> {
    todo!("Windows: file identity")
}

/// Windows runs any .exe; there is no permission to give.
pub fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn trash(_path: &Path) -> Result<(), String> {
    todo!("Windows: recycle bin")
}

pub fn detach(_command: &mut Command) -> &mut Command {
    todo!("Windows: DETACHED_PROCESS")
}

/// Windows has no exec: start the new process and exit, after giving up the pipe.
pub fn restart(_command: &mut Command) -> io::Error {
    todo!("Windows: restart")
}
