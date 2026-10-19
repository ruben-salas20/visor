//! Prototipo U1 en Windows (issue 35): una ventana winit con tres webviews hijos de WebView2.
//! El marco (React, de spikes/u1-marco-html/web/react) ocupa toda la ventana y dos páginas
//! se colocan encima del hueco que el marco anuncia con `recolocar`.
//!
//! `--auto <salida.json> --prueba a|b|c|d` ejecuta una prueba, escribe el JSON y termina solo.
//! Códigos de salida: 0 ok, 1 prueba fallida, 3 tiempo agotado, 4 hilo principal bloqueado.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod measure;
#[cfg(windows)]
mod nav;
#[cfg(windows)]
mod server;

#[cfg(windows)]
fn main() {
    app::main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("u1-windows solo funciona en Windows");
    std::process::exit(2);
}
