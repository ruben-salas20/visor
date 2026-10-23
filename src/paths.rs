use std::path::PathBuf;

/// `VISOR_SOCKET` runs a separate instance, for tests next to the one in use.
pub fn socket() -> PathBuf {
    if let Some(path) = std::env::var_os("VISOR_SOCKET") {
        return PathBuf::from(path);
    }
    crate::platform::default_socket()
}

pub fn config() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("visor/config.json")
}

/// Empty file whose time is the last check for a new version.
pub fn update_check() -> PathBuf {
    crate::platform::state_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("visor/actualizacion")
}

/// The desktop entry that offers visor for `.md` and `.html` files.
pub fn desktop_entry() -> Option<PathBuf> {
    Some(crate::platform::desktop_data_dir()?.join("applications/visor.desktop"))
}

/// The icon that the desktop entry names.
pub fn desktop_icon() -> Option<PathBuf> {
    Some(crate::platform::desktop_data_dir()?.join("icons/hicolor/scalable/apps/visor.svg"))
}

pub fn events() -> PathBuf {
    crate::platform::state_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("visor/events.jsonl")
}
