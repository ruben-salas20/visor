//! Bucle winit, modelo de pestañas (copiado de spikes/u1-marco-html) y las cuatro pruebas.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use wry::{
    Rect, WebContext, WebView, WebViewBuilder, WebViewExtWindows,
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
};

use crate::{measure, nav, server};

const LIMITE: Duration = Duration::from_secs(60);
const TAMANOS: [(f64, f64); 3] = [(1280.0, 800.0), (1000.0, 650.0), (700.0, 450.0)];
const NOTAS: [&str; 5] = [
    "carpeta-01/nota-001.html",
    "carpeta-02/nota-021.html",
    "carpeta-03/nota-041.html",
    "carpeta-04/nota-061.html",
    "carpeta-05/nota-081.html",
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Prueba {
    A,
    B,
    C,
    D,
}

#[derive(Clone, Copy)]
enum Origen {
    Marco,
    Pagina(usize),
}

// ---------------------------------------------------------------------------
// Modelo de pestañas: copiado de spikes/u1-marco-html/src/main.rs.
// ---------------------------------------------------------------------------

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

    fn rechazar(&mut self, origen: &str, motivo: &str, cuerpo: &str) {
        let linea = format!("{origen}: {motivo}: {cuerpo}");
        eprintln!("RECHAZADO {linea}");
        self.rechazos.push(linea);
    }
}

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

#[derive(PartialEq)]
enum Efecto {
    Nada,
    Listo,
    Recolocar,
    Sincronizar,
}

/// Mensajes del marco. Solo el webview del marco llega aquí.
fn mensaje_marco(e: &mut Estado, cuerpo: &str) -> Efecto {
    let Ok(m) = serde_json::from_str::<Value>(cuerpo) else {
        e.rechazar("marco", "no es JSON", cuerpo);
        return Efecto::Nada;
    };
    let accion = m["accion"].as_str().unwrap_or("").to_string();
    let resultado: Result<Efecto, String> = (|| match accion.as_str() {
        "listo" => {
            e.listo = true;
            Ok(Efecto::Listo)
        }
        "recolocar" => {
            let rectas = m["rectas"].as_array().ok_or("faltan rectas")?;
            for (i, r) in rectas.iter().enumerate().take(2) {
                let v = [r["x"].as_f64(), r["y"].as_f64(), r["w"].as_f64(), r["h"].as_f64()];
                if let [Some(x), Some(y), Some(w), Some(h)] = v {
                    e.rectas[i] = Some([x, y, w, h]);
                }
            }
            Ok(Efecto::Recolocar)
        }
        "abrir" | "nueva" => {
            let lado = lado_valido(m["lado"].as_u64())?;
            abrir_en(e, lado, texto(&m, "ruta")?).map(|_| Efecto::Sincronizar)
        }
        "activar" => {
            let lado = lado_valido(m["lado"].as_u64())?;
            let id = m["id"].as_u64().ok_or("falta id")? as u32;
            buscar(e, lado, id)?;
            e.paneles[lado].activa = Some(id);
            Ok(Efecto::Sincronizar)
        }
        "cerrar" => {
            let lado = lado_valido(m["lado"].as_u64())?;
            let id = m["id"].as_u64().ok_or("falta id")? as u32;
            cerrar(e, lado, id).map(|_| Efecto::Sincronizar)
        }
        "cerrar_activa" => {
            let lado = lado_valido(m["lado"].as_u64())?;
            match e.paneles[lado].activa {
                Some(id) => cerrar(e, lado, id).map(|_| Efecto::Sincronizar),
                None => Ok(Efecto::Nada),
            }
        }
        "soltar" => {
            let x = m["x"].as_f64().ok_or("falta x")?;
            let y = m["y"].as_f64().ok_or("falta y")?;
            let lado = (0..2)
                .find(|&i| match e.rectas[i] {
                    Some([rx, ry, rw, rh]) => x >= rx && x <= rx + rw && y >= ry && y <= ry + rh,
                    None => false,
                })
                .ok_or("el punto no cae en ningún panel")?;
            let ruta = texto(&m, "ruta")?.to_string();
            abrir_en(e, lado, &ruta)?;
            e.soltados.push(format!("coordenadas:panel{lado}:{ruta}"));
            Ok(Efecto::Sincronizar)
        }
        _ => Err(format!("acción desconocida: {accion}")),
    })();
    match resultado {
        Ok(efecto) => efecto,
        Err(motivo) => {
            e.rechazar("marco", &motivo, cuerpo);
            Efecto::Nada
        }
    }
}

