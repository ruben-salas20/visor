//! Prototipo (issue 35): menús del marco dibujados encima de una página viva.
//!
//! Vía 1: el webview del marco va encima de las páginas, con fondo transparente
//! en el hueco de cada página. La entrada del marco se limita a «ventana menos
//! huecos más menús abiertos»: forma de entrada de GDK en Linux, región de
//! ventana (`SetWindowRgn`) en Windows.
//!
//! `--prueba <salida.json>` ejecuta la secuencia de `prueba.rs` y termina.

mod prueba;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win;

use std::borrow::Cow;
use wry::http::Response;

pub const MARCO: &str = include_str!("../web/marco.html");
pub const PAGINA: &str = include_str!("../web/pagina.html");

pub fn html(texto: &'static str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Cow::Borrowed(texto.as_bytes()))
        .unwrap()
}

/// `[x, y, w, h]` from the frame's JSON.
pub fn caja(v: &serde_json::Value) -> Option<[i32; 4]> {
    let a = v.as_array()?;
    let n = |i: usize| a.get(i).and_then(|x| x.as_f64()).map(|x| x.round() as i32);
    Some([n(0)?, n(1)?, n(2)?, n(3)?])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let salida = args.iter().position(|a| a == "--prueba").and_then(|i| args.get(i + 1)).cloned();
    #[cfg(target_os = "linux")]
    linux::main(salida);
    #[cfg(windows)]
    win::main(salida);
}
