//! Linux: a `gtk::Overlay` with the page below (in a `gtk::Fixed`) and the
//! frame on top, transparent, with an input shape.

use crate::prueba::{self, Entrada, Registro};
use crate::{MARCO, PAGINA, caja, html};
use gtk::cairo;
use gtk::prelude::*;
use serde_json::Value;
use std::io::Write;
use std::process::{ChildStdin, Command, Stdio};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use wry::http::Request;
use wry::{WebContext, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

enum Ev {
    Estado(Value),
    Fin(Value),
}

pub fn main(salida: Option<String>) {
    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("menus-encima")
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(1000.0, 700.0))
        .build(&event_loop)
        .unwrap();
    let vbox = window.default_vbox().unwrap();
    let overlay = gtk::Overlay::new();
    let abajo = gtk::Fixed::new();
    let arriba = gtk::Box::new(gtk::Orientation::Vertical, 0);
    overlay.add(&abajo);
    overlay.add_overlay(&arriba);
    vbox.pack_start(&overlay, true, true, 0);
    let caja_pagina = gtk::Box::new(gtk::Orientation::Vertical, 0);
    abajo.put(&caja_pagina, 0, 0);
    {
        let cp = caja_pagina.clone();
        abajo.connect_size_allocate(move |_, a| {
            cp.size_allocate(&gtk::Allocation::new(a.x() + 200, a.y() + 40, (a.width() - 200).max(1), (a.height() - 40).max(1)));
        });
    }
    overlay.show_all();

    let reg: Registro = Default::default();
    let mut ctx = WebContext::new(None);
    let r = reg.clone();
    let pagina = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_custom_protocol("pagina".into(), |_, _| html(PAGINA))
        .with_url("pagina://localhost/")
        .with_ipc_handler(move |req: Request<String>| {
            if let Ok(v) = serde_json::from_str(req.body()) {
                r.lock().unwrap().push(("pagina", v));
            }
        })
        .build_gtk(&caja_pagina)
        .unwrap();
    let (r, p) = (reg.clone(), proxy.clone());
    let marco = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_transparent(true)
        .with_custom_protocol("marco".into(), |_, _| html(MARCO))
        .with_url("marco://localhost/")
        .with_ipc_handler(move |req: Request<String>| {
            if let Ok(v) = serde_json::from_str::<Value>(req.body()) {
                if v["tipo"] == "estado" {
                    let _ = p.send_event(Ev::Estado(v.clone()));
                }
                r.lock().unwrap().push(("marco", v));
            }
        })
        .build_gtk(&arriba)
        .unwrap();

    {
        let r = reg.clone();
        pagina.webview().connect_focus_in_event(move |_, _| {
            r.lock().unwrap().push(("gtk", serde_json::json!({ "tipo": "foco-entra-pagina" })));
            gtk::glib::Propagation::Proceed
        });
        let r = reg.clone();
        marco.webview().connect_focus_out_event(move |_, _| {
            r.lock().unwrap().push(("gtk", serde_json::json!({ "tipo": "foco-sale-marco" })));
            gtk::glib::Propagation::Proceed
        });
        let r = reg.clone();
        marco.webview().connect_focus_in_event(move |_, _| {
            r.lock().unwrap().push(("gtk", serde_json::json!({ "tipo": "foco-entra-marco" })));
            gtk::glib::Propagation::Proceed
        });
    }

    if let Some(_) = &salida {
        let (reg, p) = (reg.clone(), proxy.clone());
        std::thread::spawn(move || {
            let mut e = Sway::new();
            let informe = prueba::ejecutar(reg, &mut e);
            let _ = p.send_event(Ev::Fin(informe));
        });
        let p = proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(150));
            let _ = p.send_event(Ev::Fin(serde_json::json!({ "ok": false, "fallos": ["tiempo agotado"] })));
        });
    }

    let mut ventanas_dichas = false;
    event_loop.run(move |event, _, cf| {
        *cf = ControlFlow::Wait;
        let _ = (&window, &pagina, &ctx);
        match event {
            Event::UserEvent(Ev::Estado(v)) => {
                let vista = marco.webview();
                if !ventanas_dichas {
                    ventanas_dichas = true;
                    eprintln!("ventanas GDK del marco: {}", cadena(&vista));
                }
                if std::env::var_os("MENUS_SIN_FORMA").is_none() {
                    aplicar(&vista, &v);
                }
            }
            Event::UserEvent(Ev::Fin(v)) => {
                let texto = serde_json::to_string_pretty(&v).unwrap();
                println!("{texto}");
                if let Some(f) = &salida {
                    std::fs::write(f, texto).unwrap();
                }
                let ok = v["ok"] == true;
                std::process::exit(if ok { 0 } else { 1 });
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *cf = ControlFlow::Exit,
            _ => {}
        }
    });
}

