//! Prototipo U1 (issue 35, opción A): el marco de la interfaz es una página HTML.
//! Una ventana tao con un `gtk::Fixed`. Dentro: el webview del marco (ocupa todo)
//! y dos webviews de página encima del hueco central. Rust decide qué acepta cada origen.

mod server;

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::time::Duration;
use std::thread;

use gtk::prelude::*;
use serde_json::{json, Value};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use tao::platform::unix::WindowExtUnix;
use tao::window::WindowBuilder;
use wry::{Rect, WebView, WebViewBuilder, WebViewBuilderExtUnix, WebViewExtUnix};

const TITULO: &str = "U1 marco HTML";

/// Tarea que el hilo de pruebas pide ejecutar en el hilo principal (GTK y WebKit viven allí).
struct Hacer(Box<dyn FnOnce(&Compartido) + Send>);
type Proxy = EventLoopProxy<Hacer>;

/// Estado que decide Rust. Lo cambian solo los mensajes validados.
struct Estado {
    puerto: u16,
    listo: bool,
    arbol: Vec<String>,
    paneles: [Panel; 2],
    siguiente: u32,
    rectas: [Option<[f64; 4]>; 2],
    mostrando: [String; 2],
    cargadas: Vec<(usize, String)>,
    soltados: Vec<String>,
    rechazos: Vec<String>,
}

#[derive(Default)]
struct Panel {
    pestanas: Vec<Pestana>,
    activa: Option<u32>,
}

#[derive(Clone)]
struct Pestana {
    id: u32,
    ruta: String,
}

#[derive(Default)]
struct Vistas {
    marco: Option<WebView>,
    paginas: Vec<WebView>,
    // Hace falta para mover las páginas: gtk::Fixed ignora la posición de size_allocate.
    fijo: Option<gtk::Fixed>,
}

struct Compartido {
    est: Rc<RefCell<Estado>>,
    vis: Rc<RefCell<Vistas>>,
    ventana: tao::window::Window,
}

impl Estado {
    fn url_de(&self, ruta: &str) -> String {
        format!("http://127.0.0.1:{}/boveda/{ruta}", self.puerto)
    }

    fn destino(&self, lado: usize) -> String {
        let p = &self.paneles[lado];
        match p.activa.and_then(|id| p.pestanas.iter().find(|t| t.id == id)) {
            Some(t) => self.url_de(&t.ruta),
            None => "about:blank".to_string(),
        }
    }

    fn json_marco(&self) -> Value {
        let paneles: Vec<Value> = self
            .paneles
            .iter()
            .map(|p| {
                let pestanas: Vec<Value> = p
                    .pestanas
                    .iter()
                    .map(|t| json!({"id": t.id, "ruta": t.ruta, "activa": Some(t.id) == p.activa}))
                    .collect();
                json!({ "pestanas": pestanas })
            })
            .collect();
        json!({ "arbol": self.arbol, "paneles": paneles })
    }

    fn rechazar(&mut self, origen: &str, motivo: &str, cuerpo: &str) -> Result<(), String> {
        let linea = format!("{origen}: {motivo}: {cuerpo}");
        eprintln!("RECHAZADO {linea}");
        self.rechazos.push(linea);
        Err(motivo.to_string())
    }
}

/// Acciones que pueden cambiar el modelo de pestañas. Cada una valida antes de tocar nada.
fn abrir_en(e: &mut Estado, lado: usize, ruta: &str) -> Result<(), String> {
    if !e.arbol.iter().any(|r| r == ruta) {
        return Err(format!("ruta fuera de la bóveda: {ruta}"));
    }
    let id = e.siguiente;
    e.siguiente += 1;
    let p = &mut e.paneles[lado];
    p.pestanas.push(Pestana { id, ruta: ruta.to_string() });
    p.activa = Some(id);
    Ok(())
}

fn buscar(e: &mut Estado, lado: usize, id: u32) -> Result<usize, String> {
    e.paneles[lado]
        .pestanas
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| format!("no hay pestaña {id} en el panel {lado}"))
}

fn cerrar(e: &mut Estado, lado: usize, id: u32) -> Result<(), String> {
    let pos = buscar(e, lado, id)?;
    let p = &mut e.paneles[lado];
    p.pestanas.remove(pos);
    if p.activa == Some(id) {
        p.activa = p.pestanas.last().map(|t| t.id);
    }
    Ok(())
}

fn lado_valido(v: Option<u64>) -> Result<usize, String> {
    match v {
        Some(n @ 0..=1) => Ok(n as usize),
        _ => Err("panel no válido".to_string()),
    }
}

