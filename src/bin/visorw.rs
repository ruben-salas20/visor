//! `visorw.exe`, what Explorer and shortcuts open on Windows. A console program
//! started from Explorer gets a console window; this one is a GUI program, so it
//! gets none. It hands its arguments to `visor.exe`, in its own folder, without a
//! console either, and exits. Like `pythonw.exe` beside `python.exe`.
//!
//! Its whole contract is "same arguments, `visor.exe` in the same folder", so an
//! update that replaces only `visor.exe` keeps it working (src/update.rs). If
//! an update left only `visor.exe.old`, it puts that back first.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    // The same flag as platform::detach: with no console at all, Windows opens
    // none for visor.exe, nor a hidden one (conhost) that would cost memory.
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let Ok(exe) = std::env::current_exe() else {
        fail("No se pudo abrir visor: Windows no dice en qué carpeta está visorw.exe.");
    };
    let visor = exe.with_file_name("visor.exe");
    // An update that failed halfway may leave only visor.exe.old (update.rs).
    let old = exe.with_file_name("visor.exe.old");
    if !visor.exists() && old.exists() && std::fs::rename(&old, &visor).is_err() {
        let _ = std::fs::copy(&old, &visor);
    }
    let started = Command::new(&visor)
        .args(std::env::args_os().skip(1))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS)
        .spawn();
    if let Err(e) = started {
        let folder = visor.parent().unwrap_or(&visor).display();
        if e.kind() == std::io::ErrorKind::NotFound {
            fail(&format!("No se pudo abrir visor: falta visor.exe en la carpeta {folder}.\n\nVuelve a instalar visor."));
        }
        fail(&format!("No se pudo abrir visor.exe, en la carpeta {folder}.\n\nWindows dice: {e}"));
    }
}

/// A GUI program has no console to print to: a message box is the only way to tell the person.
#[cfg(windows)]
fn fail(text: &str) -> ! {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    const MB_ICONERROR: u32 = 0x10;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (text, caption) = (wide(text), wide("visor"));
    // SAFETY: both buffers end in NUL and live for the whole call.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR) };
    std::process::exit(1);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("visorw solo existe en Windows. Usa visor.");
    std::process::exit(1);
}
