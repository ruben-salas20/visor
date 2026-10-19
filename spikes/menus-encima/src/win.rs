//! Windows: winit + two WebView2 child views. The page below, the frame above
//! it covering the whole window, cut with a window region (`SetWindowRgn`) to
//! «window minus holes plus open menus». The region decides both what is drawn
//! and where the frame takes input.
//!
//! `--modo region` (default): frame transparent, with the region.
//! `--modo transparente`: frame transparent, no region (does WebView2 blend
//! sibling views?). `--modo opaco`: frame with an opaque background and the region.

use crate::prueba::{self, Entrada, Registro};
use crate::{MARCO, PAGINA, caja, html};
use serde_json::Value;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, GetDC, GetPixel, HGDIOBJ, RGN_DIFF, RGN_OR, ReleaseDC,
    SetWindowRgn,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{HWND_TOP, SWP_NOMOVE, SWP_NOSIZE, SetCursorPos, SetForegroundWindow, SetWindowPos};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowId};
use wry::http::Request;
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewExtWindows};

enum Ev {
    Estado(Value),
    Redimensionar(i32, i32),
    Fin(Value),
}

struct App {
    reg: Registro,
    salida: Option<String>,
    modo: String,
    proxy: EventLoopProxy<Ev>,
    contexto: WebContext,
    window: Option<Window>,
    pagina: Option<WebView>,
    marco: Option<WebView>,
}

fn caja_logica(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { position: LogicalPosition::new(x, y).into(), size: LogicalSize::new(w, h).into() }
}

fn hwnd_de(window: &Window) -> HWND {
    match window.window_handle().unwrap().as_raw() {
        RawWindowHandle::Win32(h) => HWND(h.hwnd.get() as _),
        _ => unreachable!(),
    }
}

impl App {
    fn colocar(&self) {
        let (Some(w), Some(p), Some(m)) = (&self.window, &self.pagina, &self.marco) else { return };
        let t = w.inner_size().to_logical::<f64>(w.scale_factor());
        let _ = p.set_bounds(caja_logica(200.0, 40.0, (t.width - 200.0).max(1.0), (t.height - 40.0).max(1.0)));
        let _ = m.set_bounds(caja_logica(0.0, 0.0, t.width, t.height));
    }

    /// The frame's region: the window, minus the holes, plus the open menus (rounded).
    fn aplicar(&self, v: &Value) {
        if self.modo == "transparente" {
            return;
        }
        let (Some(w), Some(m)) = (&self.window, &self.marco) else { return };
        let s = w.scale_factor();
        let f = |x: i32| (x as f64 * s).round() as i32;
        let Some([_, _, ancho, alto]) = caja(&serde_json::json!([0, 0, v["ventana"][0], v["ventana"][1]])) else { return };
        unsafe {
            let region = CreateRectRgn(0, 0, f(ancho), f(alto));
            for c in v["huecos"].as_array().into_iter().flatten().filter_map(caja) {
                let r = CreateRectRgn(f(c[0]), f(c[1]), f(c[0] + c[2]), f(c[1] + c[3]));
                CombineRgn(Some(region), Some(region), Some(r), RGN_DIFF);
                let _ = DeleteObject(HGDIOBJ(r.0));
            }
            for c in v["solidos"].as_array().into_iter().flatten().filter_map(caja) {
                let r = CreateRoundRectRgn(f(c[0]), f(c[1]), f(c[0] + c[2]) + 1, f(c[1] + c[3]) + 1, f(16), f(16));
                CombineRgn(Some(region), Some(region), Some(r), RGN_OR);
                let _ = DeleteObject(HGDIOBJ(r.0));
            }
            // The system owns the region from here on.
            SetWindowRgn(HWND(m.hwnd().0), Some(region), true);
        }
    }
}