fn texto<'a>(m: &'a Value, campo: &str) -> Result<&'a str, String> {
    m[campo].as_str().ok_or_else(|| format!("falta el campo {campo}"))
}

/// Mensajes del marco. Solo el webview del marco puede enviarlos.
static INICIO: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
/// `--abrir RUTA`: opens RUTA on both sides once the frame is ready (for measuring against visor).
static ABRIR: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// With U1_TIEMPOS=<file>, appends `<event> <ms since main> <unix ms>` once per event; the unix time lets
/// the measuring script count from the moment it spawned the process.
fn marcar_tiempo(evento: &str) {
    use std::sync::Mutex;
    static VISTOS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let Ok(ruta) = std::env::var("U1_TIEMPOS") else { return };
    let mut vistos = VISTOS.lock().unwrap();
    if vistos.iter().any(|v| v == evento) {
        return;
    }
    vistos.push(evento.to_string());
    let ms = INICIO.get().map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(ruta) {
        use std::io::Write;
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        let _ = writeln!(f, "{evento} {ms:.1} {unix:.1}");
    }
}

fn mensaje_marco(est: &RefCell<Estado>, vis: &RefCell<Vistas>, cuerpo: &str) {
    let m: Value = match serde_json::from_str(cuerpo) {
        Ok(v) => v,
        Err(_) => {
            let _ = est.borrow_mut().rechazar("marco", "no es JSON", cuerpo);
            return;
        }
    };
    let accion = m["accion"].as_str().unwrap_or("").to_string();
    let resultado: Result<(), String> = (|| {
        let mut e = est.borrow_mut();
        match accion.as_str() {
            "listo" => {
                marcar_tiempo("listo");
                e.listo = true;
                if let Some(ruta) = ABRIR.get() {
                    abrir_en(&mut e, 0, ruta)?;
                    abrir_en(&mut e, 1, ruta)?;
                }
                return Ok(());
            }
            "recolocar" => {
                if e.listo {
                    marcar_tiempo("pintado");
                }
                let rectas = m["rectas"].as_array().ok_or("faltan rectas")?;
                for (i, r) in rectas.iter().enumerate().take(2) {
                    let v = [r["x"].as_f64(), r["y"].as_f64(), r["w"].as_f64(), r["h"].as_f64()];
                    if let [Some(x), Some(y), Some(w), Some(h)] = v {
                        e.rectas[i] = Some([x, y, w, h]);
                    }
                }
                return Ok(());
            }
            "abrir" | "nueva" => {
                let lado = lado_valido(m["lado"].as_u64())?;
                abrir_en(&mut e, lado, texto(&m, "ruta")?)
            }
            "activar" => {
                let lado = lado_valido(m["lado"].as_u64())?;
                let id = m["id"].as_u64().ok_or("falta id")? as u32;
                buscar(&mut e, lado, id)?;
                e.paneles[lado].activa = Some(id);
                Ok(())
            }
            "cerrar" => {
                let lado = lado_valido(m["lado"].as_u64())?;
                let id = m["id"].as_u64().ok_or("falta id")? as u32;
                cerrar(&mut e, lado, id)
            }
            "cerrar_activa" => {
                let lado = lado_valido(m["lado"].as_u64())?;
                match e.paneles[lado].activa {
                    Some(id) => cerrar(&mut e, lado, id),
                    None => Ok(()),
                }
            }
            "soltar" => {
                // Alternativa por coordenadas: el marco envía la posición del puntero al soltar.
                let x = m["x"].as_f64().ok_or("falta x")?;
                let y = m["y"].as_f64().ok_or("falta y")?;
                let lado = (0..2)
                    .find(|&i| match e.rectas[i] {
                        Some([rx, ry, rw, rh]) => x >= rx && x <= rx + rw && y >= ry && y <= ry + rh,
                        None => false,
                    })
                    .ok_or("el punto no cae en ningún panel")?;
                let ruta = texto(&m, "ruta")?.to_string();
                abrir_en(&mut e, lado, &ruta)?;
                e.soltados.push(format!("coordenadas:panel{lado}:{ruta}"));
                Ok(())
            }
            _ => Err(format!("acción desconocida: {accion}")),
        }
    })();
    match resultado {
        Ok(()) => {
            let recolocar = accion == "recolocar";
            if !recolocar {
                sincronizar(est, vis);
            } else {
                aplicar_rectas(est, vis);
            }
        }
        Err(motivo) => {
            let _ = est.borrow_mut().rechazar("marco", &motivo, cuerpo);
        }
    }
}