/// Mensajes de una página. El lado lo fija el webview que envía, no el mensaje.
fn mensaje_pagina(e: &mut Estado, lado: usize, cuerpo: &str) -> Efecto {
    let origen = format!("página {lado}");
    let Ok(m) = serde_json::from_str::<Value>(cuerpo) else {
        e.rechazar(&origen, "no es JSON", cuerpo);
        return Efecto::Nada;
    };
    let accion = m["accion"].as_str().unwrap_or("").to_string();
    let resultado: Result<Efecto, String> = (|| match accion.as_str() {
        "cargada" => {
            let ruta = texto(&m, "ruta")?.to_string();
            if e.arbol.contains(&ruta) {
                e.cargadas.push((lado, ruta));
            }
            Ok(Efecto::Nada)
        }
        // Extra pages of test c have no panel: only `cargada` is accepted from them.
        "cerrar_pestana" if lado < 2 => match e.paneles[lado].activa {
            Some(id) => cerrar(e, lado, id).map(|_| Efecto::Sincronizar),
            None => Ok(Efecto::Nada),
        },
        "soltar_en_pagina" if lado < 2 => {
            let ruta = texto(&m, "ruta")?.to_string();
            abrir_en(e, lado, &ruta)?;
            e.soltados.push(format!("html5:panel{lado}:{ruta}"));
            Ok(Efecto::Sincronizar)
        }
        _ => Err(format!("acción no permitida desde una página: {accion}")),
    })();
    match resultado {
        Ok(efecto) => efecto,
        Err(motivo) => {
            e.rechazar(&origen, &motivo, cuerpo);
            Efecto::Nada
        }
    }
}

/// Genera 200 notas en 10 carpetas (como spikes/u1-marco-html, que no las guarda en git).
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

