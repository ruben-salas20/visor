//! Las siete pruebas. Cada una corre de principio a fin dentro de `resumed`, en estilo
//! bloqueante: `bombear` despacha los mensajes de Win32 mientras espera (como hace wry en sus
//! operaciones síncronas). winit guarda en cola los eventos que llegan mientras tanto.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler, CapturePreviewCompletedHandler,
    ContainsFullScreenElementChangedEventHandler, DevToolsProtocolEventReceivedEventHandler,
    ExecuteScriptCompletedHandler, FocusChangedEventHandler, FrameCreatedEventHandler,
    NavigationCompletedEventHandler, NavigationStartingEventHandler,
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, WebResourceResponseReceivedEventHandler,
    Microsoft::Web::WebView2::Win32::*,
    take_pwstr,
};
use windows::Win32::Foundation::{HGLOBAL, HWND, RECT};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetForegroundWindow, MSG, PM_REMOVE, PeekMessageW, SetCursorPos,
    SetForegroundWindow, TranslateMessage,
};
use windows_core::{BOOL, HSTRING, Interface, PWSTR};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Fullscreen, Window, WindowId},
};
use wry::{
    Rect, WebContext, WebView, WebViewBuilder, WebViewExtWindows,
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
};

use crate::server;

const LIMITE: Duration = Duration::from_secs(90);

type Evidencia = Arc<Mutex<Map<String, Value>>>;

/// Result of a test: criterion met, one-line verdict, and reason.
struct Veredicto {
    ok: bool,
    resultado: &'static str,
    motivo: String,
}

fn veredicto(ok: bool, resultado: &'static str, motivo: impl Into<String>) -> Result<Veredicto, String> {
    Ok(Veredicto { ok, resultado, motivo: motivo.into() })
}

trait Ctx<T> {
    fn e(self, que: &str) -> Result<T, String>;
}
impl<T, E: std::fmt::Display> Ctx<T> for Result<T, E> {
    fn e(self, que: &str) -> Result<T, String> {
        self.map_err(|e| format!("{que}: {e}"))
    }
}

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

fn caja(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { position: LogicalPosition::new(x, y).into(), size: LogicalSize::new(w, h).into() }
}

/// Dispatches Win32 messages for `ms` milliseconds.
fn bombear(ms: u64) {
    let fin = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < fin {
        bombear_una_vez();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn bombear_una_vez() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn esperar(ms: u64, mut cond: impl FnMut() -> bool) -> bool {
    let fin = Instant::now() + Duration::from_millis(ms);
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= fin {
            return false;
        }
        bombear_una_vez();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A child view with the COM objects and what its navigation events left.
struct Vista {
    wv: WebView,
    com: ICoreWebView2,
    ctl: ICoreWebView2Controller,
    /// Source() at each NavigationCompleted, with its success flag.
    completadas: Rc<RefCell<Vec<(String, bool)>>>,
    /// NavigationStarting: (uri, kind, cancelled).
    iniciadas: Rc<RefCell<Vec<(String, i32, bool)>>>,
    /// While true, NavigationStarting cancels back/forward navigations.
    interceptar: Rc<Cell<bool>>,
}

fn source_of(wv: &ICoreWebView2) -> String {
    let mut p = PWSTR::null();
    if unsafe { wv.Source(&mut p) }.is_ok() { take_pwstr(p) } else { String::new() }
}

impl Vista {
    fn nueva(window: &Window, contexto: &mut WebContext, bounds: Rect) -> Result<Vista, String> {
        let wv = WebViewBuilder::new_with_web_context(contexto)
            .with_url("about:blank")
            .with_bounds(bounds)
            .build_as_child(window)
            .e("crear webview")?;
        let com = wv.webview();
        let ctl = wv.controller();
        let completadas = Rc::new(RefCell::new(Vec::new()));
        let iniciadas = Rc::new(RefCell::new(Vec::new()));
        let interceptar = Rc::new(Cell::new(false));
        let c = completadas.clone();
        let fin = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
            let mut ok = BOOL(0);
            if let Some(args) = args {
                unsafe { args.IsSuccess(&mut ok)? };
            }
            let src = sender.as_ref().map(source_of).unwrap_or_default();
            c.borrow_mut().push((src, ok.as_bool()));
            Ok(())
        }));
        let i = iniciadas.clone();
        let inter = interceptar.clone();
        let inicio = NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            unsafe { args.Uri(&mut uri)? };
            let uri = take_pwstr(uri);
            let mut kind = COREWEBVIEW2_NAVIGATION_KIND(-1);
            if let Ok(a3) = args.cast::<ICoreWebView2NavigationStartingEventArgs3>() {
                let _ = unsafe { a3.NavigationKind(&mut kind) };
            }
            let cancelar = inter.get() && kind == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD;
            if cancelar {
                unsafe { args.SetCancel(true)? };
            }
            i.borrow_mut().push((uri, kind.0, cancelar));
            Ok(())
        }));
        let mut token = 0i64;
        unsafe {
            com.add_NavigationCompleted(&fin, &mut token).e("NavigationCompleted")?;
            com.add_NavigationStarting(&inicio, &mut token).e("NavigationStarting")?;
        }
        // Let the initial about:blank finish before anything else.
        esperar(5_000, || !completadas.borrow().is_empty());
        Ok(Vista { wv, com, ctl, completadas, iniciadas, interceptar })
    }

    fn cargar(&self, url: &str) -> Result<(), String> {
        let n = self.completadas.borrow().len();
        self.wv.load_url(url).e("load_url")?;
        let ok = esperar(15_000, || self.completadas.borrow()[n..].iter().any(|(s, _)| s == url));
        if ok { Ok(()) } else { Err(format!("{url} no terminó de cargar en 15 s")) }
    }

    fn eval(&self, js: &str) -> Option<Value> {
        let r: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let r2 = r.clone();
        self.wv
            .evaluate_script_with_callback(js, move |s| {
                *r2.lock().unwrap() = Some(s);
            })
            .ok()?;
        esperar(5_000, || r.lock().unwrap().is_some());
        let s = r.lock().unwrap().take()?;
        Some(serde_json::from_str(&s).unwrap_or(Value::String(s)))
    }

    /// Runs an expression that yields a promise and waits for it to settle.
    fn eval_promesa(&self, expr: &str) -> Value {
        let js = format!(
            "window.__p = null; try {{ Promise.resolve({expr}).then(v => window.__p = {{ok: String(v)}}, e => window.__p = {{error: String(e)}}); }} catch (e) {{ window.__p = {{error: 'síncrono: ' + e}}; }} 0"
        );
        self.eval(&js);
        let mut v = Value::Null;
        esperar(5_000, || {
            v = self.eval("window.__p").unwrap_or(Value::Null);
            !v.is_null()
        });
        if v.is_null() { json!({"error": "la promesa no se resolvió en 5 s"}) } else { v }
    }
}