/// Mensajes de una página. El lado lo fija el webview que envía, no el mensaje.
fn mensaje_pagina(est: &RefCell<Estado>, vis: &RefCell<Vistas>, lado: usize, cuerpo: &str) {
    let m: Value = match serde_json::from_str(cuerpo) {
        Ok(v) => v,
        Err(_) => {
            let _ = est.borrow_mut().rechazar(&format!("página {lado}"), "no es JSON", cuerpo);
            return;
        }
    };
    let accion = m["accion"].as_str().unwrap_or("").to_string();
    let origen = format!("página {lado}");
    let resultado: Result<(), String> = (|| {
        let mut e = est.borrow_mut();
        match accion.as_str() {
            "cargada" => {
                marcar_tiempo(&format!("pagina{lado}"));
                let ruta = texto(&m, "ruta")?.to_string();
                if e.arbol.contains(&ruta) {
                    e.cargadas.push((lado, ruta));
                }
                Ok(())
            }
            "cerrar_pestana" => match e.paneles[lado].activa {
                Some(id) => cerrar(&mut e, lado, id),
                None => Ok(()),
            },
            "soltar_en_pagina" => {
                let ruta = texto(&m, "ruta")?.to_string();
                abrir_en(&mut e, lado, &ruta)?;
                e.soltados.push(format!("html5:panel{lado}:{ruta}"));
                Ok(())
            }
            _ => Err(format!("acción no permitida desde una página: {accion}")),
        }
    })();
    match resultado {
        Ok(()) => sincronizar(est, vis),
        Err(motivo) => {
            let _ = est.borrow_mut().rechazar(&origen, &motivo, cuerpo);
        }
    }
}

/// Carga en cada webview de página la pestaña activa de su panel, y manda el estado al marco.
fn sincronizar(est: &RefCell<Estado>, vis: &RefCell<Vistas>) {
    let (cambios, json, listo) = {
        let mut e = est.borrow_mut();
        let mut cambios = Vec::new();
        for lado in 0..2 {
            let destino = e.destino(lado);
            if e.mostrando[lado] != destino {
                e.mostrando[lado] = destino.clone();
                cambios.push((lado, destino));
            }
        }
        (cambios, e.json_marco(), e.listo)
    };
    let v = vis.borrow();
    for (lado, url) in cambios {
        if let Some(p) = v.paginas.get(lado) {
            let _ = p.load_url(&url);
        }
    }
    if listo {
        if let Some(m) = &v.marco {
            let _ = m.evaluate_script(&format!("window.marco.pintar({json})"));
        }
    }
}

/// Recibe las rectas del hueco (en coordenadas de ventana) y coloca los webviews de página.
fn aplicar_rectas(est: &RefCell<Estado>, vis: &RefCell<Vistas>) {
    let rectas = est.borrow().rectas;
    let v = vis.borrow();
    for (lado, r) in rectas.iter().enumerate() {
        if let (Some([x, y, w, h]), Some(p)) = (r, v.paginas.get(lado)) {
            // El tamaño pedido a GTK hace falta: si no, el siguiente reparto de Fixed lo devuelve a 1x1.
            let widget = p.webview();
            if let Some(f) = &v.fijo {
                f.move_(&widget, x.round() as i32, y.round() as i32);
            }
            widget.set_size_request(w.round().max(1.0) as i32, h.round().max(1.0) as i32);
            let _ = p.set_bounds(caja(*x, *y, *w, *h));
        }
    }
}

