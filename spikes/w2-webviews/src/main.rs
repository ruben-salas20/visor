//! Experimento W2 (issue 35): cuánto cuestan N webviews hijos de WebView2 en una ventana.
//! Uso: `w2-webviews <1-10> [salida.json] [--modo entorno|contexto]`.
//! - entorno: el primer webview crea el entorno y el resto lo recibe con `with_environment`.
//! - contexto: solo el `WebContext` compartido, sin `with_environment`.
//!
//! Los webviews se crean dentro del bucle de eventos, de uno en uno, y el límite de 60 s
//! cubre también la creación. Las trazas de tiempo van a stderr.
//!
//! Códigos de salida: 0 medido (el resultado de redimensionar va en el JSON),
//! 1 error al crear la ventana o un webview, 2 no se crearon o cargaron en 60 s,
//! 3 la ventana se cerró antes, 64 argumentos inválidos.

#[cfg(not(windows))]
compile_error!("w2-webviews solo compila para Windows (WebView2).");

mod measure;
mod server;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Window, WindowBuilder};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment;
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

const LIMITE: Duration = Duration::from_secs(60);
const ESPERA_MEDIDA: Duration = Duration::from_secs(5);
const ESPERA_REDIMENSION: Duration = Duration::from_secs(1);
/// Tamaño inicial y los dos redimensionados (lógicos).
const TAMANOS: [(f64, f64); 3] = [(1200.0, 800.0), (900.0, 600.0), (1400.0, 900.0)];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Modo {
    Entorno,
    Contexto,
}

impl Modo {
    fn nombre(self) -> &'static str {
        match self {
            Modo::Entorno => "entorno",
            Modo::Contexto => "contexto",
        }
    }
}

struct Args {
    n: usize,
    salida: PathBuf,
    modo: Modo,
}

fn leer_args() -> Result<Args, String> {
    let mut posicionales = Vec::new();
    let mut modo = Modo::Entorno;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--modo" {
            modo = match it.next().as_deref() {
                Some("entorno") => Modo::Entorno,
                Some("contexto") => Modo::Contexto,
                otro => return Err(format!("--modo acepta entorno o contexto, no {otro:?}")),
            };
        } else if a.starts_with("--") {
            return Err(format!("opción desconocida: {a}"));
        } else {
            posicionales.push(a);
        }
    }
    let n = posicionales
        .first()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| (1..=10).contains(n))
        .ok_or_else(|| "falta N (1-10)".to_string())?;
    let salida = posicionales
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("w2-{n}-{}.json", modo.nombre())));
    Ok(Args { n, salida, modo })
}