fn cdp(com: &ICoreWebView2, metodo: &str, params: &Value) -> Result<Value, String> {
    let r: Rc<RefCell<Option<Result<String, String>>>> = Rc::new(RefCell::new(None));
    let r2 = r.clone();
    let h = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |hr, s| {
        *r2.borrow_mut() = Some(hr.map(|_| s).map_err(|e| e.to_string()));
        Ok(())
    }));
    unsafe {
        com.CallDevToolsProtocolMethod(&HSTRING::from(metodo), &HSTRING::from(params.to_string()), &h)
            .e(metodo)?;
    }
    if !esperar(5_000, || r.borrow().is_some()) {
        return Err(format!("{metodo}: sin respuesta en 5 s"));
    }
    let s = r.borrow_mut().take().unwrap()?;
    Ok(serde_json::from_str(&s).unwrap_or(Value::String(s)))
}

fn cdp_evento(com: &ICoreWebView2, nombre: &str, f: impl FnMut(Value) + 'static) -> Result<(), String> {
    let receptor = unsafe { com.GetDevToolsProtocolEventReceiver(&HSTRING::from(nombre)) }.e(nombre)?;
    let f = RefCell::new(f);
    let h = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut p = PWSTR::null();
        unsafe { args.ParameterObjectAsJson(&mut p)? };
        let v = serde_json::from_str(&take_pwstr(p)).unwrap_or(Value::Null);
        (f.borrow_mut())(v);
        Ok(())
    }));
    let mut token = 0i64;
    unsafe { receptor.add_DevToolsProtocolEventReceived(&h, &mut token) }.e(nombre)
}

fn ejecutar_en_marco(frame: &ICoreWebView2Frame, js: &str) -> Option<Value> {
    let f2: ICoreWebView2Frame2 = frame.cast().ok()?;
    let r: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let r2 = r.clone();
    let h = ExecuteScriptCompletedHandler::create(Box::new(move |hr, s| {
        *r2.borrow_mut() = Some(match hr {
            Ok(()) => s,
            Err(e) => json!({"error": e.to_string()}).to_string(),
        });
        Ok(())
    }));
    unsafe { f2.ExecuteScript(&HSTRING::from(js), &h) }.ok()?;
    esperar(5_000, || r.borrow().is_some());
    let s = r.borrow_mut().take()?;
    Some(serde_json::from_str(&s).unwrap_or(Value::String(s)))
}

struct App {
    prueba: String,
    salida: PathBuf,
    t0: Instant,
    evidencia: Evidencia,
    puertos: server::Puertos,
    contexto: WebContext,
    window: Option<Window>,
}

impl App {
    fn ev(&self, clave: &str, valor: Value) {
        eprintln!("{clave}: {valor}");
        self.evidencia.lock().unwrap().insert(clave.into(), valor);
    }

    fn url(&self, ruta: &str) -> String {
        format!("http://127.0.0.1:{}{ruta}", self.puertos.servidor)
    }

    fn vista(&mut self, bounds: Rect) -> Result<Vista, String> {
        let window = self.window.as_ref().ok_or("sin ventana")?;
        Vista::nueva(window, &mut self.contexto, bounds)
    }