fn caja(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: tao::dpi::LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

/// Genera 200 notas en 10 carpetas. Se rehace en cada arranque, así siempre coincide con el árbol.
fn generar_boveda(raiz: &Path) -> std::io::Result<Vec<String>> {
    let destino = raiz.join("boveda");
    let _ = fs::remove_dir_all(&destino);
    let mut rutas = Vec::new();
    for c in 1..=10 {
        let carpeta = format!("carpeta-{c:02}");
        fs::create_dir_all(destino.join(&carpeta))?;
        for n in 1..=20 {
            let k = (c - 1) * 20 + n;
            let ruta = format!("{carpeta}/nota-{k:03}.html");
            let html = format!(
                "<!doctype html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n<title>nota {k:03}</title>\n</head>\n<body>\n<h1>Nota {k:03}</h1>\n<p>Texto de prueba de la {carpeta}.</p>\n<script src=\"/cliente-pagina.js\"></script>\n</body>\n</html>\n"
            );
            fs::write(destino.join(&ruta), html)?;
            rutas.push(ruta);
        }
    }
    rutas.sort();
    Ok(rutas)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    INICIO.get_or_init(std::time::Instant::now);
    let args: Vec<String> = std::env::args().collect();
    if let Some(ruta) = args.iter().position(|a| a == "--abrir").and_then(|i| args.get(i + 1)) {
        let _ = ABRIR.set(ruta.clone());
    }
    let salida_prueba = args.iter().position(|a| a == "--prueba").and_then(|i| args.get(i + 1)).cloned();
    let raiz: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    let arbol = generar_boveda(&raiz)?;
    let puerto = server::start(raiz.clone())?;

    let estado = Rc::new(RefCell::new(Estado {
        puerto,
        listo: false,
        arbol,
        paneles: [Panel::default(), Panel::default()],
        siguiente: 1,
        rectas: [None, None],
        mostrando: ["about:blank".to_string(), "about:blank".to_string()],
        cargadas: Vec::new(),
        soltados: Vec::new(),
        rechazos: Vec::new(),
    }));
    let vistas = Rc::new(RefCell::new(Vistas::default()));

    let event_loop: EventLoop<Hacer> = tao::event_loop::EventLoopBuilder::<Hacer>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let ventana = WindowBuilder::new()
        .with_title(TITULO)
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_decorations(false)
        .build(&event_loop)?;

    // The frame is the overlay's main child, so it follows the window size and sets no minimum.
    // Pages live in a Fixed overlay: overlay children do not count towards the window's size request.
    let capas = gtk::Overlay::new();
    let base = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let fijo = gtk::Fixed::new();
    capas.add(&base);
    capas.add_overlay(&fijo);
    capas.set_overlay_pass_through(&fijo, true);
    let caja_tao = ventana.default_vbox().ok_or("tao no expone la caja GTK")?;
    caja_tao.pack_start(&capas, true, true, 0);
    capas.show_all();
    vistas.borrow_mut().fijo = Some(fijo.clone());

    // `--marco simple|react|svelte`: the same IPC protocol, three implementations of the frame.
    let marco_elegido = args.iter().position(|a| a == "--marco").and_then(|i| args.get(i + 1)).map(String::as_str).unwrap_or("simple");
    let ruta_marco = match marco_elegido {
        "simple" => "marco.html",
        "react" => "react/index.html",
        "svelte" => "svelte/index.html",
        otro => return Err(format!("--marco acepta simple, react o svelte, no {otro}").into()),
    };
    let url_marco = format!("http://127.0.0.1:{puerto}/{ruta_marco}");
    // One context for every view, as visor does: separate contexts start a network process each.
    let mut contexto = wry::WebContext::new(None);
    // The built frames live in their own folder and may navigate inside it; the simple one is a single file.
    let prefijo_marco = match ruta_marco.rsplit_once('/') {
        Some((dir, _)) => format!("http://127.0.0.1:{puerto}/{dir}/"),
        None => url_marco.clone(),
    };
    let est_m = estado.clone();
    let vis_m = vistas.clone();
    let marco = WebViewBuilder::new_with_web_context(&mut contexto)
        .with_url(&url_marco)
        .with_navigation_handler({
            let prefijo = prefijo_marco.clone();
            move |url: String| url.starts_with(&prefijo)
        })
        .with_ipc_handler(move |req| mensaje_marco(&est_m, &vis_m, req.body()))
        .build_gtk(&base)?;
    vistas.borrow_mut().marco = Some(marco);

    for lado in 0..2usize {
        let est_p = estado.clone();
        let vis_p = vistas.clone();
        let prefijo_boveda = format!("http://127.0.0.1:{puerto}/boveda/");
        let prefijo_hostil = format!("http://127.0.0.1:{puerto}/hostil.html");
        let pagina = WebViewBuilder::new_with_web_context(&mut contexto)
            .with_url("about:blank")
            .with_bounds(caja(0.0, 0.0, 1.0, 1.0))
            .with_navigation_handler(move |url: String| {
                url == "about:blank" || url.starts_with(&prefijo_boveda) || url.starts_with(&prefijo_hostil)
            })
            .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |req| mensaje_pagina(&est_p, &vis_p, lado, req.body()))
            .build_gtk(&fijo)?;
        vistas.borrow_mut().paginas.push(pagina);
    }

    let compartido = Compartido { est: estado.clone(), vis: vistas.clone(), ventana };
    if let Some(ruta_salida) = salida_prueba {
        let proxy2 = proxy.clone();
        thread::spawn(move || {
            let informe = guion(&proxy2);
            let _ = fs::write(&ruta_salida, serde_json::to_string_pretty(&informe).unwrap_or_default());
            println!("{}", serde_json::to_string_pretty(&informe).unwrap_or_default());
            std::process::exit(0);
        });
    }

    event_loop.run(move |event, _, control| {
        *control = ControlFlow::Wait;
        match event {
            Event::UserEvent(Hacer(tarea)) => tarea(&compartido),
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control = ControlFlow::Exit,
            _ => {}
        }
        let _ = &compartido;
    });
}

