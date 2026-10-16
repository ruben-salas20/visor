//! Control del experimento W2: el ejemplo `multiwebview` de wry 0.57 (4 webviews, winit 0.30).
//! `--variante` activa una sola diferencia respecto al ejemplo:
//! - `base`: el ejemplo tal cual.
//! - `ipc`: `with_html` con `window.ipc.postMessage('loaded')` al cargar, y `with_ipc_handler` que traza.
//! - `contexto`: `new_with_web_context` con un `WebContext` compartido.
//! - `bounds_rejilla`: `with_bounds` con rejilla en píxeles físicos, como en `main.rs` (2x2 para 4).
//!
//! Latido en `about_to_wait` (despierta cada 50 ms, traza cada 1 s). Sale con 0 a los 15 s.
//! Un vigilante en otro hilo sale con 4 si el latido para más de 5 s.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use wry::{
    Rect, WebContext, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
};

const URLS: [&str; 4] = [
    "https://tauri.app",
    "https://github.com/tauri-apps/wry",
    "https://twitter.com/TauriApps",
    "https://google.com",
];

const PAGINA_IPC: &str = "<!doctype html><script>window.addEventListener('load', () => window.ipc.postMessage('loaded'));</script>";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Variante {
    Base,
    Ipc,
    Contexto,
    BoundsRejilla,
}

fn variante_de_args() -> Variante {
    let mut args = std::env::args().skip(1);
    let mut variante = Variante::Base;
    while let Some(a) = args.next() {
        if a == "--variante" {
            variante = match args.next().as_deref() {
                Some("base") => Variante::Base,
                Some("ipc") => Variante::Ipc,
                Some("contexto") => Variante::Contexto,
                Some("bounds_rejilla") => Variante::BoundsRejilla,
                otro => {
                    eprintln!(
                        "--variante acepta base, ipc, contexto o bounds_rejilla, no {otro:?}"
                    );
                    std::process::exit(64);
                }
            };
        }
    }
    variante
}

/// Rectángulo del webview `i`. `logica` y `fisica` son el tamaño de la ventana en ambas unidades.
fn rect_de(variante: Variante, i: usize, logica: (u32, u32), fisica: (u32, u32)) -> Rect {
    if variante == Variante::BoundsRejilla {
        // Rejilla de 4 celdas en píxeles físicos: 2 columnas (techo de la raíz de 4) y 2 filas.
        let (cw, ch) = ((fisica.0 / 2).max(1), (fisica.1 / 2).max(1));
        return Rect {
            position: PhysicalPosition::new(
                ((i % 2) as u32 * cw) as i32,
                ((i / 2) as u32 * ch) as i32,
            )
            .into(),
            size: PhysicalSize::new(cw, ch).into(),
        };
    }
    let (w, h) = (logica.0 / 2, logica.1 / 2);
    let (x, y) = match i {
        0 => (0, 0),
        1 => (w, 0),
        2 => (0, h),
        _ => (w, h),
    };
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w, h).into(),
    }
}

#[derive(Default)]
struct State {
    variante: Option<Variante>,
    contexto: Option<WebContext>,
    window: Option<Window>,
    vistas: Vec<wry::WebView>,
    t0: Option<Instant>,
    latido: Option<Arc<AtomicU64>>,
    ultima_traza: Option<Instant>,
}

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

impl ApplicationHandler for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let variante = self.variante.unwrap_or(Variante::Base);
        let mut attributes = Window::default_attributes();
        attributes.inner_size = Some(LogicalSize::new(800, 800).into());
        let window = event_loop.create_window(attributes).unwrap();

        let fisica = window.inner_size();
        let logica = fisica.to_logical::<u32>(window.scale_factor());
        let (logica, fisica) = ((logica.width, logica.height), (fisica.width, fisica.height));
        let t0 = self.t0.unwrap_or_else(Instant::now);

        for i in 0..4 {
            let builder = match self.contexto.as_mut() {
                Some(ctx) => WebViewBuilder::new_with_web_context(ctx),
                None => WebViewBuilder::new(),
            };
            let builder = builder.with_bounds(rect_de(variante, i, logica, fisica));
            let builder = if variante == Variante::Ipc {
                builder.with_html(PAGINA_IPC).with_ipc_handler(
                    move |req: wry::http::Request<String>| {
                        eprintln!(
                            "[{:>6} ms] ipc webview {i}: {}",
                            ms(t0.elapsed()),
                            req.body()
                        );
                    },
                )
            } else {
                builder.with_url(URLS[i])
            };
            self.vistas.push(builder.build_as_child(&window).unwrap());
        }

        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                if let Some(window) = &self.window {
                    let variante = self.variante.unwrap_or(Variante::Base);
                    let logica = size.to_logical::<u32>(window.scale_factor());
                    for (i, vista) in self.vistas.iter().enumerate() {
                        vista
                            .set_bounds(rect_de(
                                variante,
                                i,
                                (logica.width, logica.height),
                                (size.width, size.height),
                            ))
                            .unwrap();
                    }
                }
            }
            WindowEvent::CloseRequested => {
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Añadido para el control: despertar cada 50 ms y latir.
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
        let t0 = *self.t0.get_or_insert_with(Instant::now);
        if let Some(latido) = &self.latido {
            latido.store(ms(t0.elapsed()), Ordering::Relaxed);
        }
        let traza = self.ultima_traza.get_or_insert_with(Instant::now);
        if traza.elapsed() >= Duration::from_secs(1) {
            eprintln!("[{:>6} ms] latido", ms(t0.elapsed()));
            self.ultima_traza = Some(Instant::now());
        }
        if t0.elapsed() >= Duration::from_secs(15) {
            println!("control ok: el bucle siguió vivo 15 s");
            std::process::exit(0);
        }
    }
}

fn main() -> wry::Result<()> {
    let variante = variante_de_args();
    eprintln!("variante: {variante:?}");

    // Vigilante: si el latido para 5 s, el bucle está bloqueado.
    let latido = Arc::new(AtomicU64::new(0));
    let vigilado = Arc::clone(&latido);
    let t0 = Instant::now();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let ahora = ms(t0.elapsed());
            let ultimo = vigilado.load(Ordering::Relaxed);
            // Armed only after the first beat: creating the first webview can take over 5 s.
            if ultimo != 0 && ahora.saturating_sub(ultimo) > 5_000 {
                eprintln!("[{ahora:>6} ms] hilo principal bloqueado desde {ultimo} ms");
                std::process::exit(4);
            }
        }
    });

    let contexto = if variante == Variante::Contexto {
        let dir = std::env::temp_dir().join(format!("control-wry-contexto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Some(WebContext::new(Some(dir)))
    } else {
        None
    };

    let event_loop = EventLoop::new().unwrap();
    let mut state = State {
        variante: Some(variante),
        contexto,
        t0: Some(t0),
        latido: Some(latido),
        ..State::default()
    };
    event_loop.run_app(&mut state).unwrap();

    Ok(())
}