fn caja(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

// ---------------------------------------------------------------------------
// La aplicación y las pruebas.
// ---------------------------------------------------------------------------

struct App {
    prueba: Prueba,
    salida: PathBuf,
    t0: Instant,
    latido: Arc<AtomicU64>,
    contexto: WebContext,
    window: Option<Window>,
    marco: Option<WebView>,
    paginas: Vec<WebView>,
    navs: Vec<Rc<RefCell<nav::Nav>>>,
    cola: Rc<RefCell<VecDeque<(Origen, String)>>>,
    bloqueadas: Rc<RefCell<Vec<String>>>,
    ventanas_denegadas: Rc<RefCell<Vec<String>>>,
    respuestas: Arc<Mutex<Vec<String>>>,
    est: Estado,
    recolocar_n: u32,
    ultimo_recolocar: Option<Instant>,
    tiempos: Map<String, Value>,
    paso: u32,
    paso_t: Instant,
    base_recolocar: u32,
    instantanea: Option<String>,
    evidencia: Map<String, Value>,
    pasos_d: Vec<Value>,
}

impl App {
    fn marcar(&mut self, evento: &str) {
        if !self.tiempos.contains_key(evento) {
            self.tiempos.insert(evento.into(), json!(ms(self.t0.elapsed())));
        }
    }

    fn avanzar(&mut self) {
        self.paso += 1;
        self.paso_t = Instant::now();
    }

    fn crear_pagina(&mut self, lado: usize, bounds: Rect) -> Result<(), String> {
        let puerto = self.est.puerto;
        let prefijo_boveda = format!("http://127.0.0.1:{puerto}/boveda/");
        let prefijo_hostil = format!("http://127.0.0.1:{puerto}/hostil.html");
        let cola = self.cola.clone();
        let bloqueadas = self.bloqueadas.clone();
        let denegadas = self.ventanas_denegadas.clone();
        let window = self.window.as_ref().ok_or("sin ventana")?;
        let pagina = WebViewBuilder::new_with_web_context(&mut self.contexto)
            .with_url("about:blank")
            .with_bounds(bounds)
            .with_navigation_handler(move |url: String| {
                let ok = url == "about:blank" || url.starts_with(&prefijo_boveda) || url.starts_with(&prefijo_hostil);
                if !ok {
                    eprintln!("NAVEGACIÓN BLOQUEADA página {lado}: {url}");
                    bloqueadas.borrow_mut().push(format!("página {lado}: {url}"));
                }
                ok
            })
            .with_new_window_req_handler(move |url, _features| {
                eprintln!("VENTANA NUEVA DENEGADA página {lado}: {url}");
                denegadas.borrow_mut().push(format!("página {lado}: {url}"));
                wry::NewWindowResponse::Deny
            })
            .with_ipc_handler(move |req| cola.borrow_mut().push_back((Origen::Pagina(lado), req.body().clone())))
            .build_as_child(window)
            .map_err(|e| format!("no se pudo crear la página {lado}: {e}"))?;
        let estado_nav = Rc::new(RefCell::new(nav::Nav::default()));
        nav::register(&pagina.webview(), &estado_nav).map_err(|e| format!("eventos COM de la página {lado}: {e}"))?;
        self.paginas.push(pagina);
        self.navs.push(estado_nav);
        Ok(())
    }

    fn crear_todo(&mut self) -> Result<(), String> {
        let puerto = self.est.puerto;
        let window = self.window.as_ref().ok_or("sin ventana")?;
        let tam = window.inner_size();
        let prefijo_marco = format!("http://127.0.0.1:{puerto}/react/");
        let cola = self.cola.clone();
        let marco = WebViewBuilder::new_with_web_context(&mut self.contexto)
            .with_url(format!("{prefijo_marco}index.html"))
            .with_bounds(Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(tam.width, tam.height).into(),
            })
            .with_navigation_handler(move |url: String| url.starts_with(&prefijo_marco))
            .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |req| cola.borrow_mut().push_back((Origen::Marco, req.body().clone())))
            .build_as_child(window)
            .map_err(|e| format!("no se pudo crear el marco: {e}"))?;
        self.marco = Some(marco);
        self.marcar("marco_creado");
        for lado in 0..2 {
            self.crear_pagina(lado, caja(0.0, 0.0, 1.0, 1.0))?;
        }
        self.marcar("paginas_creadas");
        Ok(())
    }

    fn sincronizar(&mut self) {
        for lado in 0..2 {
            let destino = self.est.destino(lado);
            if self.est.mostrando[lado] != destino {
                self.est.mostrando[lado] = destino.clone();
                if let Some(p) = self.paginas.get(lado) {
                    let _ = p.load_url(&destino);
                }
            }
        }
        if self.est.listo {
            if let Some(m) = &self.marco {
                let _ = m.evaluate_script(&format!("window.marco.pintar({})", self.est.json_marco()));
            }
        }
    }

    fn aplicar_rectas(&mut self) {
        for (lado, r) in self.est.rectas.iter().enumerate() {
            if let (Some([x, y, w, h]), Some(p)) = (r, self.paginas.get(lado)) {
                let _ = p.set_bounds(caja(*x, *y, *w, *h));
            }
        }
    }

    fn al_listo(&mut self) {
        self.marcar("listo");
        let _ = abrir_en(&mut self.est, 0, NOTAS[0]);
        if self.prueba == Prueba::B {
            // The hostile page is not a vault note, so it does not go through the tab model.
            let url = format!("http://127.0.0.1:{}/hostil.html", self.est.puerto);
            if let Some(p) = self.paginas.get(1) {
                let _ = p.load_url(&url);
            }
        } else {
            let _ = abrir_en(&mut self.est, 1, NOTAS[1]);
        }
        self.sincronizar();
        if self.prueba == Prueba::B {
            self.instantanea = Some(self.est.json_marco().to_string());
        }
    }

    fn procesar_cola(&mut self) {
        let mensajes: Vec<_> = self.cola.borrow_mut().drain(..).collect();
        for (origen, cuerpo) in mensajes {
            let efecto = match origen {
                Origen::Marco => mensaje_marco(&mut self.est, &cuerpo),
                Origen::Pagina(lado) => {
                    let antes = self.est.cargadas.len();
                    let efecto = mensaje_pagina(&mut self.est, lado, &cuerpo);
                    if self.est.cargadas.len() > antes {
                        self.marcar(&format!("cargada_{lado}"));
                    }
                    efecto
                }
            };
            match efecto {
                Efecto::Nada => {}
                Efecto::Listo => self.al_listo(),
                Efecto::Recolocar => {
                    self.recolocar_n += 1;
                    self.ultimo_recolocar = Some(Instant::now());
                    self.marcar("recolocar");
                    self.aplicar_rectas();
                }
                Efecto::Sincronizar => self.sincronizar(),
            }
        }
    }

    fn cargada(&self, lado: usize) -> bool {
        self.est.cargadas.iter().any(|(l, _)| *l == lado)
    }

    fn cargado(&self, lados: &[usize]) -> bool {
        self.est.listo && self.recolocar_n > 0 && lados.iter().all(|&l| self.cargada(l))
    }

    fn sondear(&self) {
        self.respuestas.lock().unwrap().clear();
        let vistas = self.marco.iter().chain(self.paginas.iter().take(2));
        for (i, v) in vistas.enumerate() {
            let r = self.respuestas.clone();
            let _ = v.evaluate_script_with_callback("JSON.stringify(document.readyState)", move |s| {
                r.lock().unwrap().push(format!("vista {i}: {s}"));
            });
        }
    }

    fn fin(&mut self, ok: bool, motivo: &str, codigo: i32) -> ! {
        let committed: Vec<Value> = self
            .navs
            .iter()
            .map(|n| {
                let n = n.borrow();
                json!({"confirmada": n.committed, "historial": n.history, "source_al_cargar": n.source_al_cargar})
            })
            .collect();
        let mut informe = Map::new();
        informe.insert("prueba".into(), json!(format!("{:?}", self.prueba).to_lowercase()));
        informe.insert("ok".into(), json!(ok));
        informe.insert("motivo".into(), json!(motivo));
        informe.insert("ms_total".into(), json!(ms(self.t0.elapsed())));
        informe.insert("tiempos_ms".into(), Value::Object(self.tiempos.clone()));
        informe.insert("evidencia".into(), Value::Object(self.evidencia.clone()));
        informe.insert("rechazos".into(), json!(self.est.rechazos));
        informe.insert("navegaciones_bloqueadas".into(), json!(*self.bloqueadas.borrow()));
        informe.insert("ventanas_denegadas".into(), json!(*self.ventanas_denegadas.borrow()));
        informe.insert("url_confirmada_por_vista".into(), json!(committed));
        informe.insert("recolocar_recibidos".into(), json!(self.recolocar_n));
        let texto = serde_json::to_string_pretty(&Value::Object(informe)).unwrap_or_default();
        let _ = fs::write(&self.salida, &texto);
        println!("{texto}");
        std::process::exit(codigo);
    }

    fn tick_a(&mut self) {
        match self.paso {
            0 if self.cargado(&[0, 1]) => {
                self.sondear();
                self.avanzar();
            }
            1 if self.respuestas.lock().unwrap().len() == 3 => {
                self.evidencia.insert("sondeo_1".into(), json!(*self.respuestas.lock().unwrap()));
                self.avanzar();
            }
            1 if self.paso_t.elapsed() > Duration::from_secs(5) => {
                self.fin(false, "evaluate_script sin respuesta tras cargar", 1)
            }
            2 if self.paso_t.elapsed() > Duration::from_secs(3) => {
                self.sondear();
                self.avanzar();
            }
            3 if self.respuestas.lock().unwrap().len() == 3 => {
                self.evidencia.insert("sondeo_2".into(), json!(*self.respuestas.lock().unwrap()));
                self.fin(true, "marco y dos páginas cargados; la ventana responde 3 s después", 0)
            }
            3 if self.paso_t.elapsed() > Duration::from_secs(5) => {
                self.fin(false, "evaluate_script sin respuesta 3 s después", 1)
            }
            _ => {}
        }
    }

    fn tick_b(&mut self) {
        match self.paso {
            0 => {
                let hostil = self.navs.get(1).is_some_and(|n| {
                    n.borrow().history.iter().any(|u| u.ends_with("/hostil.html"))
                });
                if self.cargado(&[0]) && hostil {
                    self.marcar("hostil_confirmada");
                    self.avanzar();
                }
            }
            // hostil.html fires its navigations at 200, 400 and 600 ms: 4 s is plenty.
            1 if self.paso_t.elapsed() > Duration::from_secs(4) => {
                let de_hostil: Vec<String> =
                    self.est.rechazos.iter().filter(|r| r.starts_with("página 1:")).cloned().collect();
                let ahora = self.est.json_marco().to_string();
                let sin_cambios = self.instantanea.as_deref() == Some(ahora.as_str());
                let confirmada = self.navs[1].borrow().committed.clone();
                let sigue = confirmada.as_deref().is_some_and(|u| u.ends_with("/hostil.html"));
                let recta_intacta = !matches!(self.est.rectas[0], Some([_, _, w, _]) if w >= 9999.0);
                self.evidencia.insert("rechazos_de_hostil".into(), json!(de_hostil.len()));
                self.evidencia.insert("estado_del_marco_sin_cambios".into(), json!(sin_cambios));
                self.evidencia.insert("recta_no_tomada_de_hostil".into(), json!(recta_intacta));
                self.evidencia.insert("url_confirmada_pagina_1".into(), json!(confirmada));
                self.evidencia.insert("eventos_com_pagina_1".into(), json!(self.navs[1].borrow().events));
                let ok = de_hostil.len() == 8 && sin_cambios && sigue && recta_intacta;
                self.fin(ok, if ok { "8 rechazos, estado intacto, URL confirmada en hostil.html" } else { "ver evidencia" }, if ok { 0 } else { 1 })
            }
            _ => {}
        }
    }

    fn medida(&self) -> Value {
        match measure::medir(std::process::id()) {
            Ok(m) => {
                let todos: Vec<&measure::Proceso> = std::iter::once(&m.host).chain(m.hijos.iter()).collect();
                let pb: u64 = todos.iter().filter_map(|p| p.private_bytes).sum();
                let ws: u64 = todos.iter().filter_map(|p| p.working_set).sum();
                let detalle: Vec<Value> = todos
                    .iter()
                    .map(|p| json!({"pid": p.pid, "padre": p.padre, "nombre": p.nombre, "ws": p.working_set, "pb": p.private_bytes}))
                    .collect();
                json!({"procesos": todos.len(), "private_bytes": pb, "working_set": ws, "detalle": detalle})
            }
            Err(e) => json!({"error": e}),
        }
    }

    fn tick_c(&mut self) {
        match self.paso {
            0 if self.cargado(&[0, 1]) => self.avanzar(),
            1 if self.paso_t.elapsed() > Duration::from_secs(5) => {
                let m = self.medida();
                self.evidencia.insert("marco_mas_2".into(), m);
                for i in 0..3 {
                    let lado = 2 + i;
                    if let Err(e) = self.crear_pagina(lado, caja(10.0 + 210.0 * i as f64, 300.0, 200.0, 150.0)) {
                        self.fin(false, &e, 1);
                    }
                    let url = self.est.url_de(NOTAS[lado]);
                    let _ = self.paginas[lado].load_url(&url);
                }
                self.avanzar();
            }
            2 if self.cargado(&[0, 1, 2, 3, 4]) => self.avanzar(),
            3 if self.paso_t.elapsed() > Duration::from_secs(5) => {
                let m = self.medida();
                let antes = &self.evidencia["marco_mas_2"];
                let mb = |campo: &str| -> Option<f64> {
                    let a = antes[campo].as_u64()? as f64;
                    let b = m[campo].as_u64()? as f64;
                    Some((b - a) / 3.0 / 1_048_576.0)
                };
                let (pb, ws) = (mb("private_bytes"), mb("working_set"));
                let resumen = json!({
                    "mb_private_bytes_por_pagina_extra": pb,
                    "mb_working_set_por_pagina_extra": ws,
                    "procesos_2": antes["procesos"], "procesos_5": m["procesos"],
                    "mb_private_bytes_2": antes["private_bytes"].as_u64().map(|v| v as f64 / 1_048_576.0),
                    "mb_private_bytes_5": m["private_bytes"].as_u64().map(|v| v as f64 / 1_048_576.0),
                    "mb_working_set_2": antes["working_set"].as_u64().map(|v| v as f64 / 1_048_576.0),
                    "mb_working_set_5": m["working_set"].as_u64().map(|v| v as f64 / 1_048_576.0),
                });
                self.evidencia.insert("marco_mas_5".into(), m);
                self.evidencia.insert("resumen".into(), resumen);
                let ok = pb.is_some() && ws.is_some();
                self.fin(ok, if ok { "medido" } else { "no se pudo leer la memoria" }, if ok { 0 } else { 1 })
            }
            _ => {}
        }
    }

    fn comprobar_d(&mut self, pedido: (f64, f64), recolocar_nuevo: bool) -> bool {
        let window = self.window.as_ref().unwrap();
        let escala = window.scale_factor();
        let interior = window.inner_size();
        let cerca = |a: f64, b: f64| (a - b).abs() <= 2.0;
        let mut lados = Vec::new();
        let mut todo = recolocar_nuevo;
        for lado in 0..2 {
            let r = self.est.rectas[lado];
            let b = self.paginas[lado].bounds();
            let (Some([x, y, w, h]), Ok(b)) = (r, b) else {
                lados.push(json!({"lado": lado, "error": "sin recta o sin bounds"}));
                todo = false;
                continue;
            };
            let pos = b.position.to_physical::<f64>(escala);
            let tam = b.size.to_physical::<f64>(escala);
            let esperado = [x * escala, y * escala, w.max(1.0) * escala, h.max(1.0) * escala];
            let real = [pos.x, pos.y, tam.width, tam.height];
            let coincide = esperado.iter().zip(real.iter()).all(|(a, b)| cerca(*a, *b));
            let dentro = (x + w) * escala <= interior.width as f64 + 2.0 && (y + h) * escala <= interior.height as f64 + 2.0;
            todo &= coincide;
            lados.push(json!({"lado": lado, "recta_logica": [x, y, w, h], "esperado_fisico": esperado, "bounds_fisico": real, "coincide": coincide, "dentro_de_la_ventana": dentro}));
        }
        let marco = self.marco.as_ref().and_then(|m| m.bounds().ok()).map(|b| {
            let t = b.size.to_physical::<f64>(escala);
            [t.width, t.height]
        });
        let marco_ok = marco.is_some_and(|[w, h]| cerca(w, interior.width as f64) && cerca(h, interior.height as f64));
        todo &= marco_ok;
        let tamano_ok = cerca(interior.width as f64, pedido.0 * escala) && cerca(interior.height as f64, pedido.1 * escala);
        self.pasos_d.push(json!({
            "pedido_logico": [pedido.0, pedido.1], "interior_fisico": [interior.width, interior.height],
            "escala": escala, "ventana_tomo_el_tamano": tamano_ok, "recolocar_nuevo": recolocar_nuevo,
            "marco_fisico": marco, "marco_ocupa_la_ventana": marco_ok, "paginas": lados, "ok": todo,
        }));
        todo
    }

    fn tick_d(&mut self) {
        let k = (self.paso.saturating_sub(1) / 2) as usize;
        match self.paso {
            0 if self.cargado(&[0, 1]) && self.ultimo_recolocar.is_some_and(|t| t.elapsed() > Duration::from_secs(1)) => {
                self.avanzar()
            }
            p if p >= 1 && p % 2 == 1 => {
                let (w, h) = TAMANOS[k];
                self.base_recolocar = self.recolocar_n;
                let devuelto = self.window.as_ref().unwrap().request_inner_size(LogicalSize::new(w, h));
                eprintln!("[{:>6} ms] pedido {w}x{h}, devuelto {devuelto:?}", ms(self.t0.elapsed()));
                self.avanzar();
            }
            p if p >= 2 && p % 2 == 0 => {
                let nuevo = self.recolocar_n > self.base_recolocar;
                let quieto = self.ultimo_recolocar.is_some_and(|t| t.elapsed() > Duration::from_millis(500));
                let listo = nuevo && quieto && self.paso_t.elapsed() > Duration::from_millis(700);
                if listo || self.paso_t.elapsed() > Duration::from_secs(8) {
                    self.comprobar_d(TAMANOS[k], nuevo);
                    if k + 1 < TAMANOS.len() {
                        self.avanzar();
                    } else {
                        let ok = self.pasos_d.iter().all(|p| p["ok"] == json!(true));
                        self.evidencia.insert("pasos".into(), json!(self.pasos_d));
                        self.fin(ok, if ok { "los bounds siguen a las rectas en los tres tamaños" } else { "ver pasos" }, if ok { 0 } else { 1 })
                    }
                }
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("U1 marco HTML (Windows)")
            .with_inner_size(LogicalSize::new(1000.0, 650.0));
        match event_loop.create_window(attrs) {
            Ok(w) => self.window = Some(w),
            Err(e) => self.fin(false, &format!("no se pudo crear la ventana: {e}"), 1),
        }
        self.marcar("ventana");
        if let Err(e) = self.crear_todo() {
            self.fin(false, &e, 1);
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => {
                if let Some(m) = &self.marco {
                    let _ = m.set_bounds(Rect {
                        position: PhysicalPosition::new(0, 0).into(),
                        size: PhysicalSize::new(size.width, size.height).into(),
                    });
                }
            }
            WindowEvent::CloseRequested => std::process::exit(0),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50)));
        self.latido.store(ms(self.t0.elapsed()).max(1), Ordering::Relaxed);
        self.procesar_cola();
        if self.t0.elapsed() > LIMITE {
            let motivo = format!("tiempo agotado en el paso {}", self.paso);
            if !self.pasos_d.is_empty() {
                self.evidencia.insert("pasos".into(), json!(self.pasos_d));
            }
            self.fin(false, &motivo, 3);
        }
        match self.prueba {
            Prueba::A => self.tick_a(),
            Prueba::B => self.tick_b(),
            Prueba::C => self.tick_c(),
            Prueba::D => self.tick_d(),
        }
    }
}

