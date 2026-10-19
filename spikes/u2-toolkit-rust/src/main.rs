// Experimento U2: ¿cabe un webview de wry dentro de una ventana de eframe en Wayland?
//
// Modos (variable U2_MODO):
//   child -> build_as_child con el manejador de ventana de eframe (intento a).
//   gtk   -> gtk::init + build_gtk sobre un gtk::Fixed en una ventana GTK propia (intento b).
// Cada modo se ejecuta en un proceso aparte para que sus errores no se mezclen.

use eframe::egui;
use gtk::prelude::*;
use wry::{WebViewBuilder, WebViewBuilderExtUnix};

const PAGINA: &str = "<html><body style='background:#284;color:#fff;font-family:sans-serif'>\
<h1>Webview de wry</h1><p>Si lees esto, la página carga.</p></body></html>";

#[derive(Default)]
struct Estado {
    modo: String,
    resultado: String,
    webview: Option<wry::WebView>,
    gtk_fixed: Option<gtk::Fixed>,
    intentado: bool,
}

struct App {
    e: Estado,
}

impl eframe::App for App {
    fn update(&mut self, ui_ctx: &egui::Context, frame: &mut eframe::Frame) {
        if !self.e.intentado {
            self.e.intentado = true;
            match self.e.modo.as_str() {
                "child" => {
                    let r = WebViewBuilder::new()
                        .with_html(PAGINA)
                        .build_as_child(&*frame);
                    self.e.resultado = match r {
                        Ok(wv) => {
                            self.e.webview = Some(wv);
                            "OK".to_string()
                        }
                        Err(e) => format!("Err: {e:?} / {e}"),
                    };
                }
                "gtk" => {
                    let fixed = self.e.gtk_fixed.take().expect("gtk::Fixed");
                    let r = WebViewBuilder::new().with_html(PAGINA).build_gtk(&fixed);
                    self.e.resultado = match r {
                        Ok(wv) => {
                            self.e.webview = Some(wv);
                            "OK (ventana GTK separada)".to_string()
                        }
                        Err(e) => format!("Err: {e:?} / {e}"),
                    };
                }
                _ => {}
            }
            let tipo = match raw_window_handle::HasWindowHandle::window_handle(&*frame) {
                Ok(h) => match h.as_raw() {
                    raw_window_handle::RawWindowHandle::Wayland(_) => "Wayland".to_string(),
                    raw_window_handle::RawWindowHandle::Xlib(_) => "Xlib".to_string(),
                    other => format!("{other:?}"),
                },
                Err(e) => format!("sin handle: {e}"),
            };
            eprintln!("U2 modo={} handle_eframe={} resultado={}", self.e.modo, tipo, self.e.resultado);
        }
        if self.e.modo == "gtk" {
            // Bombea el bucle de GTK: eframe no lo ejecuta por nosotros.
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            ui_ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        egui::SidePanel::left("panel").show(ui_ctx, |ui| {
            ui.heading("Panel egui");
            ui.label(format!("modo: {}", self.e.modo));
            ui.label(format!("resultado: {}", self.e.resultado));
        });
        egui::CentralPanel::default().show(ui_ctx, |ui| {
            ui.label("Zona central (aquí iría el webview en el intento a).");
        });
    }
}

fn main() -> eframe::Result {
    let modo = std::env::var("U2_MODO").unwrap_or_else(|_| "child".into());
    let mut estado = Estado { modo: modo.clone(), ..Default::default() };
    if modo == "gtk" {
        gtk::init().expect("gtk::init");
        let ventana = gtk::Window::new(gtk::WindowType::Toplevel);
        ventana.set_title("U2 GTK separada");
        ventana.set_default_size(400, 300);
        let fixed = gtk::Fixed::new();
        fixed.set_size_request(400, 300);
        ventana.add(&fixed);
        ventana.show_all();
        estado.gtk_fixed = Some(fixed);
    }
    let opciones = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("U2 eframe").with_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "u2-eframe",
        opciones,
        Box::new(move |_cc| Ok(Box::new(App { e: estado }))),
    )
}