    fn hwnd(&self) -> Option<HWND> {
        let h = self.window.as_ref()?.window_handle().ok()?;
        match h.as_raw() {
            RawWindowHandle::Win32(w) => Some(HWND(w.hwnd.get() as *mut _)),
            _ => None,
        }
    }

    fn fin(&self, v: Result<Veredicto, String>) -> ! {
        let (ok, resultado, motivo, codigo) = match v {
            Ok(v) => (v.ok, v.resultado, v.motivo, if v.ok { 0 } else { 1 }),
            Err(e) => (false, "error", e, 1),
        };
        escribir(&self.salida, &self.prueba, ok, resultado, &motivo, self.t0, &self.evidencia);
        std::process::exit(codigo);
    }

    // 1. ------------------------------------------------------------------------------------
    fn captura(&mut self) -> Result<Veredicto, String> {
        let v = self.vista(caja(0.0, 0.0, 800.0, 600.0))?;
        v.cargar(&self.url("/color.html"))?;
        bombear(800);
        let mut rect = RECT::default();
        unsafe { v.ctl.Bounds(&mut rect) }.e("Bounds")?;
        let (bw, bh) = ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32);
        self.ev("vista_fisica", json!([bw, bh]));

        let stream: IStream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }.e("CreateStreamOnHGlobal")?;
        let hecho: Rc<RefCell<Option<Result<(), String>>>> = Rc::new(RefCell::new(None));
        let h2 = hecho.clone();
        let handler = CapturePreviewCompletedHandler::create(Box::new(move |r| {
            *h2.borrow_mut() = Some(r.map_err(|e| e.to_string()));
            Ok(())
        }));
        let t = Instant::now();
        unsafe { v.com.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler) }
            .e("CapturePreview")?;
        if !esperar(10_000, || hecho.borrow().is_some()) {
            return veredicto(false, "no funciona", "CapturePreview no terminó en 10 s");
        }
        hecho.borrow_mut().take().unwrap().e("CapturePreview terminó con error")?;
        self.ev("ms_captura", json!(ms(t.elapsed())));

        let bytes = leer_stream(&stream)?;
        let png_ruta = self.salida.with_extension("png");
        fs::write(&png_ruta, &bytes).e("guardar PNG")?;
        self.ev("png_bytes", json!(bytes.len()));
        let dec = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut lector = dec.read_info().e("leer PNG")?;
        let mut buf = vec![0; lector.output_buffer_size()];
        let info = lector.next_frame(&mut buf).e("decodificar PNG")?;
        let canales = info.color_type.samples();
        let (w, h) = (info.width, info.height);
        let pixel = |x: u32, y: u32| {
            let i = ((y * w + x) as usize) * canales;
            [buf[i], buf[i + 1], buf[i + 2]]
        };
        let izq = pixel(w / 4, h / 2);
        let der = pixel(3 * w / 4, h / 2);
        self.ev("png", json!({"ancho": w, "alto": h, "tipo": format!("{:?}", info.color_type), "pixel_izquierda": izq, "pixel_derecha": der}));
        let cerca = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 3);
        let colores = cerca(izq, [10, 200, 90]) && cerca(der, [200, 30, 60]);
        let tamano = w == bw && h == bh;
        let ok = !bytes.is_empty() && colores && tamano;
        let motivo = format!("PNG {w}x{h} (vista {bw}x{bh}); izquierda {izq:?}, derecha {der:?}");
        veredicto(ok, if ok { "funciona" } else { "no funciona" }, motivo)
    }

    // 2. ------------------------------------------------------------------------------------
    fn pantalla(&mut self) -> Result<Veredicto, String> {
        let v = self.vista(caja(0.0, 0.0, 800.0, 600.0))?;
        let eventos: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        let e2 = eventos.clone();
        let h = ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
            let mut b = BOOL(0);
            if let Some(s) = sender {
                unsafe { s.ContainsFullScreenElement(&mut b)? };
            }
            e2.borrow_mut().push(b.as_bool());
            Ok(())
        }));
        let mut token = 0i64;
        unsafe { v.com.add_ContainsFullScreenElementChanged(&h, &mut token) }.e("add_ContainsFullScreenElementChanged")?;
        v.cargar(&self.url("/pantalla.html"))?;

        let pedir = "document.getElementById('caja').requestFullscreen().then(() => 'ok')";
        let r1 = v.eval_promesa(pedir);
        self.ev("execute_script_request", r1);
        let mut entrada = esperar(3_000, || eventos.borrow().contains(&true));
        self.ev("entrada_con_execute_script", json!(entrada));
        if !entrada {
            let r2 = cdp(
                &v.com,
                "Runtime.evaluate",
                &json!({"expression": pedir, "userGesture": true, "awaitPromise": true, "returnByValue": true}),
            );
            self.ev("cdp_user_gesture_request", r2.unwrap_or_else(|e| json!({"error": e})));
            entrada = esperar(3_000, || eventos.borrow().contains(&true));
            self.ev("entrada_con_cdp_user_gesture", json!(entrada));
        }
        if !entrada {
            self.ev("eventos", json!(*eventos.borrow()));
            return veredicto(false, "no funciona", "requestFullscreen no entró ni con ExecuteScript ni con gesto de CDP");
        }

        let window = self.window.as_ref().unwrap();
        let antes = window.inner_size();
        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        bombear(1_000);
        let dentro = window.inner_size();
        let _ = v.wv.set_bounds(Rect { position: PhysicalPosition::new(0, 0).into(), size: PhysicalSize::new(dentro.width, dentro.height).into() });
        bombear(500);
        let monitor = window.current_monitor().map(|m| [m.size().width, m.size().height]);
        let ventana_dentro = window.fullscreen().is_some();
        self.ev("ventana_entrar", json!({"antes": [antes.width, antes.height], "dentro": [dentro.width, dentro.height], "monitor": monitor, "fullscreen": ventana_dentro}));
        let pagina = v.eval("[document.fullscreenElement && document.fullscreenElement.id, innerWidth, innerHeight]");
        self.ev("pagina_dentro", json!(pagina));

        let r3 = v.eval_promesa("document.exitFullscreen().then(() => 'ok')");
        self.ev("exit_fullscreen", r3);
        let salida = esperar(3_000, || eventos.borrow().last() == Some(&false));
        let window = self.window.as_ref().unwrap();
        window.set_fullscreen(None);
        bombear(1_000);
        let fuera = window.inner_size();
        let _ = v.wv.set_bounds(caja(0.0, 0.0, 800.0, 600.0));
        let ventana_fuera = window.fullscreen().is_none();
        self.ev("ventana_salir", json!({"fuera": [fuera.width, fuera.height], "sin_fullscreen": ventana_fuera}));
        self.ev("eventos", json!(*eventos.borrow()));
        let ok = entrada && salida && ventana_dentro && ventana_fuera;
        veredicto(ok, if ok { "funciona" } else { "no funciona" }, format!("eventos {:?}", eventos.borrow()))
    }

    // 3. ------------------------------------------------------------------------------------
    fn fallos(&mut self) -> Result<Veredicto, String> {
        let v = self.vista(caja(0.0, 0.0, 800.0, 600.0))?;
        // WebResourceResponseReceived: every response with its status.
        let respuestas: Rc<RefCell<Vec<(String, i32)>>> = Rc::new(RefCell::new(Vec::new()));
        let r2 = respuestas.clone();
        let h = WebResourceResponseReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            let mut estado = 0i32;
            unsafe {
                args.Request()?.Uri(&mut uri)?;
                args.Response()?.StatusCode(&mut estado)?;
            }
            r2.borrow_mut().push((take_pwstr(uri), estado));
            Ok(())
        }));
        let wv2: ICoreWebView2_2 = v.com.cast().e("ICoreWebView2_2")?;
        let mut token = 0i64;
        unsafe { wv2.add_WebResourceResponseReceived(&h, &mut token) }.e("add_WebResourceResponseReceived")?;

        // DevTools protocol, Network domain: requests, responses and network failures.
        let pedidas: Rc<RefCell<HashMap<String, Value>>> = Rc::new(RefCell::new(HashMap::new()));
        let fallidas: Rc<RefCell<Vec<Value>>> = Rc::new(RefCell::new(Vec::new()));
        let p = pedidas.clone();
        cdp_evento(&v.com, "Network.requestWillBeSent", move |e| {
            if let Some(id) = e["requestId"].as_str() {
                p.borrow_mut().insert(id.into(), json!({"url": e["request"]["url"], "tipo": e["type"], "marco": e["frameId"]}));
            }
        })?;
        let p = pedidas.clone();
        let f = fallidas.clone();
        cdp_evento(&v.com, "Network.responseReceived", move |e| {
            let estado = e["response"]["status"].as_i64().unwrap_or(0);
            if estado >= 400 {
                let pedida = p.borrow().get(e["requestId"].as_str().unwrap_or("")).cloned().unwrap_or(Value::Null);
                f.borrow_mut().push(json!({"url": e["response"]["url"], "motivo": format!("HTTP {estado}"), "tipo": e["type"], "marco": e["frameId"], "pedida": pedida}));
            }
        })?;
        let p = pedidas.clone();
        let f = fallidas.clone();
        cdp_evento(&v.com, "Network.loadingFailed", move |e| {
            let pedida = p.borrow().get(e["requestId"].as_str().unwrap_or("")).cloned().unwrap_or(Value::Null);
            f.borrow_mut().push(json!({"url": pedida["url"], "motivo": e["errorText"], "cancelada": e["canceled"], "tipo": e["type"], "marco": pedida["marco"]}));
        })?;
        let red = cdp(&v.com, "Network.enable", &json!({}));
        self.ev("network_enable", json!(red.is_ok()));
        if let Err(e) = red {
            return veredicto(false, "no funciona", format!("Network.enable: {e}"));
        }

        self.ev("puerto_cerrado", json!(self.puertos.cerrado));
        let principal = self.url("/fallos.html");
        v.cargar(&principal)?;
        bombear(3_000);
        let arbol = cdp(&v.com, "Page.getFrameTree", &json!({})).unwrap_or(Value::Null);
        let marco_principal = arbol["frameTree"]["frame"]["id"].clone();
        let hijos: Vec<Value> = arbol["frameTree"]["childFrames"]
            .as_array()
            .map(|a| a.iter().map(|c| json!({"id": c["frame"]["id"], "url": c["frame"]["url"]})).collect())
            .unwrap_or_default();
        self.ev("marcos", json!({"principal": marco_principal, "hijos": hijos}));

        let resp = respuestas.borrow().clone();
        self.ev("response_received", json!(resp.iter().map(|(u, s)| format!("{s} {u}")).collect::<Vec<_>>()));
        // Visor's filter: the main document does not count.
        let lista: Vec<Value> = fallidas
            .borrow()
            .iter()
            .filter(|f| !(f["tipo"] == "Document" && f["marco"] == marco_principal))
            .cloned()
            .collect();
        self.ev("fallos_cdp", json!(lista));
        let tiene = |sufijo: &str| lista.iter().any(|f| f["url"].as_str().is_some_and(|u| u.ends_with(sufijo)));
        let por_respuesta = |sufijo: &str| resp.iter().any(|(u, s)| u.ends_with(sufijo) && *s >= 400);
        let comprobar = json!({
            "img_404": {"cdp": tiene("/no-existe.png"), "response_received": por_respuesta("/no-existe.png")},
            "script_red": {"cdp": tiene("/red.js"), "response_received": false},
            "iframe_mismo_origen_img_404": {"cdp": tiene("/hijo-no-existe.png"), "response_received": por_respuesta("/hijo-no-existe.png")},
            "iframe_otro_sitio_img_404": {"cdp": tiene("/externo-no-existe.png"), "response_received": por_respuesta("/externo-no-existe.png")},
            "principal_cuenta": lista.iter().any(|f| f["url"] == principal.as_str()),
        });
        self.ev("comprobacion", comprobar.clone());

        // The main document failing must not count either: load a 404 page.
        let n = fallidas.borrow().len();
        let _ = v.wv.load_url(&self.url("/no-hay.html"));
        bombear(2_000);
        let principal_404: Vec<Value> = fallidas.borrow()[n..].to_vec();
        let excluida = principal_404.iter().all(|f| f["tipo"] == "Document" && f["marco"] == marco_principal);
        self.ev("principal_404", json!({"eventos": principal_404, "filtrado_como_principal": excluida}));

        let tres = tiene("/no-existe.png") && tiene("/red.js") && tiene("/hijo-no-existe.png");
        let ok = tres && comprobar["principal_cuenta"] == false && excluida;
        let externo = tiene("/externo-no-existe.png");
        let resultado = if !ok { "no funciona" } else if externo { "funciona" } else { "funciona con límites" };
        veredicto(ok, resultado, format!("tres fallos por CDP: {tres}; iframe de otro sitio: {externo}"))
    }

    // 4. ------------------------------------------------------------------------------------
    fn estilo(&mut self) -> Result<Veredicto, String> {
        let v = self.vista(caja(0.0, 0.0, 800.0, 600.0))?;
        let marcos: Rc<RefCell<Vec<ICoreWebView2Frame>>> = Rc::new(RefCell::new(Vec::new()));
        let m2 = marcos.clone();
        let h = FrameCreatedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                m2.borrow_mut().push(unsafe { args.Frame()? });
            }
            Ok(())
        }));
        let wv4: ICoreWebView2_4 = v.com.cast().e("ICoreWebView2_4")?;
        let mut token = 0i64;
        unsafe { wv4.add_FrameCreated(&h, &mut token) }.e("add_FrameCreated")?;

        const C1: &str = "rgb(200, 100, 0)";
        const C2: &str = "rgb(0, 100, 200)";
        let id1 = añadir_script(&v.com, &script_tema(C1))?;
        v.cargar(&self.url("/estilo.html"))?;
        bombear(1_500);
        let m1 = medir_estilo(&v, &marcos.borrow());
        self.ev("con_tema_1", m1.clone());

        // Hot change: swap the document-created script and re-run it in every live frame.
        unsafe { v.com.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id1)) }.e("RemoveScript")?;
        let _id2 = añadir_script(&v.com, &script_tema(C2))?;
        let aplicar = script_tema(C2);
        v.eval(&aplicar);
        let en_marcos: Vec<bool> = marcos.borrow().iter().map(|f| ejecutar_en_marco(f, &aplicar).is_some()).collect();
        self.ev("execute_script_en_marcos", json!(en_marcos));
        bombear(300);
        let m2v = medir_estilo(&v, &marcos.borrow());
        self.ev("tema_2_sin_recargar", m2v.clone());

        marcos.borrow_mut().clear();
        let n = v.completadas.borrow().len();
        v.wv.reload().e("reload")?;
        esperar(10_000, || v.completadas.borrow().len() > n);
        bombear(1_500);
        let m3 = medir_estilo(&v, &marcos.borrow());
        self.ev("tema_2_tras_recargar", m3.clone());

        let cumple = |m: &Value, c: &str| {
            m["principal"]["body"] == c && m["mismo_origen"]["body"] == c && m["principal"]["normal"] == c
        };
        let ok = cumple(&m1, C1) && cumple(&m2v, C2) && cumple(&m3, C2);
        let otro = [&m1, &m2v, &m3].iter().zip([C1, C2, C2]).all(|(m, c)| {
            m["marcos"].as_array().is_some_and(|a| a.iter().any(|f| f["url"].as_str().is_some_and(|u| u.contains("localhost")) && f["body"] == c))
        });
        let importante_pagina = m1["principal"]["importante"] == "rgb(1, 2, 3)";
        let motivo = format!(
            "principal y mismo origen: {ok}; iframe de otro sitio: {otro}; la regla !important de la página gana al tema: {importante_pagina}"
        );
        let resultado = if !ok { "no funciona" } else if otro && !importante_pagina { "funciona" } else { "funciona con límites" };
        veredicto(ok, resultado, motivo)
    }

    // 5. ------------------------------------------------------------------------------------
    fn zoom(&mut self) -> Result<Veredicto, String> {
        let v = self.vista(caja(0.0, 0.0, 800.0, 600.0))?;
        let medir = "({dpr: devicePixelRatio, ancho: innerWidth, alto: innerHeight})";
        let factor = |v: &Vista| {
            let mut z = 0.0f64;
            let _ = unsafe { v.ctl.ZoomFactor(&mut z) };
            z
        };
        v.cargar(&self.url("/zoom-1.html"))?;
        let m0 = v.eval(medir).unwrap_or(Value::Null);
        v.wv.zoom(1.5).e("zoom")?;
        bombear(500);
        let m1 = v.eval(medir).unwrap_or(Value::Null);
        let f1 = factor(&v);
        v.cargar(&self.url("/zoom-2.html"))?;
        bombear(300);
        let m2 = v.eval(medir).unwrap_or(Value::Null);
        let f2 = factor(&v);
        // Another origin too (localhost), where per-origin zoom would show.
        v.cargar(&format!("http://localhost:{}/zoom-1.html", self.puertos.servidor))?;
        bombear(300);
        let m3 = v.eval(medir).unwrap_or(Value::Null);
        let f3 = factor(&v);
        self.ev("medidas", json!({"antes": m0, "zoom_1_5": m1, "factor_1": f1, "otra_pagina": m2, "factor_2": f2, "otro_origen": m3, "factor_3": f3}));
        let ancho = |m: &Value| m["ancho"].as_f64().unwrap_or(0.0);
        let dpr = |m: &Value| m["dpr"].as_f64().unwrap_or(0.0);
        let aplica = (ancho(&m1) - ancho(&m0) / 1.5).abs() <= 2.0 && (dpr(&m1) - dpr(&m0) * 1.5).abs() < 0.05;
        let conserva = (ancho(&m2) - ancho(&m1)).abs() <= 1.0 && (f2 - 1.5).abs() < 0.01;
        let ok = aplica && conserva;
        let motivo = format!("aplica {aplica}; se conserva al navegar {conserva}; otro origen ancho {}", ancho(&m3));
        veredicto(ok, if ok { "funciona" } else { "no funciona" }, motivo)
    }

    // 6. ------------------------------------------------------------------------------------
    fn foco(&mut self) -> Result<Veredicto, String> {
        let a = self.vista(caja(0.0, 0.0, 400.0, 600.0))?;
        let b = self.vista(caja(400.0, 0.0, 400.0, 600.0))?;
        let eventos: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        for (nombre, v) in [("A", &a), ("B", &b)] {
            let mut token = 0i64;
            let e = eventos.clone();
            let got = FocusChangedEventHandler::create(Box::new(move |_, _| {
                e.borrow_mut().push(format!("{nombre} GotFocus"));
                Ok(())
            }));
            let e = eventos.clone();
            let lost = FocusChangedEventHandler::create(Box::new(move |_, _| {
                e.borrow_mut().push(format!("{nombre} LostFocus"));
                Ok(())
            }));
            unsafe {
                v.ctl.add_GotFocus(&got, &mut token).e("add_GotFocus")?;
                v.ctl.add_LostFocus(&lost, &mut token).e("add_LostFocus")?;
            }
        }
        let url = self.url("/foco.html");
        a.cargar(&url)?;
        b.cargar(&url)?;
        if let Some(w) = self.window.as_ref() {
            w.focus_window();
        }
        bombear(500);
        let primer_plano = self.hwnd().map(|h| unsafe { GetForegroundWindow() } == h);
        self.ev("ventana_en_primer_plano", json!(primer_plano));
        self.ev("eventos_iniciales", json!(eventos.borrow().clone()));

        let n = eventos.borrow().len();
        b.wv.focus().e("focus B")?;
        let llega_b = esperar(3_000, || eventos.borrow()[n..].iter().any(|e| e == "B GotFocus"));
        bombear(200);
        let foco_b = (b.eval("document.hasFocus()"), a.eval("document.hasFocus()"));
        self.ev("tras_focus_b", json!({"eventos": eventos.borrow()[n..].to_vec(), "hasFocus_b": foco_b.0, "hasFocus_a": foco_b.1}));

        let n = eventos.borrow().len();
        a.wv.focus().e("focus A")?;
        let llega_a = esperar(3_000, || eventos.borrow()[n..].iter().any(|e| e == "A GotFocus"));
        bombear(200);
        let foco_a = (a.eval("document.hasFocus()"), b.eval("document.hasFocus()"));
        self.ev("tras_focus_a", json!({"eventos": eventos.borrow()[n..].to_vec(), "hasFocus_a": foco_a.0, "hasFocus_b": foco_a.1}));

        let ok = llega_b && llega_a && foco_b.0 == Some(json!(true)) && foco_a.0 == Some(json!(true));
        let motivo = format!("GotFocus B {llega_b}, A {llega_a}; hasFocus B {:?}, A {:?}", foco_b.0, foco_a.0);
        veredicto(ok, if ok { "funciona" } else { "no funciona" }, motivo)
    }

    // 7. ------------------------------------------------------------------------------------
    fn lateral(&mut self) -> Result<Veredicto, String> {
        let v = self.vista(caja(0.0, 0.0, 800.0, 600.0))?;
        v.cargar(&self.url("/lateral-1.html"))?;
        v.eval("sessionStorage.clear()");
        v.cargar(&self.url("/lateral-2.html"))?;
        let mut fases = Map::new();
        let mut entregado = false;
        for (fase, ruta, interceptar) in [
            ("sin_bloqueo", "/lateral-2.html", false),
            ("prevent_default", "/lateral-2.html?bloquear=1", false),
            ("cancelar_en_rust", "/lateral-2.html", true),
        ] {
            let destino = self.url(ruta);
            if source_of(&v.com) != destino {
                v.cargar(&destino)?;
            }
            v.eval("sessionStorage.setItem('ev', '[]')");
            v.interceptar.set(interceptar);
            let n = v.iniciadas.borrow().len();
            let envio = self.pulsar_atras();
            bombear(2_500);
            v.interceptar.set(false);
            let navs: Vec<Value> = v.iniciadas.borrow()[n..]
                .iter()
                .map(|(u, k, c)| json!({"uri": u, "tipo": k, "cancelada": c}))
                .collect();
            let eventos = v.eval("sessionStorage.getItem('ev')").unwrap_or(Value::Null);
            let ahora = source_of(&v.com);
            if !navs.is_empty() || eventos.as_str().is_some_and(|s| s.contains(" 3")) {
                entregado = true;
            }
            fases.insert(fase.into(), json!({"send_input": envio, "navegaciones": navs, "eventos_pagina": eventos, "url_final": ahora, "volvio_atras": ahora.ends_with("/lateral-1.html")}));
        }
        let fases = Value::Object(fases);
        self.ev("fases", fases.clone());
        if !entregado {
            return veredicto(false, "sin probar", "SendInput no llegó a la vista (sin escritorio interactivo); queda para W7");
        }
        let navega_solo = fases["sin_bloqueo"]["volvio_atras"] == true;
        let js_bloquea = fases["prevent_default"]["volvio_atras"] == false;
        let rust_cancela = fases["cancelar_en_rust"]["volvio_atras"] == false
            && fases["cancelar_en_rust"]["navegaciones"].as_array().is_some_and(|a| a.iter().any(|n| n["cancelada"] == true));
        let ok = rust_cancela || js_bloquea;
        let motivo = format!("WebView2 navega solo: {navega_solo}; preventDefault lo evita: {js_bloquea}; Rust lo cancela en NavigationStarting: {rust_cancela}");
        veredicto(ok, if ok { "funciona" } else { "no funciona" }, motivo)
    }

    /// XBUTTON1 (back) with SendInput at the centre of the window.
    fn pulsar_atras(&self) -> Value {
        let Some(window) = self.window.as_ref() else { return json!("sin ventana") };
        let Some(hwnd) = self.hwnd() else { return json!("sin HWND") };
        let pos = window.inner_position().unwrap_or_default();
        let tam = window.inner_size();
        let (x, y) = (pos.x + tam.width as i32 / 2, pos.y + tam.height as i32 / 2);
        unsafe {
            let primer_plano = SetForegroundWindow(hwnd).as_bool();
            let cursor = SetCursorPos(x, y).is_ok();
            bombear(200);
            let raton = |flags| INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 1, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
            };
            let entradas = [raton(MOUSEEVENTF_XDOWN), raton(MOUSEEVENTF_XUP)];
            let enviados = SendInput(&entradas, size_of::<INPUT>() as i32);
            let error = if enviados == 0 { windows::core::Error::from_thread().to_string() } else { String::new() };
            json!({"punto": [x, y], "set_foreground": primer_plano, "set_cursor": cursor, "enviados": enviados, "error": error})
        }
    }
}