// ---------------------------------------------------------------------------
// Guion de pruebas (modo --prueba). Corre en otro hilo; las acciones de GTK
// y de WebKit se piden al hilo principal con `ejecutar` y `js`.
// Los clics y el teclado reales salen por sway (swaymsg) y wtype.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Obj {
    Marco,
    Pagina(usize),
}

fn ejecutar<R: Send + 'static>(proxy: &Proxy, f: impl FnOnce(&Compartido) -> R + Send + 'static) -> R {
    let (tx, rx) = channel();
    let _ = proxy.send_event(Hacer(Box::new(move |c| {
        let _ = tx.send(f(c));
    })));
    rx.recv_timeout(Duration::from_secs(10)).expect("el hilo principal no respondió")
}

/// Evalúa JS en un webview y devuelve el valor. El script debe acabar en `JSON.stringify(...)`.
fn js(proxy: &Proxy, obj: Obj, script: &str) -> Value {
    let (tx, rx) = channel::<String>();
    let script = script.to_string();
    let _ = proxy.send_event(Hacer(Box::new(move |c| {
        let vis = c.vis.borrow();
        let wv = match obj {
            Obj::Marco => vis.marco.as_ref(),
            Obj::Pagina(i) => vis.paginas.get(i),
        };
        if let Some(wv) = wv {
            let _ = wv.evaluate_script_with_callback(&script, move |r| {
                let _ = tx.send(r);
            });
        }
    })));
    let raw = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::String(s)) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        Ok(v) => v,
        Err(_) => Value::String(raw),
    }
}

fn sway(args: &[&str]) -> String {
    Command::new("swaymsg")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn buscar_ventana(n: &Value) -> Option<[f64; 4]> {
    if n["name"].as_str() == Some(TITULO) {
        let r = &n["rect"];
        return Some([r["x"].as_f64()?, r["y"].as_f64()?, r["width"].as_f64()?, r["height"].as_f64()?]);
    }
    for clave in ["nodes", "floating_nodes"] {
        if let Some(hijos) = n[clave].as_array() {
            for h in hijos {
                if let Some(r) = buscar_ventana(h) {
                    return Some(r);
                }
            }
        }
    }
    None
}

/// Posición y tamaño de la ventana en coordenadas del escritorio de sway.
fn ventana_sway() -> [f64; 4] {
    let arbol: Value = serde_json::from_str(&sway(&["-t", "get_tree"])).unwrap_or(Value::Null);
    buscar_ventana(&arbol).unwrap_or([0.0; 4])
}

fn puntero(x: f64, y: f64) {
    let [wx, wy, _, _] = ventana_sway();
    sway(&["seat", "-", "cursor", "set", &(wx + x).round().to_string(), &(wy + y).round().to_string()]);
}

fn boton(accion: &str) -> String {
    sway(&["seat", "-", "cursor", accion, "BTN_LEFT"])
}

fn clic(x: f64, y: f64) {
    puntero(x, y);
    thread::sleep(Duration::from_millis(100));
    boton("press");
    thread::sleep(Duration::from_millis(100));
    boton("release");
}

/// Arrastre con el ratón: pulsa en (sx,sy), avanza en pasos y suelta en (tx,ty).
fn arrastrar(sx: f64, sy: f64, tx: f64, ty: f64) -> String {
    puntero(sx, sy);
    thread::sleep(Duration::from_millis(200));
    let mut salida = boton("press");
    thread::sleep(Duration::from_millis(200));
    for paso in 1..=12 {
        let f = paso as f64 / 12.0;
        puntero(sx + (tx - sx) * f, sy + (ty - sy) * f);
        thread::sleep(Duration::from_millis(50));
    }
    thread::sleep(Duration::from_millis(200));
    salida.push_str(&boton("release"));
    salida
}

fn captura(nombre: &str) {
    let [x, y, w, h] = ventana_sway();
    let destino = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capturas").join(nombre);
    let _ = Command::new("grim")
        .args(["-g", &format!("{},{} {}x{}", x as i64, y as i64, w as i64, h as i64)])
        .arg(destino)
        .status();
}

fn fila(filas: &mut Vec<Value>, prueba: &str, resultado: &str, evidencia: Value) {
    filas.push(json!({ "prueba": prueba, "resultado": resultado, "evidencia": evidencia }));
}

fn veredicto(ok: bool) -> &'static str {
    if ok { "funciona" } else { "no funciona" }
}

