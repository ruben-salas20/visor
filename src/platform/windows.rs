use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    GetLongPathNameW, OPEN_EXISTING,
};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
    SHFileOperationW,
};

use crate::i18n::tr;

/// Windows accepts both kinds of slash, and a name may not contain either.
pub const SEPARATORS: &[char] = &['/', '\\'];

/// What tells one file from another, whatever path reaches it.
/// The volume serial and the file index together are unique while the file exists.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FileId(u32, u64);

/// The file `path` leads to, following links.
/// Backup semantics lets it open folders too; reading attributes needs no more rights.
pub fn file_id(path: &Path) -> Option<FileId> {
    let wide = wide(path.as_os_str());
    // SAFETY: `wide` is NUL-terminated and lives across the call; the handle is closed below.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: an all-zero struct is a valid out-parameter; the call fills it on success.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is open and `info` is a valid, writable struct.
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    // SAFETY: `handle` came from CreateFileW and is closed exactly once.
    unsafe { CloseHandle(handle) };
    (ok != 0).then(|| {
        FileId(
            info.dwVolumeSerialNumber,
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        )
    })
}

/// The real absolute path of `path`, with links followed. `canonicalize` returns the
/// `\\?\` form, which plain paths do not match, so that prefix is taken off.
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    let real = std::fs::canonicalize(path)?;
    Ok(real.to_str().and_then(unprefixed).map_or(real, PathBuf::from))
}

/// `C:\x` and `\\server\share\x` are absolute; `C:x` is not.
pub fn is_absolute_text(text: &str) -> bool {
    let b = text.as_bytes();
    text.starts_with(['/', '\\'])
        || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\'))
}

/// Loose page addresses use `/` only, so `C:\x\y` becomes `C:/x/y`.
pub fn loose_text(path: &str) -> String {
    unprefixed(path).unwrap_or_else(|| path.to_string()).replace('\\', "/")
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why Windows refuses `name` as one file name, or None. A name ending in a dot or a space
/// would be silently changed by Windows. `.` and `..` are steps, not names, so they pass.
pub fn name_problem(name: &str) -> Option<&'static str> {
    if name == "." || name == ".." {
        return None;
    }
    if name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c)) {
        return Some(tr(
            "El nombre tiene un carácter que Windows no admite: < > : \" / \\ | ? *",
            "The name has a character Windows does not allow: < > : \" / \\ | ? *",
        ));
    }
    if name.ends_with(['.', ' ']) {
        return Some(tr(
            "El nombre no puede acabar en punto ni en espacio.",
            "The name cannot end with a dot or a space.",
        ));
    }
    let stem = name.split('.').next().unwrap_or(name);
    if RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
        return Some(tr(
            "Windows reserva ese nombre para un dispositivo (CON, NUL, COM1…).",
            "Windows reserves that name for a device (CON, NUL, COM1…).",
        ));
    }
    None
}

/// The name of the item `path` leads to, as stored on disk. A short name such as `VISOR~1`
/// gives the long one, so a short name cannot hide a folder whose real name starts with a dot.
pub fn real_name(path: &Path) -> Option<OsString> {
    let wide = wide(path.as_os_str());
    let mut buf = vec![0u16; 32_768];
    // SAFETY: `wide` is NUL-terminated and `buf` is writable for the length passed.
    let n = unsafe { GetLongPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    PathBuf::from(OsString::from_wide(&buf[..n]))
        .file_name()
        .map(OsStr::to_os_string)
}

/// The folder where the app keeps its state. Windows has no state folder, so the local data one is used.
pub fn state_dir() -> Option<PathBuf> {
    dirs::data_local_dir()
}

/// Whether Windows marks `entry` hidden or system, as Explorer hides it. Reading
/// the attributes costs nothing: the folder listing already brought them.
pub fn hidden_entry(entry: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;
    entry.metadata().is_ok_and(|m| m.file_attributes() & HIDDEN_OR_SYSTEM != 0)
}

/// The socket path when `VISOR_SOCKET` is not set. The folder may not exist yet.
pub fn default_socket() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("visor")
        .join("visor.sock")
}

/// Fills `bytes` from the system's secure random source.
pub fn random(bytes: &mut [u8]) -> io::Result<()> {
    getrandom::fill(bytes).map_err(io::Error::other)
}

/// Windows runs any .exe; there is no permission to give.
pub fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Moves `path` to the Recycle Bin, where it can be restored.
/// The shell API wants a list of paths ending in two NUL characters.
pub fn trash(path: &Path) -> Result<(), String> {
    let mut from = wide(path.as_os_str());
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    // SAFETY: `op` and the buffer it points to are valid for the whole call.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted != 0 {
        return Err(format!("código de Windows {code:#x}"));
    }
    Ok(())
}

/// Detached from this console and from its process group, so closing the terminal keeps the child alive.
pub fn detach(command: &mut Command) -> &mut Command {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
}

/// Windows has no exec: start the new process and exit. Returns only if the start fails.
/// `VISOR_RESTART` tells the new process to wait for this one to free the pipe.
pub fn restart(command: &mut Command) -> io::Error {
    match detach(command.env("VISOR_RESTART", "1")).spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => e,
    }
}

/// `\\?\C:\x` is `C:\x`, and `\\?\UNC\server\share` is `\\server\share`. None for other text.
fn unprefixed(text: &str) -> Option<String> {
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return Some(format!(r"\\{unc}"));
    }
    let rest = text.strip_prefix(r"\\?\")?;
    rest.as_bytes()
        .get(1)
        .is_some_and(|b| *b == b':')
        .then(|| rest.to_string())
}

/// The text as UTF-16 with a final NUL, the form the Windows calls take.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}
