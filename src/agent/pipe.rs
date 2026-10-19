//! The agent channel on Windows: a named pipe that only the current user can
//! open. The first instance claims the name; every accepted client gets a new
//! instance, as a socket gets a new connection.

use crate::i18n::tr;
use crate::trf;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
    SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

/// Size of the pipe's buffers in each direction. A request or reply is one short line.
const BUFFER: u32 = 64 * 1024;
/// How long to wait for a busy instance to free up, and how many times to try.
const BUSY_WAIT: Duration = Duration::from_millis(500);
const BUSY_TRIES: u32 = 4;
/// How long a restarted visor waits for the one it replaces to exit and free the name.
const RESTART_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum ClaimError {
    /// Another instance owns the pipe.
    Running,
    Failed(String),
}

/// Nothing to remove: Windows frees the name when the last handle to the pipe
/// closes, which happens when the process ends or the `Listener` is dropped.
pub struct Claim;

impl Claim {
    pub fn release(self) {}
}

/// Takes the pipe name derived from `path`. Only the first instance can
/// claim it, so two instances never run at once.
pub fn claim(path: &Path) -> Result<(Listener, Claim), ClaimError> {
    let name = pipe_name(path);
    let security = Security::for_current_user().map_err(|e| failed(path, e))?;
    let deadline = std::time::Instant::now() + RESTART_WAIT;
    let restarting = std::env::var_os("VISOR_RESTART").is_some();
    let waiting = loop {
        match create_instance(&name, &security, true) {
            Ok(handle) => break handle,
            // Windows answers ERROR_ACCESS_DENIED when this first-instance flag meets an existing pipe.
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
                if !restarting || std::time::Instant::now() >= deadline {
                    // The name is taken: by our own visor, or by another user posing as it.
                    return match Stream::connect(path) {
                        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Err(ClaimError::Failed(e.to_string())),
                        _ => Err(ClaimError::Running),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(failed(path, e)),
        }
    };
    Ok((Listener { name, security, waiting: Mutex::new(waiting) }, Claim))
}

fn failed(path: &Path, e: io::Error) -> ClaimError {
    ClaimError::Failed(trf!("No se pudo crear {}: {e}", "Could not create {}: {e}", path.display()))
}

pub struct Listener {
    name: Vec<u16>,
    security: Security,
    /// The instance that waits for the next client. There is always one.
    waiting: Mutex<Handle>,
}

impl Listener {
    fn accept(&self) -> io::Result<Stream> {
        // Create the next instance before serving this one, so no client finds the name without a waiting instance.
        let next = create_instance(&self.name, &self.security, false)?;
        let current = {
            let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::replace(&mut *waiting, next)
        };
        // SAFETY: `current` is an instance of this pipe that nobody has connected yet.
        let connected = unsafe { ConnectNamedPipe(current.0, null_mut()) };
        if connected == 0 && io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
            return Err(io::Error::last_os_error());
        }
        Ok(Stream { handle: current })
    }

    pub fn incoming(&self) -> Incoming<'_> {
        Incoming { listener: self }
    }
}

pub struct Incoming<'a> {
    listener: &'a Listener,
}

impl Iterator for Incoming<'_> {
    type Item = io::Result<Stream>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.listener.accept())
    }
}

pub struct Stream {
    handle: Handle,
}

impl Stream {
    /// Connects to the app listening at `path`. Fails at once when nobody listens.
    pub fn connect(path: &Path) -> io::Result<Stream> {
        let name = pipe_name(path);
        for _ in 0..BUSY_TRIES {
            // SAFETY: `name` is a NUL-terminated wide string that lives for the call.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    null(),
                    OPEN_EXISTING,
                    // The app may read who we are, but may not act as us.
                    SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                let stream = Stream { handle: Handle(handle) };
                // The name can be guessed, so another user could have created it first.
                if !served_by_us(&stream) {
                    return Err(foreign());
                }
                return Ok(stream);
            }
            let e = io::Error::last_os_error();
            // A pipe whose DACL leaves us out was made by someone else.
            if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                return Err(foreign());
            }
            if e.raw_os_error() != Some(ERROR_PIPE_BUSY as i32) {
                return Err(e);
            }
            // SAFETY: as above; the call only waits.
            unsafe { WaitNamedPipeW(name.as_ptr(), BUSY_WAIT.as_millis() as u32) };
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, tr("La app está ocupada y no responde.", "The app is busy and does not respond.")))
    }
}