fn main() {
    let Args { n, salida, modo } = match leer_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("uso: w2-webviews <1-10> [salida.json] [--modo entorno|contexto]\n{e}");
            std::process::exit(64);
        }
    };

    // Perfil de datos nuevo en cada ejecución: el entorno se crea desde cero.
    let datos = std::env::temp_dir().join(format!(
        "w2-webviews-{n}-{}-{}",
        modo.nombre(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&datos);

    let t0 = Instant::now();
    let puerto = match server::start() {
        Ok(p) => p,
        Err(e) => fallo(1, &salida, n, format!("servidor HTTP: {e}")),
    };

    let event_loop = EventLoop::new();
    let ventana = match WindowBuilder::new()
        .with_title(format!("W2 · {n} webviews · {}", modo.nombre()))
        .with_inner_size(LogicalSize::new(TAMANOS[0].0, TAMANOS[0].1))
        .build(&event_loop)
    {
        Ok(v) => v,
        Err(e) => fallo(1, &salida, n, format!("ventana: {e}")),
    };

    let (tx, rx) = mpsc::channel::<(usize, String)>();
    let mut estado = Estado {
        n,
        modo,
        t0,
        puerto,
        ventana,
        contexto: WebContext::new(Some(datos)),
        vistas: Vec::with_capacity(n),
        entorno: None,
        tx,
        rx,
        creacion_ms: Vec::with_capacity(n),
        cargas_ms: vec![None; n],
        fase: Fase::Creando,
        salida,
        medida: None,
        redimensiones: Vec::new(),
    };
    estado.traza(&format!("ventana creada; N={n}, modo {}", modo.nombre()));

    event_loop.run(move |evento, _, flujo| {
        *flujo = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
        match evento {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let codigo =
                    estado.terminar(3, Some("la ventana se cerró antes de terminar".into()));
                std::process::exit(codigo);
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => estado.colocar(),
            _ => {}
        }
        if let Some(codigo) = estado.avanzar() {
            std::process::exit(codigo);
        }
    });
}

enum Fase {
    /// Un webview por evento del bucle, hasta tener los N.
    Creando,
    Cargando,
    Asentando {
        hasta: Instant,
    },
    /// Redimensionado en curso: se comprueba `bounds()` de cada hijo cuando pasa el plazo.
    Esperando {
        paso: usize,
        hasta: Instant,
    },
}

struct Estado {
    n: usize,
    modo: Modo,
    t0: Instant,
    puerto: u16,
    ventana: Window,
    /// Se conserva viva: los webviews dependen de él.
    contexto: WebContext,
    vistas: Vec<WebView>,
    /// Entorno del primer webview; solo se usa en modo `entorno`.
    entorno: Option<ICoreWebView2Environment>,
    tx: Sender<(usize, String)>,
    rx: Receiver<(usize, String)>,
    creacion_ms: Vec<f64>,
    cargas_ms: Vec<Option<f64>>,
    fase: Fase,
    salida: PathBuf,
    medida: Option<Value>,
    redimensiones: Vec<Value>,
}

impl Estado {
    fn traza(&self, msg: &str) {
        eprintln!("[{:>9.1} ms] {msg}", ms(self.t0.elapsed()));
    }

    fn cambiar(&mut self, fase: Fase, msg: &str) {
        self.fase = fase;
        self.traza(msg);
    }

    /// Devuelve el código de salida cuando el experimento ha terminado.
    fn avanzar(&mut self) -> Option<i32> {
        let ahora = Instant::now();
        if matches!(self.fase, Fase::Creando | Fase::Cargando) && self.t0.elapsed() > LIMITE {
            let cargados = self.cargas_ms.iter().filter(|c| c.is_some()).count();
            let msg = format!(
                "no se crearon o cargaron en 60 s: {} creados, {cargados}/{} cargados",
                self.vistas.len(),
                self.n
            );
            return Some(self.terminar(2, Some(msg)));
        }
        match self.fase {
            Fase::Creando => {
                if let Err(e) = self.crear_siguiente() {
                    return Some(self.terminar(1, Some(e)));
                }
                if self.vistas.len() == self.n {
                    self.cambiar(
                        Fase::Cargando,
                        "todos los webviews creados; esperando carga",
                    );
                }
            }
            Fase::Cargando => {
                while let Ok((i, mensaje)) = self.rx.try_recv() {
                    if mensaje.starts_with("loaded") && self.cargas_ms[i].is_none() {
                        self.cargas_ms[i] = Some(ms(self.t0.elapsed()));
                        self.traza(&format!("webview {i}: cargado"));
                    }
                }
                if self.cargas_ms.iter().all(Option::is_some) {
                    self.cambiar(
                        Fase::Asentando {
                            hasta: ahora + ESPERA_MEDIDA,
                        },
                        "todos cargados; asentando antes de medir",
                    );
                }
            }
            Fase::Asentando { hasta } if ahora >= hasta => {
                self.medida = Some(self.capturar());
                self.pedir_tamano(1);
                self.cambiar(
                    Fase::Esperando {
                        paso: 1,
                        hasta: Instant::now() + ESPERA_REDIMENSION,
                    },
                    "medido; redimensionando al paso 1",
                );
            }
            Fase::Esperando { paso, hasta } if ahora >= hasta => {
                self.comprobar(paso);
                if paso + 1 < TAMANOS.len() {
                    self.pedir_tamano(paso + 1);
                    self.cambiar(
                        Fase::Esperando {
                            paso: paso + 1,
                            hasta: Instant::now() + ESPERA_REDIMENSION,
                        },
                        &format!("redimensionando al paso {}", paso + 1),
                    );
                } else {
                    return Some(self.terminar(0, None));
                }
            }
            _ => {}
        }
        None
    }

    /// Crea el webview siguiente y lo añade a la lista. Una llamada por evento del bucle.
    fn crear_siguiente(&mut self) -> Result<(), String> {
        let i = self.vistas.len();
        let tam = self.ventana.inner_size();
        let rect = rejilla(self.n, tam.width, tam.height).swap_remove(i);
        self.traza(&format!("webview {i}: creación inicio"));
        let inicio = Instant::now();
        let tx = self.tx.clone();
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.contexto)
            .with_url(format!("http://127.0.0.1:{}/?n={i}", self.puerto))
            .with_bounds(rect)
            .with_ipc_handler(move |req: wry::http::Request<String>| {
                let _ = tx.send((i, req.body().clone()));
            });
        if self.modo == Modo::Entorno {
            if let Some(env) = &self.entorno {
                builder = builder.with_environment(env.clone());
            }
        }
        let vista = builder
            .build_as_child(&self.ventana)
            .map_err(|e| format!("webview {i}: {e}"))?;
        if self.modo == Modo::Entorno && self.entorno.is_none() {
            self.entorno = Some(vista.environment());
        }
        let creado = ms(inicio.elapsed());
        self.creacion_ms.push(creado);
        self.vistas.push(vista);
        self.traza(&format!("webview {i}: creación fin ({creado:.1} ms)"));
        Ok(())
    }

    fn pedir_tamano(&self, paso: usize) {
        let (ancho, alto) = TAMANOS[paso];
        self.ventana.set_inner_size(LogicalSize::new(ancho, alto));
    }

    /// Recoloca los webviews en la rejilla del tamaño actual de la ventana.
    fn colocar(&self) {
        let tam = self.ventana.inner_size();
        for (vista, rect) in self
            .vistas
            .iter()
            .zip(rejilla(self.n, tam.width, tam.height))
        {
            let _ = vista.set_bounds(rect);
        }
    }

    fn comprobar(&mut self, paso: usize) {
        let tam = self.ventana.inner_size();
        let escala = self.ventana.scale_factor();
        let (ancho_pedido, alto_pedido) = TAMANOS[paso];
        let ventana_ok = (f64::from(tam.width) - ancho_pedido * escala).abs() <= 1.0
            && (f64::from(tam.height) - alto_pedido * escala).abs() <= 1.0;

        let mut mal = Vec::new();
        let mut reales = Vec::new();
        for (i, (vista, esperada)) in self
            .vistas
            .iter()
            .zip(rejilla(self.n, tam.width, tam.height))
            .enumerate()
        {
            let (ex, ey) = pos(&esperada);
            let (ea, eb) = tam_de(&esperada);
            match vista.bounds() {
                Ok(b) => {
                    let (x, y) = pos(&b);
                    let (w, h) = tam_de(&b);
                    let ok = (x, y, w, h) == (ex, ey, ea, eb);
                    if !ok {
                        mal.push(i);
                    }
                    reales.push(json!({ "webview": i, "pedido": [ex, ey, ea, eb], "real": [x, y, w, h], "ok": ok }));
                }
                Err(e) => {
                    mal.push(i);
                    reales.push(json!({ "webview": i, "error": e.to_string(), "ok": false }));
                }
            }
        }
        self.redimensiones.push(json!({
            "paso": paso,
            "tamano_pedido_logico": [ancho_pedido, alto_pedido],
            "escala": escala,
            "ventana_fisica": [tam.width, tam.height],
            "ventana_ok": ventana_ok,
            "bounds_ok": mal.is_empty(),
            "webviews_mal": mal,
            "bounds": reales,
        }));
    }

    fn capturar(&self) -> Value {
        match measure::medir(std::process::id()) {
            Ok(m) => {
                let todos = || std::iter::once(&m.host).chain(m.hijos.iter());
                let ws_total: u64 = todos().filter_map(|p| p.working_set).sum();
                let pb_total: u64 = todos().filter_map(|p| p.private_bytes).sum();
                let webview2 = m
                    .hijos
                    .iter()
                    .filter(|p| p.nombre.eq_ignore_ascii_case("msedgewebview2.exe"))
                    .count();
                json!({
                    "procesos_hijos": m.hijos.len(),
                    "procesos_msedgewebview2": webview2,
                    "working_set_total_bytes": ws_total,
                    "private_bytes_total": pb_total,
                    "host": proceso_json(&m.host),
                    "hijos": m.hijos.iter().map(proceso_json).collect::<Vec<_>>(),
                })
            }
            Err(e) => json!({ "error": e }),
        }
    }

    /// Escribe el JSON en el archivo y en stdout, y devuelve el código de salida.
    fn terminar(&self, codigo: i32, error: Option<String>) -> i32 {
        self.traza(&format!("fin con código {codigo}"));
        let cargas: Vec<f64> = self.cargas_ms.iter().flatten().copied().collect();
        let tiempo_carga = if cargas.len() == self.n {
            Some(cargas.iter().copied().fold(0.0, f64::max))
        } else {
            None
        };
        let redim_ok = self
            .redimensiones
            .iter()
            .all(|r| r["bounds_ok"] == true && r["ventana_ok"] == true);
        let informe = json!({
            "n": self.n,
            "modo": self.modo.nombre(),
            "exito": codigo == 0,
            "error": error,
            "creados": self.vistas.len(),
            "creacion_ms": self.creacion_ms,
            "tiempo_creacion_ms": self.creacion_ms.iter().sum::<f64>(),
            "tiempo_carga_ms": tiempo_carga,
            "cargas_ms": self.cargas_ms,
            "medida_a_los_s": ESPERA_MEDIDA.as_secs(),
            "medida": self.medida,
            "redimensionado": self.redimensiones,
            "redimensionado_ok": redim_ok,
            "entorno": match self.modo {
                Modo::Entorno => "el primer webview crea el entorno; el resto usa with_environment",
                Modo::Contexto => "solo WebContext compartido; sin with_environment",
            },
            "wry": "0.57.0",
            "tao": "0.37.1",
        });
        let texto = serde_json::to_string_pretty(&informe)
            .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"));
        println!("{texto}");
        if let Err(e) = std::fs::write(&self.salida, &texto) {
            eprintln!("no se pudo escribir {}: {e}", self.salida.display());
        }
        if let Some(msg) = informe["error"].as_str() {
            eprintln!("error: {msg}");
        }
        codigo
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Termina antes de crear la ventana o el servidor.
fn fallo(codigo: i32, salida: &PathBuf, n: usize, msg: String) -> ! {
    eprintln!("error: {msg}");
    let informe = json!({ "n": n, "exito": false, "error": msg });
    let texto = informe.to_string();
    println!("{texto}");
    let _ = std::fs::write(salida, &texto);
    std::process::exit(codigo);
}

fn proceso_json(p: &measure::Proceso) -> Value {
    json!({
        "pid": p.pid,
        "padre": p.padre,
        "nombre": p.nombre,
        "working_set_bytes": p.working_set,
        "private_bytes": p.private_bytes,
    })
}

/// Rejilla de `n` celdas en el área `ancho` x `alto` (físicos): columnas = techo de la raíz de n.
fn rejilla(n: usize, ancho: u32, alto: u32) -> Vec<Rect> {
    let columnas = (n as f64).sqrt().ceil() as u32;
    let filas = (n as u32).div_ceil(columnas);
    let (cw, ch) = ((ancho / columnas).max(1), (alto / filas).max(1));
    (0..n as u32)
        .map(|i| Rect {
            position: PhysicalPosition::new(
                ((i % columnas) * cw) as i32,
                ((i / columnas) * ch) as i32,
            )
            .into(),
            size: PhysicalSize::new(cw, ch).into(),
        })
        .collect()
}

fn pos(r: &Rect) -> (i32, i32) {
    let p = r.position.to_physical::<i32>(1.0);
    (p.x, p.y)
}

fn tam_de(r: &Rect) -> (i32, i32) {
    let s = r.size.to_physical::<i32>(1.0);
    (s.width, s.height)
}