fn rect(c: [i32; 4]) -> cairo::RectangleInt {
    cairo::RectangleInt::new(c[0], c[1], c[2], c[3])
}

/// Input of the frame: the window, minus the holes, plus the open menus.
fn aplicar(vista: &webkit2gtk::WebView, v: &Value) {
    let Some([_, _, w, h]) = caja(&serde_json::json!([0, 0, v["ventana"][0], v["ventana"][1]])) else { return };
    let region = cairo::Region::create_rectangle(&rect([0, 0, w, h]));
    let huecos = if v["capturar"] == true { None } else { v["huecos"].as_array() };
    for c in huecos.into_iter().flatten().filter_map(caja) {
        let _ = region.subtract_rectangle(&rect(c));
    }
    for c in v["solidos"].as_array().into_iter().flatten().filter_map(caja) {
        let _ = region.union_rectangle(&rect(c));
    }
    // GtkOverlay gives each overlay child its own GdkWindow, and GDK picks the
    // deepest window whose input shape holds the point: shape every window
    // from the view up to the toplevel, or the overlay's would take the click.
    let top = vista.toplevel().and_then(|t| t.window());
    let (mut dx, mut dy) = (0, 0);
    let mut actual = vista.window();
    while let Some(win) = actual {
        if Some(&win) == top.as_ref() {
            break;
        }
        win.input_shape_combine_region(&region, dx, dy);
        let (x, y) = win.position();
        dx += x;
        dy += y;
        actual = win.parent();
    }
}

fn cadena(vista: &webkit2gtk::WebView) -> String {
    let top = vista.toplevel().and_then(|t| t.window());
    let mut partes = Vec::new();
    let mut actual = vista.window();
    while let Some(win) = actual {
        let es_top = Some(&win) == top.as_ref();
        partes.push(format!("{:?}{}", win.position(), if es_top { " (toplevel)" } else { "" }));
        actual = win.parent();
    }
    partes.join(" -> ")
}

/// Real input in the headless sway: a wlroots virtual pointer (`puntero`),
/// `wtype` for keys, `grim` for pixels. The window sits at 0,0 at scale 1.
struct Sway {
    entrada: ChildStdin,
}

impl Sway {
    fn new() -> Sway {
        let exe = std::env::current_exe().unwrap().with_file_name("puntero");
        let mut hijo = Command::new(exe).stdin(Stdio::piped()).spawn().expect("puntero");
        let entrada = hijo.stdin.take().unwrap();
        // Without a keyboard on the seat no window is active and GTK sends no
        // focus events: keep a virtual keyboard alive for the whole test.
        let _ = Command::new("wtype").args(["-s", "170000", "-k", "Shift_L"]).spawn();
        std::thread::sleep(std::time::Duration::from_millis(300));
        Sway { entrada }
    }

    fn orden(&mut self, s: &str) {
        let _ = writeln!(self.entrada, "{s}");
        let _ = self.entrada.flush();
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

impl Entrada for Sway {
    fn mover(&mut self, x: i32, y: i32) {
        self.orden(&format!("mover {x} {y}"));
    }
    fn pulsar(&mut self) {
        self.orden("pulsar");
    }
    fn soltar(&mut self) {
        self.orden("soltar");
    }
    fn rueda(&mut self, pasos: i32) {
        for _ in 0..pasos {
            self.orden("rueda 1");
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }
    fn teclear(&mut self, texto: &str) {
        let _ = Command::new("wtype").args(["-s", "300", texto]).status();
    }
    fn pixel(&mut self, x: i32, y: i32) -> Option<[u8; 3]> {
        let out = Command::new("grim").args(["-g", &format!("{x},{y} 1x1"), "-t", "ppm", "-"]).output().ok()?;
        let b = out.stdout;
        (b.len() >= 3).then(|| [b[b.len() - 3], b[b.len() - 2], b[b.len() - 1]])
    }
    fn captura(&mut self, nombre: &str) {
        if let Some(dir) = std::env::var_os("MENUS_CAPTURAS") {
            let f = std::path::Path::new(&dir).join(format!("linux-{nombre}.png"));
            let _ = Command::new("grim").args(["-g", "0,0 1000x700"]).arg(f).status();
        }
    }
    fn redimensionar(&mut self, w: i32, h: i32) {
        let _ = Command::new("swaymsg").arg(format!("[title=\"menus-encima\"] resize set {w} {h}, move position 0 0")).output();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let arbol = Command::new("swaymsg").args(["-t", "get_tree", "-r"]).output().map(|o| o.stdout).unwrap_or_default();
        let arbol = String::from_utf8_lossy(&arbol);
        if let Some(i) = arbol.find("\"menus-encima\"") {
            let trozo = &arbol[arbol[..i].rfind("\"rect\"").unwrap_or(0)..];
            eprintln!("tras redimensionar: {}", trozo.chars().take(90).collect::<String>().replace(char::is_whitespace, ""));
        }
    }
}
