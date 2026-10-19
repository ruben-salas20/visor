//! Prototipo W4 en Windows (issue 35): siete funciones que visor usa en Linux y que wry 0.57
//! no da en Windows, probadas con la API COM de WebView2.
//!
//! `--auto <salida.json> --prueba <nombre>` ejecuta una prueba, escribe el JSON y termina solo.
//! Pruebas: captura, pantalla, fallos, estilo, zoom, foco, lateral.
//! Códigos de salida: 0 criterio cumplido, 1 no cumplido o error, 2 uso, 3 tiempo agotado.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod server;

#[cfg(windows)]
fn main() {
    app::main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("w4-funciones solo funciona en Windows");
    std::process::exit(2);
}
