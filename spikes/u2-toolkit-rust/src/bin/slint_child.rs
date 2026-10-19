// Experimento U2, intento (a) con slint: build_as_child con el manejador de la ventana winit de slint.
// Se usa la macro slint! para no necesitar build.rs ni archivos .slint aparte.

use slint::winit_030::WinitWindowAccessor;
use wry::WebViewBuilder;

slint::slint! {
    export component Ventana inherits Window {
        title: "U2 slint";
        width: 900px;
        height: 600px;
        HorizontalLayout {
            Rectangle { width: 200px; background: #223; Text { text: "Panel slint"; color: white; } }
            Rectangle { background: #111; }
        }
    }
}

fn raw_handle_kind(w: &slint::winit_030::winit::window::Window) -> &'static str {
    use raw_window_handle::HasWindowHandle;
    match w.window_handle().map(|h| h.as_raw()) {
        Ok(raw_window_handle::RawWindowHandle::Wayland(_)) => "Wayland",
        Ok(raw_window_handle::RawWindowHandle::Xlib(_)) => "Xlib",
        _ => "otro",
    }
}

const PAGINA: &str = "<html><body style='background:#284;color:#fff'><h1>Webview de wry</h1></body></html>";

fn main() -> Result<(), slint::PlatformError> {
    let ventana = Ventana::new()?;
    ventana.show()?;
    let ventana_debil = ventana.as_weak();
    // Esperamos a que winit cree la superficie antes de pedir el manejador.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), move || {
        let Some(v) = ventana_debil.upgrade() else { return };
        // Manejador winit (Wayland en esta sesión) de la ventana de slint.
        let r = v.window().with_winit_window(|w| {
            let tipo = format!("{:?}", raw_handle_kind(w));
            let r = WebViewBuilder::new().with_html(PAGINA).build_as_child(w);
            (tipo, r.map(|wv| std::mem::forget(wv)))
        });
        match r {
            Some((tipo, Ok(()))) => eprintln!("U2 slint handle={tipo} resultado=OK"),
            Some((tipo, Err(e))) => eprintln!("U2 slint handle={tipo} resultado=Err: {e:?} / {e}"),
            None => eprintln!("U2 slint: sin ventana winit"),
        }
    });
    ventana.run()
}