impl Read for &Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read = 0u32;
        let len = buf.len().min(u32::MAX as usize) as u32;
        // SAFETY: `buf` is valid for `len` bytes; the handle is open.
        let ok = unsafe { ReadFile(self.handle.0, buf.as_mut_ptr().cast(), len, &mut read, null_mut()) };
        if ok == 0 {
            let e = io::Error::last_os_error();
            // The client closed its end: the same end of stream a socket gives.
            if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                return Ok(0);
            }
            return Err(e);
        }
        Ok(read as usize)
    }
}

impl Write for &Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0u32;
        let len = buf.len().min(u32::MAX as usize) as u32;
        // SAFETY: `buf` is valid for `len` bytes; the handle is open.
        let ok = unsafe { WriteFile(self.handle.0, buf.as_ptr().cast(), len, &mut written, null_mut()) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(written as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        // Nothing to do: the pipe passes the bytes on as they are written.
        Ok(())
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self).flush()
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}

/// An open handle, closed on drop.
struct Handle(HANDLE);

// SAFETY: the pipe calls used here accept any thread, and each handle has one owner.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle is owned here and closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// The security descriptor that lets only the current user open the pipe.
struct Security(PSECURITY_DESCRIPTOR);

// SAFETY: the descriptor is built once and only read afterwards.
unsafe impl Send for Security {}

impl Security {
    fn for_current_user() -> io::Result<Security> {
        // "D:P" is a DACL with no inheritance; "GA" grants all access, to this user's SID only.
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", current_user_sid()?).encode_utf16().chain(Some(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: the output pointer is valid; the descriptor is freed in `Drop`.
        let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, null_mut()) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Security(descriptor))
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW, freed once.
        unsafe { LocalFree(self.0 as HLOCAL) };
    }
}

/// The SID of the user who runs this process, as text (`S-1-5-21-…`).
fn current_user_sid() -> io::Result<String> {
    // SAFETY: the pseudo handle of this process needs no closing.
    process_sid(unsafe { GetCurrentProcess() })
}

/// True if the process at the other end of `stream` runs as this user.
fn served_by_us(stream: &Stream) -> bool {
    let mut pid = 0u32;
    // SAFETY: the handle is an open client end of a pipe; the output pointer is valid.
    if unsafe { GetNamedPipeServerProcessId(stream.handle.0, &mut pid) } == 0 {
        return false;
    }
    // SAFETY: plain call; a null result means we may not look at that process.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Handle(process);
    matches!((process_sid(process.0), current_user_sid()), (Ok(theirs), Ok(ours)) if theirs == ours)
}

fn foreign() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        tr(
            "El canal de visor lo ha creado otro usuario de este equipo. No se le envía nada.",
            "Another user of this computer created visor's channel. Nothing is sent to it.",
        ),
    )
}

/// The SID of the user `process` runs as, as text.
fn process_sid(process: HANDLE) -> io::Result<String> {
    let mut token: HANDLE = null_mut();
    // SAFETY: the output pointer is valid.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let mut len = 0u32;
    // SAFETY: a null buffer only asks for the size; the call fails with that size set.
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut len) };
    // Word-sized elements, so the buffer is aligned for TOKEN_USER.
    let mut buf = vec![0usize; (len as usize).div_ceil(size_of::<usize>())];
    // SAFETY: the buffer has `len` bytes and holds a TOKEN_USER once the call succeeds.
    if unsafe { GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the buffer now holds a TOKEN_USER, and the SID points into the same buffer.
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text: *mut u16 = null_mut();
    // SAFETY: the output pointer is valid; the text is freed below.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut end = 0;
    // SAFETY: Windows returns a NUL-terminated string.
    unsafe {
        while *text.add(end) != 0 {
            end += 1;
        }
    }
    // SAFETY: `text` is a SID string from ConvertSidToStringSidW, freed once.
    let sid = unsafe {
        let units = std::slice::from_raw_parts(text, end);
        let sid = String::from_utf16_lossy(units);
        LocalFree(text.cast());
        sid
    };
    Ok(sid)
}