fn argumento(args: &[String], nombre: &str) -> Option<String> {
    args.iter().position(|a| a == nombre).and_then(|i| args.get(i + 1)).cloned()
}

pub fn main() {
    let t0 = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let (Some(salida), Some(prueba)) = (argumento(&args, "--auto"), argumento(&args, "--prueba")) else {
        eprintln!("uso: u1-windows --auto <salida.json> --prueba a|b|c|d");
        std::process::exit(2);
    };
    let prueba = match prueba.as_str() {
        "a" => Prueba::A,
        "b" => Prueba::B,
        "c" => Prueba::C,
        "d" => Prueba::D,
        otra => {
            eprintln!("--prueba acepta a, b, c o d, no {otra}");
            std::process::exit(2);
        }
    };
    let salida = PathBuf::from(salida);

    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../u1-marco-html/web");
    let arbol = match generar_boveda(&raiz) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("no se pudo generar la bóveda en {}: {e}", raiz.display());
            std::process::exit(1);
        }
    };
    let puerto = server::start(raiz).expect("servidor local");

    // Watchdog: if the beat stops for 5 s the main thread is blocked. Armed after the first beat,
    // because creating the first WebView2 can take longer than that.
    let latido = Arc::new(AtomicU64::new(0));
    let vigilado = latido.clone();
    let salida_v = salida.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let ahora = ms(t0.elapsed());
        let ultimo = vigilado.load(Ordering::Relaxed);
        if ultimo != 0 && ahora.saturating_sub(ultimo) > 5_000 {
            let motivo = format!("hilo principal bloqueado desde {ultimo} ms (ahora {ahora} ms)");
            eprintln!("{motivo}");
            let informe = json!({"prueba": format!("{prueba:?}").to_lowercase(), "ok": false, "motivo": motivo});
            let _ = fs::write(&salida_v, serde_json::to_string_pretty(&informe).unwrap_or_default());
            std::process::exit(4);
        }
    });

    let datos = std::env::temp_dir().join(format!("u1-windows-{}", std::process::id()));
    let mut app = App {
        prueba,
        salida,
        t0,
        latido,
        // One context for every view, as visor does.
        contexto: WebContext::new(Some(datos)),
        window: None,
        marco: None,
        paginas: Vec::new(),
        navs: Vec::new(),
        cola: Rc::new(RefCell::new(VecDeque::new())),
        bloqueadas: Rc::new(RefCell::new(Vec::new())),
        ventanas_denegadas: Rc::new(RefCell::new(Vec::new())),
        respuestas: Arc::new(Mutex::new(Vec::new())),
        est: Estado {
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
        },
        recolocar_n: 0,
        ultimo_recolocar: None,
        tiempos: Map::new(),
        paso: 0,
        paso_t: Instant::now(),
        base_recolocar: 0,
        instantanea: None,
        evidencia: Map::new(),
        pasos_d: Vec::new(),
    };
    let event_loop = EventLoop::new().expect("bucle de eventos");
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("el bucle terminó con error: {e}");
        std::process::exit(1);
    }
}
