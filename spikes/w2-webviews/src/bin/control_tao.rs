//! Control del experimento W2: el ejemplo `multiwebview` de wry 0.57 portado a tao 0.37.
//! Solo la variante `base`: cuatro webviews hijos creados antes de `run`, sin cambios respecto al ejemplo.
//! El bucle usa `tao` (como `main.rs`) en vez de `winit`: si este control va bien y `main.rs` no,
//! la diferencia está en la forma de crear o alimentar el bucle, no en la librería de ventanas.
//!
//! Latido en cada vuelta del bucle (despierta cada 50 ms, traza cada 1 s). Sale con 0 a los 15 s.
//! Un vigilante en otro hilo sale con 4 si el latido para más de 5 s.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::{
    Rect, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize as WryLogicalSize},
};

const URLS: [&str; 4] = [
    "https://tauri.app",
    "https://github.com/tauri-apps/wry",
    "https://twitter.com/TauriApps",
    "https://google.com",
];

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

/// Rectángulo del webview `i` en una rejilla 2x2 de la ventana de `w x h` lógicos.
fn rect_de(i: usize, w: u32, h: u32) -> Rect {
    let (cw, ch) = (w / 2, h / 2);
    let (x, y) = match i {
        0 => (0, 0),
        1 => (cw, 0),
        2 => (0, ch),
        _ => (cw, ch),
    };
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: WryLogicalSize::new(cw, ch).into(),
    }
}

fn main() -> wry::Result<()> {
    let t0 = Instant::now();

    // Vigilante: si el latido para 5 s, el bucle está bloqueado.
    let latido = Arc::new(AtomicU64::new(0));
    let vigilado = Arc::clone(&latido);
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

    let event_loop = EventLoop::new();
    let ventana = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(800.0, 800.0))
        .build(&event_loop)
        .unwrap();

    let tamano = ventana.inner_size();
    let escala = ventana.scale_factor();
    let (w, h) = (
        (tamano.width as f64 / escala) as u32,
        (tamano.height as f64 / escala) as u32,
    );

    // Los cuatro hijos se crean antes de entrar en el bucle, como en el ejemplo de wry.
    let mut vistas = Vec::with_capacity(4);
    for (i, url) in URLS.iter().enumerate() {
        let vista = WebViewBuilder::new()
            .with_bounds(rect_de(i, w, h))
            .with_url(*url)
            .build_as_child(&ventana)?;
        vistas.push(vista);
    }
    eprintln!("[{:>6} ms] cuatro webviews creados", ms(t0.elapsed()));

    let mut ultima_traza = Instant::now();
    event_loop.run(move |evento, _, flujo| {
        *flujo = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
        latido.store(ms(t0.elapsed()), Ordering::Relaxed);
        if ultima_traza.elapsed() >= Duration::from_secs(1) {
            eprintln!("[{:>6} ms] latido", ms(t0.elapsed()));
            ultima_traza = Instant::now();
        }
        match evento {
            Event::WindowEvent {
                event: WindowEvent::Resized(tamano),
                ..
            } => {
                let escala = ventana.scale_factor();
                let (w, h) = (
                    (tamano.width as f64 / escala) as u32,
                    (tamano.height as f64 / escala) as u32,
                );
                for (i, vista) in vistas.iter().enumerate() {
                    vista.set_bounds(rect_de(i, w, h)).unwrap();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => std::process::exit(0),
            _ => {}
        }
        if t0.elapsed() >= Duration::from_secs(15) {
            println!("control ok: el bucle siguió vivo 15 s");
            std::process::exit(0);
        }
    });
}