/// Creates one instance of the pipe. `first` makes Windows refuse when the pipe already exists.
fn create_instance(name: &[u16], security: &Security, first: bool) -> io::Result<Handle> {
    let mut mode = PIPE_ACCESS_DUPLEX;
    if first {
        mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    // SAFETY: `name` is NUL-terminated and `attributes` lives for the call.
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER,
            BUFFER,
            0,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(handle))
}

/// `\\.\pipe\visor-<hash>`, derived from the socket path, so `VISOR_SOCKET` gives a test instance its own pipe.
fn pipe_name(path: &Path) -> Vec<u16> {
    // FNV-1a over the path's UTF-16 units: fixed for every build, unlike std's hashers.
    let hash = path
        .as_os_str()
        .encode_wide()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, unit| (h ^ u64::from(unit)).wrapping_mul(0x0100_0000_01b3));
    format!(r"\\.\pipe\visor-{hash:016x}").encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use std::thread;
    use std::time::Instant;

    /// A path that does not need to exist: only its name matters.
    fn test_path(name: &str) -> PathBuf {
        PathBuf::from(format!(r"C:\visor-test\{name}-{}.sock", std::process::id()))
    }

    #[test]
    fn different_paths_get_different_pipe_names() {
        let a = pipe_name(&test_path("a"));
        let b = pipe_name(&test_path("b"));
        assert_ne!(a, b);
        let text = String::from_utf16_lossy(&a);
        assert!(text.starts_with(r"\\.\pipe\visor-"), "{text}");
    }

    #[test]
    fn only_one_instance_gets_the_channel() {
        let path = test_path("one");
        let (listener, claim_a) = claim(&path).unwrap();
        assert!(matches!(claim(&path), Err(ClaimError::Running)));
        claim_a.release();
        drop(listener);
        assert!(claim(&path).is_ok(), "free again once the first instance is gone");
    }

    #[test]
    fn a_line_goes_out_and_the_reply_comes_back() {
        let path = test_path("line");
        let (listener, _claim) = claim(&path).unwrap();
        let server = thread::spawn(move || {
            let stream = listener.incoming().next().unwrap().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            writeln!(&stream, "respuesta a {}", line.trim()).unwrap();
        });
        let mut client = Stream::connect(&path).unwrap();
        writeln!(client, "hola").unwrap();
        let mut reply = String::new();
        BufReader::new(client).read_line(&mut reply).unwrap();
        assert_eq!(reply, "respuesta a hola\n");
        server.join().unwrap();
    }

    #[test]
    fn two_clients_at_once_are_both_served() {
        let path = test_path("two");
        let (listener, _claim) = claim(&path).unwrap();
        let server = thread::spawn(move || {
            // As in server.rs: one thread per accepted client.
            let mut workers = Vec::new();
            for stream in listener.incoming().take(2) {
                let stream = stream.unwrap();
                workers.push(thread::spawn(move || {
                    let mut line = String::new();
                    BufReader::new(&stream).read_line(&mut line).unwrap();
                    writeln!(&stream, "{}", line.trim().to_uppercase()).unwrap();
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        // The first client connects and waits; the second must still be served.
        let mut first = Stream::connect(&path).unwrap();
        let mut second = Stream::connect(&path).unwrap();
        writeln!(second, "b").unwrap();
        let mut reply = String::new();
        BufReader::new(&mut second).read_line(&mut reply).unwrap();
        assert_eq!(reply, "B\n");
        writeln!(first, "a").unwrap();
        let mut reply = String::new();
        BufReader::new(&mut first).read_line(&mut reply).unwrap();
        assert_eq!(reply, "A\n");
        server.join().unwrap();
    }

    #[test]
    fn our_own_server_passes_the_owner_check() {
        let path = test_path("owner");
        let (listener, _claim) = claim(&path).unwrap();
        let server = thread::spawn(move || listener.incoming().next().unwrap().unwrap());
        let stream = Stream::connect(&path).expect("our own visor is accepted");
        assert!(served_by_us(&stream));
        drop(server.join().unwrap());
    }

    #[test]
    fn connect_fails_fast_without_a_server() {
        let path = test_path("nobody");
        let start = Instant::now();
        assert!(Stream::connect(&path).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
