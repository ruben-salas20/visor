//! Whether a package installed this copy of visor (.deb or Flatpak). The package
//! then updates it and brings its own desktop entry, so visor does neither.

use std::path::Path;

/// `/usr/local` is where people put programs by hand, not a package.
pub fn managed(exe: &Path) -> bool {
    cfg!(target_os = "linux") && ((exe.starts_with("/usr") && !exe.starts_with("/usr/local")) || exe.starts_with("/app"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_paths_are_managed_and_hand_installed_ones_are_not() {
        assert!(managed(Path::new("/usr/bin/visor")));
        assert!(managed(Path::new("/app/bin/visor")));
        assert!(!managed(Path::new("/usr/local/bin/visor")));
        assert!(!managed(Path::new("/home/ana/.local/bin/visor")));
        assert!(!managed(Path::new("/application/visor")));
    }
}