fn leer_stream(stream: &IStream) -> Result<Vec<u8>, String> {
    unsafe {
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).e("Stat")?;
        stream.Seek(0, STREAM_SEEK_SET, None).e("Seek")?;
        let mut buf = vec![0u8; stat.cbSize as usize];
        let mut leidos = 0u32;
        stream.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut leidos)).ok().e("Read")?;
        buf.truncate(leidos as usize);
        Ok(buf)
    }
}

/// The injected theme. It is re-appended at DOMContentLoaded so it comes after the page's styles.
fn script_tema(color: &str) -> String {
    let css = format!("body{{background-color:{color}}} p{{color:{color} !important}}");
    format!(
        "(() => {{ const css = {css:?}; const put = () => {{ const raiz = document.head || document.documentElement; if (!raiz) return; let s = document.getElementById('__w4tema'); if (!s) {{ s = document.createElement('style'); s.id = '__w4tema'; }} s.textContent = css; raiz.appendChild(s); }}; put(); document.addEventListener('DOMContentLoaded', put, {{once: true}}); }})(); 0"
    )
}

fn añadir_script(com: &ICoreWebView2, js: &str) -> Result<String, String> {
    let r: Rc<RefCell<Option<Result<String, String>>>> = Rc::new(RefCell::new(None));
    let r2 = r.clone();
    let h = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |hr, id| {
        *r2.borrow_mut() = Some(hr.map(|_| id).map_err(|e| e.to_string()));
        Ok(())
    }));
    unsafe { com.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(js), &h) }.e("AddScriptToExecuteOnDocumentCreated")?;
    esperar(5_000, || r.borrow().is_some());
    r.borrow_mut().take().unwrap_or(Err("sin respuesta".into()))
}

