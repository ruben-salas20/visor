//! Experimento W3: ¿se puede saber en WebView2 qué URL está comprometida
//! uniendo NavigationStarting y ContentLoading por NavigationId?
//!
//! Uso (en Windows): `w3-committed.exe [directorio_de_salida]`. Escribe
//! `w3-resultados.json` y `w3-resultados.md` y sale con 1 si algún caso falla.

#![cfg_attr(not(windows), allow(dead_code))]

mod cases;
mod server;
#[cfg(windows)]
mod runner;

use std::path::PathBuf;

fn main() {
    #[cfg(windows)]
    {
        let dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        std::process::exit(runner::run(dir));
    }
    #[cfg(not(windows))]
    {
        let _ = PathBuf::new();
        eprintln!("El experimento W3 necesita WebView2: ejecútalo en Windows.");
        std::process::exit(2);
    }
}