impl ApplicationHandler<Ev> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("menus-encima")
            .with_position(LogicalPosition::new(0.0, 0.0))
            .with_decorations(false)
            .with_inner_size(LogicalSize::new(1000.0, 700.0));
        let window = event_loop.create_window(attrs).expect("ventana");
        let r = self.reg.clone();
        let pagina = WebViewBuilder::new_with_web_context(&mut self.contexto)
            .with_custom_protocol("pagina".into(), |_, _| html(PAGINA))
            .with_url("http://pagina.localhost/")
            .with_bounds(caja_logica(200.0, 40.0, 800.0, 660.0))
            .with_ipc_handler(move |req: Request<String>| {
                if let Ok(v) = serde_json::from_str(req.body()) {
                    r.lock().unwrap().push(("pagina", v));
                }
            })
            .build_as_child(&window)
            .expect("página");
        let (r, p) = (self.reg.clone(), self.proxy.clone());
        let mut marco = WebViewBuilder::new_with_web_context(&mut self.contexto)
            .with_custom_protocol("marco".into(), |_, _| html(MARCO))
            .with_url("http://marco.localhost/")
            .with_bounds(caja_logica(0.0, 0.0, 1000.0, 700.0))
            .with_ipc_handler(move |req: Request<String>| {
                if let Ok(v) = serde_json::from_str::<Value>(req.body()) {
                    if v["tipo"] == "estado" {
                        let _ = p.send_event(Ev::Estado(v.clone()));
                    }
                    r.lock().unwrap().push(("marco", v));
                }
            });
        marco = if self.modo == "opaco" { marco.with_background_color((0xe8, 0xe4, 0xda, 255)) } else { marco.with_transparent(true) };
        let marco = marco.build_as_child(&window).expect("marco");
        unsafe {
            let _ = SetWindowPos(HWND(marco.hwnd().0), Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = SetForegroundWindow(hwnd_de(&window));
        }
        let padre = hwnd_de(&window).0 as isize;
        self.window = Some(window);
        self.pagina = Some(pagina);
        self.marco = Some(marco);

        if self.salida.is_some() {
            let (reg, p) = (self.reg.clone(), self.proxy.clone());
            let px = self.proxy.clone();
            std::thread::spawn(move || {
                let mut e = Win { padre, proxy: px };
                let informe = prueba::ejecutar(reg, &mut e);
                let _ = p.send_event(Ev::Fin(informe));
            });
            let p = self.proxy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(150));
                let _ = p.send_event(Ev::Fin(serde_json::json!({ "ok": false, "fallos": ["tiempo agotado"] })));
            });
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, ev: Ev) {
        match ev {
            Ev::Estado(v) => self.aplicar(&v),
            Ev::Redimensionar(w, h) => {
                if let Some(win) = &self.window {
                    let _ = win.request_inner_size(LogicalSize::new(w as f64, h as f64));
                }
            }
            Ev::Fin(mut v) => {
                v["modo"] = Value::String(self.modo.clone());
                let texto = serde_json::to_string_pretty(&v).unwrap();
                println!("{texto}");
                if let Some(f) = &self.salida {
                    std::fs::write(f, texto).unwrap();
                }
                std::process::exit(if v["ok"] == true { 0 } else { 1 });
            }
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(_) => self.colocar(),
            WindowEvent::CloseRequested => std::process::exit(0),
            _ => {}
        }
    }
}

/// Real input with SendInput; pixels read from the screen, as DWM composed it.
struct Win {
    padre: isize,
    proxy: EventLoopProxy<Ev>,
}

impl Win {
    fn pantalla(&self, x: i32, y: i32) -> (i32, i32) {
        unsafe {
            let h = HWND(self.padre as _);
            let s = GetDpiForWindow(h) as f64 / 96.0;
            let mut p = POINT { x: (x as f64 * s).round() as i32, y: (y as f64 * s).round() as i32 };
            let _ = ClientToScreen(h, &mut p);
            (p.x, p.y)
        }
    }

    fn raton(&self, flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS, datos: i32) {
        let i = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: datos as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        };
        unsafe { SendInput(&[i], size_of::<INPUT>() as i32) };
    }
}

impl Entrada for Win {
    fn mover(&mut self, x: i32, y: i32) {
        let (sx, sy) = self.pantalla(x, y);
        unsafe {
            let _ = SetCursorPos(sx, sy);
        }
        // A zero relative move, so the view gets a real mouse-move event too.
        self.raton(windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_MOVE, 0);
    }
    fn pulsar(&mut self) {
        self.raton(MOUSEEVENTF_LEFTDOWN, 0);
    }
    fn soltar(&mut self) {
        self.raton(MOUSEEVENTF_LEFTUP, 0);
    }
    fn rueda(&mut self, pasos: i32) {
        for _ in 0..pasos {
            self.raton(MOUSEEVENTF_WHEEL, -120);
            std::thread::sleep(Duration::from_millis(40));
        }
    }
    fn teclear(&mut self, texto: &str) {
        for c in texto.encode_utf16() {
            for up in [false, true] {
                let flags = if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE };
                let i = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: c, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
                };
                unsafe { SendInput(&[i], size_of::<INPUT>() as i32) };
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    fn pixel(&mut self, x: i32, y: i32) -> Option<[u8; 3]> {
        let (sx, sy) = self.pantalla(x, y);
        unsafe {
            let dc = GetDC(None);
            let c = GetPixel(dc, sx, sy).0;
            ReleaseDC(None, dc);
            (c != 0xFFFF_FFFF).then(|| [(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8])
        }
    }
    fn redimensionar(&mut self, w: i32, h: i32) {
        let _ = self.proxy.send_event(Ev::Redimensionar(w, h));
    }
}

pub fn main(salida: Option<String>) {
    let args: Vec<String> = std::env::args().collect();
    let modo = args.iter().position(|a| a == "--modo").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "region".into());
    let event_loop = EventLoop::<Ev>::with_user_event().build().expect("bucle");
    let datos = std::env::temp_dir().join(format!("menus-encima-{}", std::process::id()));
    let mut app = App {
        reg: Default::default(),
        salida,
        modo,
        proxy: event_loop.create_proxy(),
        contexto: WebContext::new(Some(datos)),
        window: None,
        pagina: None,
        marco: None,
    };
    event_loop.run_app(&mut app).expect("bucle");
}