const MEDIR_DOC: &str = "(d => { const c = (s, p) => s ? getComputedStyle(s)[p] : null; return {url: d.location.href, body: c(d.body, 'backgroundColor'), normal: c(d.querySelector('p.normal'), 'color'), importante: c(d.querySelector('p.importante'), 'color')}; })";

fn medir_estilo(v: &Vista, marcos: &[ICoreWebView2Frame]) -> Value {
    let principal = v.eval(&format!("{MEDIR_DOC}(document)")).unwrap_or(Value::Null);
    let mismo = v
        .eval(&format!("(() => {{ const f = document.getElementById('mismo'); try {{ return {MEDIR_DOC}(f.contentDocument); }} catch (e) {{ return {{error: String(e)}}; }} }})()"))
        .unwrap_or(Value::Null);
    let de_marcos: Vec<Value> = marcos.iter().map(|f| ejecutar_en_marco(f, &format!("{MEDIR_DOC}(document)")).unwrap_or(Value::Null)).collect();
    json!({"principal": principal, "mismo_origen": mismo, "marcos": de_marcos})
}

fn escribir(salida: &PathBuf, prueba: &str, ok: bool, resultado: &str, motivo: &str, t0: Instant, evidencia: &Evidencia) {
    let ev = evidencia.lock().map(|m| m.clone()).unwrap_or_default();
    let informe = json!({
        "prueba": prueba, "ok": ok, "resultado": resultado, "motivo": motivo,
        "ms_total": ms(t0.elapsed()), "evidencia": Value::Object(ev),
    });
    let texto = serde_json::to_string_pretty(&informe).unwrap_or_default();
    let _ = fs::write(salida, &texto);
    println!("{texto}");
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("W4 funciones (Windows)")
            .with_inner_size(LogicalSize::new(800.0, 600.0));
        match event_loop.create_window(attrs) {
            Ok(w) => self.window = Some(w),
            Err(e) => self.fin(Err(format!("no se pudo crear la ventana: {e}"))),
        }
        bombear(300);
        let r = match self.prueba.as_str() {
            "captura" => self.captura(),
            "pantalla" => self.pantalla(),
            "fallos" => self.fallos(),
            "estilo" => self.estilo(),
            "zoom" => self.zoom(),
            "foco" => self.foco(),
            "lateral" => self.lateral(),
            otra => Err(format!("prueba desconocida: {otra}")),
        };
        self.fin(r);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            std::process::exit(0);
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
        eprintln!("uso: w4-funciones --auto <salida.json> --prueba captura|pantalla|fallos|estilo|zoom|foco|lateral");
        std::process::exit(2);
    };
    let salida = PathBuf::from(salida);
    let puertos = server::start().expect("servidor local");
    let evidencia: Evidencia = Arc::new(Mutex::new(Map::new()));

    // Watchdog: writes what there is and exits if the test does not finish in time.
    let (s, p, e) = (salida.clone(), prueba.clone(), evidencia.clone());
    std::thread::spawn(move || {
        std::thread::sleep(LIMITE);
        escribir(&s, &p, false, "error", "tiempo agotado", t0, &e);
        std::process::exit(3);
    });

    let datos = std::env::temp_dir().join(format!("w4-funciones-{}", std::process::id()));
    let mut app = App { prueba, salida, t0, evidencia, puertos, contexto: WebContext::new(Some(datos)), window: None };
    let event_loop = EventLoop::new().expect("bucle de eventos");
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("el bucle terminó con error: {e}");
        std::process::exit(1);
    }
}