fn cuantas(proxy: &Proxy, lado: usize) -> usize {
    ejecutar(proxy, move |c| c.est.borrow().paneles[lado].pestanas.len())
}

fn esperar(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

fn guion(proxy: &Proxy) -> Value {
    let mut filas = Vec::new();
    esperar(1500);
    let _ = std::process::Command::new("swaymsg").args(["[title=\"U1 marco HTML\"] focus"]).output();
    esperar(300);

    // (a) Marco y dos webviews de página.
    let listo = ejecutar(proxy, |c| c.est.borrow().listo);
    let anchos = js(proxy, Obj::Marco, "JSON.stringify([...document.querySelectorAll('.contenido')].map(e=>Math.round(e.getBoundingClientRect().width)))");
    let estados = [js(proxy, Obj::Pagina(0), "JSON.stringify(document.readyState)"), js(proxy, Obj::Pagina(1), "JSON.stringify(document.readyState)")];
    let arbol_n = ejecutar(proxy, |c| c.est.borrow().arbol.len());
    captura("a-marco.png");
    let ok_a = listo
        && arbol_n == 200
        && anchos.as_array().map(|a| a.len() == 2 && a.iter().all(|w| w.as_f64().unwrap_or(0.0) > 0.0)).unwrap_or(false)
        && estados.iter().all(|e| e == "complete");
    fila(
        &mut filas,
        "a) marco y dos webviews de página en su sitio",
        veredicto(ok_a),
        json!({ "marco_listo": listo, "nodos_arbol": arbol_n, "anchos_paneles": anchos, "estados_paginas": estados.iter().map(|e| e.to_string()).collect::<Vec<_>>() }),
    );

    // (b) Crear, activar y cerrar pestañas (con clics DOM dentro del marco).
    js(proxy, Obj::Marco, "document.querySelector('.item[data-ruta=\"carpeta-02/nota-021.html\"]').click(); JSON.stringify(1)");
    esperar(500);
    let t1 = cuantas(proxy, 0);
    js(proxy, Obj::Marco, "document.querySelector('.panel[data-lado=\"0\"] .nueva').click(); JSON.stringify(1)");
    esperar(500);
    let t2 = cuantas(proxy, 0);
    js(proxy, Obj::Marco, "document.querySelectorAll('.panel[data-lado=\"0\"] .pestana')[0].click(); JSON.stringify(1)");
    esperar(300);
    let activa_tras_activar = ejecutar(proxy, |c| c.est.borrow().paneles[0].activa);
    js(proxy, Obj::Marco, "document.querySelectorAll('.panel[data-lado=\"0\"] .cerrar')[1].click(); JSON.stringify(1)");
    esperar(800);
    let t3 = cuantas(proxy, 0);
    let ruta_pagina = js(proxy, Obj::Pagina(0), "JSON.stringify(location.pathname)");
    captura("b-pestanas.png");
    let ok_b = t1 == 1 && t2 == 2 && t3 == 1 && activa_tras_activar.is_some() && ruta_pagina == "/boveda/carpeta-02/nota-021.html";
    fila(
        &mut filas,
        "b) crear, activar y cerrar pestañas",
        veredicto(ok_b),
        json!({ "pestanas_tras_abrir": t1, "tras_nueva": t2, "tras_cerrar": t3, "pagina_panel0": ruta_pagina }),
    );

    // (c) Abrir archivo desde el árbol.
    js(proxy, Obj::Marco, "document.querySelector('.item[data-ruta=\"carpeta-03/nota-041.html\"]').click(); JSON.stringify(1)");
    esperar(900);
    let (activa_c, cargadas_c) = ejecutar(proxy, |c| {
        let e = c.est.borrow();
        let activa = e.paneles[0].activa.and_then(|id| e.paneles[0].pestanas.iter().find(|t| t.id == id).map(|t| t.ruta.clone()));
        (activa, e.cargadas.iter().filter(|(l, r)| *l == 0 && r == "carpeta-03/nota-041.html").count())
    });
    let ok_c = activa_c.as_deref() == Some("carpeta-03/nota-041.html") && cargadas_c >= 1;
    captura("c-abrir-arbol.png");
    fila(
        &mut filas,
        "c) abrir archivo desde el árbol",
        veredicto(ok_c),
        json!({ "pestana_activa_panel0": activa_c, "carga_confirmada_por_la_pagina": cargadas_c >= 1 }),
    );

    // (d) Recolocar al cambiar el tamaño de la ventana.
    // set_inner_size no cambió el tamaño en sway; se redimensiona desde el compositor.
    sway(&["[title=\"U1 marco HTML\"] resize set 1000 650"]);
    esperar(1500);
    let marco_rect = js(proxy, Obj::Marco, "JSON.stringify((()=>{const b=document.querySelector('.contenido[data-lado=\"0\"]').getBoundingClientRect();return [b.left,b.top,b.width,b.height];})())");
    // Se lee la reserva de GTK (x, y, ancho, alto), no bounds(): wry no lee la posición.
    let pagina_rect = ejecutar(proxy, |c| {
        let a = c.vis.borrow().paginas[0].webview().allocation();
        [a.x() as f64, a.y() as f64, a.width() as f64, a.height() as f64]
    });
    let tam = ejecutar(proxy, |c| c.ventana.inner_size().to_logical::<f64>(c.ventana.scale_factor()).width);
    captura("d-recolocada.png");
    let coincide = marco_rect
        .as_array()
        .map(|a| a.iter().enumerate().all(|(i, v)| (v.as_f64().unwrap_or(-99.0) - pagina_rect[i]).abs() < 1.5))
        .unwrap_or(false);
    fila(
        &mut filas,
        "d) recolocación al redimensionar",
        veredicto(coincide),
        json!({ "hueco_marco_x_y_w_h": marco_rect, "webview_pagina_x_y_w_h": pagina_rect, "ancho_ventana_logico": tam }),
    );
    sway(&["[title=\"U1 marco HTML\"] resize set 1280 800"]);
    esperar(1200);

    // (e) Página hostil: intenta órdenes del marco, rutas fuera y navegación.
    js(proxy, Obj::Marco, "document.querySelector('.panel[data-lado=\"1\"] .nueva').click(); JSON.stringify(1)");
    esperar(800);
    let antes_json = ejecutar(proxy, |c| c.est.borrow().json_marco());
    let rech_antes = ejecutar(proxy, |c| c.est.borrow().rechazos.len());
    ejecutar(proxy, |c| {
        let p = c.est.borrow().puerto;
        c.est.borrow_mut().mostrando[1] = String::new();
        let _ = c.vis.borrow().paginas[1].load_url(&format!("http://127.0.0.1:{p}/hostil.html"));
    });
    esperar(2000);
    let despues_json = ejecutar(proxy, |c| c.est.borrow().json_marco());
    let rech_despues = ejecutar(proxy, |c| c.est.borrow().rechazos.len());
    let url_hostil = ejecutar(proxy, |c| c.vis.borrow().paginas[1].url().unwrap_or_default());
    let ok_e = antes_json == despues_json && url_hostil.ends_with("/hostil.html") && rech_despues - rech_antes >= 8;
    fila(
        &mut filas,
        "e) página hostil: órdenes y rutas rechazadas",
        veredicto(ok_e),
        json!({
            "estado_del_marco_intacto": antes_json == despues_json,
            "rechazos_nuevos": rech_despues - rech_antes,
            "url_del_webview_tras_ataque": url_hostil,
            "primeros_rechazos": ejecutar(proxy, |c| c.est.borrow().rechazos.iter().rev().take(3).cloned().collect::<Vec<_>>()),
        }),
    );
    ejecutar(proxy, |c| {
        c.est.borrow_mut().mostrando[1] = String::new();
        sincronizar(&c.est, &c.vis);
    });
    esperar(1000);

    // (f) Arrastrar un archivo del árbol a la página del panel 1.
    let origen = js(proxy, Obj::Marco, "JSON.stringify((()=>{const e=document.querySelector('.item[data-ruta=\"carpeta-08/nota-141.html\"]');e.scrollIntoView({block:\'center\'});const b=e.getBoundingClientRect();return [b.left+b.width/2,b.top+b.height/2];})())");
    let destino = js(proxy, Obj::Marco, "JSON.stringify((()=>{const b=document.querySelector('.contenido[data-lado=\"1\"]').getBoundingClientRect();return [b.left+b.width/2,b.top+b.height/2];})())");
    let (o, d) = (origen.as_array().cloned().unwrap_or_default(), destino.as_array().cloned().unwrap_or_default());
    let n_antes = cuantas(proxy, 1);
    js(proxy, Obj::Marco, "window.__ev = []; JSON.stringify(1)");
    if o.len() == 2 {
        clic(o[0].as_f64().unwrap_or(0.0), o[1].as_f64().unwrap_or(0.0));
        esperar(300);
    }
    let ev_clic = js(proxy, Obj::Marco, "JSON.stringify(window.__ev)");
    js(proxy, Obj::Marco, "window.__ev = []; JSON.stringify(1)");
    let salida_arrastre = if o.len() == 2 && d.len() == 2 {
        arrastrar(o[0].as_f64().unwrap(), o[1].as_f64().unwrap(), d[0].as_f64().unwrap(), d[1].as_f64().unwrap())
    } else {
        String::from("sin coordenadas")
    };
    esperar(1200);
    let soltados = ejecutar(proxy, |c| c.est.borrow().soltados.clone());
    let eventos_marco = js(proxy, Obj::Marco, "JSON.stringify(window.__ev)");
    let n_despues = cuantas(proxy, 1);
    captura("f-arrastre.png");
    let html5 = soltados.iter().any(|s| s.starts_with("html5:panel1:carpeta-08/nota-141.html"));
    let coords = soltados.iter().any(|s| s.starts_with("coordenadas:panel1:carpeta-08/nota-141.html"));
    let resultado_f = if html5 {
        "funciona"
    } else if coords {
        "funciona con límites"
    } else {
        "no funciona"
    };
    fila(
        &mut filas,
        "f) arrastre del árbol a una página",
        resultado_f,
        json!({
            "eventos_del_marco_tras_clic_de_prueba": ev_clic,
            "eventos_del_marco_durante_arrastre": eventos_marco,
            "origen_arrastre_x_y": o,
            "destino_arrastre_x_y": d,
            "via_html5_drop_en_pagina": html5,
            "via_coordenadas_del_marco": coords,
            "pestanas_panel1_antes": n_antes,
            "pestanas_panel1_despues": n_despues,
            "eventos_registrados": soltados,
            "salida_sway_arrastre": salida_arrastre.trim(),
        }),
    );

    // (g) Ctrl+W con foco en una página y con foco en el marco.
    let pestanas_1 = cuantas(proxy, 1);
    let centro1 = js(proxy, Obj::Marco, "JSON.stringify((()=>{const b=document.querySelector('.contenido[data-lado=\"1\"]').getBoundingClientRect();return [b.left+b.width/2,b.top+b.height/2];})())");
    if let Some(c) = centro1.as_array() {
        clic(c[0].as_f64().unwrap_or(0.0), c[1].as_f64().unwrap_or(0.0));
    }
    esperar(400);
    sway(&["[title=\"U1 marco HTML\"] focus"]);
    let _ = Command::new("wtype").args(["-M", "ctrl", "w", "-m", "ctrl"]).status();
    esperar(900);
    let pestanas_1_tras = cuantas(proxy, 1);
    let ok_g1 = pestanas_1 >= 1 && pestanas_1_tras + 1 == pestanas_1;
    fila(
        &mut filas,
        "g1) Ctrl+W con foco en la página (panel 1)",
        if pestanas_1 == 0 { "no probado (el panel 1 no tenía pestañas)" } else { veredicto(ok_g1) },
        json!({ "pestanas_antes": pestanas_1, "pestanas_despues": pestanas_1_tras }),
    );

    // Sin clic de ratón: el marco no recibió ningún evento de ratón en esta prueba (ver f).
    // Se da el foco de teclado al marco por código y se pulsa Ctrl+W con la ventana enfocada.
    let total_antes = cuantas(proxy, 0) + cuantas(proxy, 1);
    ejecutar(proxy, |c| {
        if let Some(m) = &c.vis.borrow().marco {
            m.webview().grab_focus();
        }
    });
    esperar(300);
    sway(&["[title=\"U1 marco HTML\"] focus"]);
    let salida_wtype = Command::new("wtype").args(["-M", "ctrl", "w", "-m", "ctrl"]).status();
    esperar(900);
    let total_tras = cuantas(proxy, 0) + cuantas(proxy, 1);
    let ok_g2 = total_antes >= 1 && total_tras + 1 == total_antes;
    fila(
        &mut filas,
        "g2) Ctrl+W con foco en el marco",
        veredicto(ok_g2),
        json!({ "pestanas_en_paneles_antes": total_antes, "pestanas_en_paneles_despues": total_tras, "wtype_ok": salida_wtype.map(|s| s.success()).unwrap_or(false) }),
    );

    json!({ "pruebas": filas, "rechazos_totales": ejecutar(proxy, |c| c.est.borrow().rechazos.len()) })
}
