//! Control del experimento W2: el ejemplo `multiwebview` de wry 0.57 portado a tao 0.37.
//! Cuatro webviews hijos creados antes de `run`, como en el ejemplo de wry.
//! `--variante` cambia solo la forma de esperar del bucle:
//! - `base`: `ControlFlow::WaitUntil` de 50 ms.
//! - `poll`: `ControlFlow::Poll` (el bucle no duerme nunca).
//! - `espera`: `ControlFlow::Wait`, y un hilo despierta el bucle con `EventLoopProxy` cada 50 ms.
//!
//! Latido en cada vuelta del bucle (traza cada 1 s). Sale con 0 a los 15 s.
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

#[derive(Clone, Copy, Debug)]
enum Variante {
    Base,
    Poll,
    Espera,
}

fn variante_de_args() -> Variante {
    let mut args = std::env::args().skip(1);
    let mut variante = Variante::Base;
    while let Some(a) = args.next() {
        if a == "--variante" {
            variante = match args.next().as_deref() {
                Some("base") => Variante::Base,
                Some("poll") => Variante::Poll,
                Some("espera") => Variante::Espera,
                otro => {
                    eprintln!("--variante acepta base, poll o espera, no {otro:?}");
                    std::process::exit(64);
                }
            };
        }
    }
    variante
}

fn main() -> wry::Result<()> {
    let variante = variante_de_args();
    eprintln!("variante: {variante:?}");
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
    if let Variante::Espera = variante {
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            if proxy.send_event(()).is_err() {
                break;
            }
        });
    }
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
        *flujo = match variante {
            Variante::Base => ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50)),
            Variante::Poll => ControlFlow::Poll,
            Variante::Espera => ControlFlow::Wait,
        };
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
