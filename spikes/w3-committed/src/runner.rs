//! Runner de Windows: un WebView2 real, los handlers de navegación y el bucle de casos.
//! La URL comprometida sale solo de NavigationStarting + ContentLoading (por NavigationId).
//! `Source()` se lee únicamente para el registro comparativo.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use tao::{event::Event, event_loop::{ControlFlow, EventLoop}, window::WindowBuilder};
use webview2_com::{
    take_pwstr, ContentLoadingEventHandler, HistoryChangedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, SourceChangedEventHandler,
    Microsoft::Web::WebView2::Win32::ICoreWebView2,
};
use windows_core::{BOOL, HSTRING, PWSTR};
use wry::{WebViewBuilder, WebViewExtWindows};

use crate::{
    cases::{self, Case, CaseResult, Seen},
    server::{self, Ports},
};

const LIMITE_GLOBAL: Duration = Duration::from_secs(90);
const TICK: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Nav {
    /// NavigationId -> URI, filled by NavigationStarting. A redirect overwrites the URI of its id.
    pending: HashMap<u64, String>,
    committed: Option<String>,
    committed_error: bool,
    history: Vec<String>,
    events: Vec<String>,
    completed: usize,
    /// Last Source() value, for the comparative log only.
    source_seen: String,
}

fn register(wv: &ICoreWebView2, nav: &Rc<RefCell<Nav>>) -> windows_core::Result<()> {
    let nav_start = nav.clone();
    let on_start = NavigationStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut uri = PWSTR::null();
        let mut id = 0u64;
        let mut redirigida = BOOL(0);
        unsafe {
            args.Uri(&mut uri)?;
            args.NavigationId(&mut id)?;
            args.IsRedirected(&mut redirigida)?;
        }
        let uri = take_pwstr(uri);
        let mut n = nav_start.borrow_mut();
        n.events.push(format!("NavigationStarting id={id} redirigida={} uri={uri}", redirigida.as_bool()));
        n.pending.insert(id, uri);
        Ok(())
    }));

    let nav_load = nav.clone();
    let on_loading = ContentLoadingEventHandler::create(Box::new(move |webview, args| {
        let Some(args) = args else { return Ok(()) };
        let mut id = 0u64;
        let mut error_page = BOOL(0);
        unsafe {
            args.NavigationId(&mut id)?;
            args.IsErrorPage(&mut error_page)?;
        }
        // Log only: this value never decides the committed URL.
        let source = match webview {
            Some(wv) => {
                let mut p = PWSTR::null();
                if unsafe { wv.Source(&mut p) }.is_ok() { take_pwstr(p) } else { String::new() }
            }
            None => String::new(),
        };
        let mut n = nav_load.borrow_mut();
        if !source.is_empty() {
            n.source_seen = source.clone();
        }
        match n.pending.get(&id).cloned() {
            Some(uri) => {
                n.events.push(format!(
                    "ContentLoading id={id} error={} uri={uri} Source()={source}",
                    error_page.as_bool()
                ));
                n.committed = Some(uri.clone());
                n.committed_error = error_page.as_bool();
                n.history.push(uri);
            }
            // Fail closed: a document whose navigation we never saw starting is not trusted.
            None => {
                n.events.push(format!("ContentLoading id={id} sin NavigationStarting previo: sin URL comprometida"));
                n.committed = None;
                n.committed_error = false;
            }
        }
        Ok(())
    }));

    let nav_done = nav.clone();
    let on_completed = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let mut ok = BOOL(0);
        let mut id = 0u64;
        unsafe {
            args.IsSuccess(&mut ok)?;
            args.NavigationId(&mut id)?;
        }
        let mut n = nav_done.borrow_mut();
        n.completed += 1;
        n.events.push(format!("NavigationCompleted id={id} exito={}", ok.as_bool()));
        Ok(())
    }));

    let nav_src = nav.clone();
    let on_source = SourceChangedEventHandler::create(Box::new(move |_, _| {
        nav_src.borrow_mut().events.push("SourceChanged (solo registro)".into());
        Ok(())
    }));

    let nav_hist = nav.clone();
    let on_history = HistoryChangedEventHandler::create(Box::new(move |_, _| {
        nav_hist.borrow_mut().events.push("HistoryChanged (solo registro)".into());
        Ok(())
    }));

    let mut token = 0i64;
    unsafe {
        wv.add_NavigationStarting(&on_start, &mut token)?;
        wv.add_ContentLoading(&on_loading, &mut token)?;
        wv.add_NavigationCompleted(&on_completed, &mut token)?;
        wv.add_SourceChanged(&on_source, &mut token)?;
        wv.add_HistoryChanged(&on_history, &mut token)?;
    }
    Ok(())
}

struct Run {
    cases: Vec<Case>,
    ports: Ports,
    idx: usize,
    started: Option<Instant>,
    fired: usize,
    results: Vec<CaseResult>,
    wv: ICoreWebView2,
    nav: Rc<RefCell<Nav>>,
    global_start: Instant,
    out_dir: PathBuf,
}

impl Run {
    /// Advances the case list. Returns the exit code once everything has finished.
    fn tick(&mut self) -> Option<i32> {
        if self.idx >= self.cases.len() {
            return Some(self.finish());
        }
        if self.global_start.elapsed() >= LIMITE_GLOBAL {
            for case in &self.cases[self.idx..] {
                self.results.push(cases::not_run(case, &self.ports, "límite global de 90 s alcanzado"));
            }
            self.idx = self.cases.len();
            return Some(self.finish());
        }

        let case = self.cases[self.idx].clone();
        if self.started.is_none() {
            *self.nav.borrow_mut() = Nav::default();
            self.started = Some(Instant::now());
            self.fired = 0;
        }
        let elapsed = self.started.map(|s| s.elapsed()).unwrap_or_default();

        while self.fired < case.steps.len()
            && elapsed >= Duration::from_millis(case.steps[self.fired].after_ms)
        {
            let url = cases::url_of(case.steps[self.fired].url, &self.ports);
            if let Err(e) = unsafe { self.wv.Navigate(&HSTRING::from(url.as_str())) } {
                self.nav.borrow_mut().events.push(format!("Navigate a {url} fallo: {e}"));
            }
            self.fired += 1;
        }

        if elapsed >= Duration::from_millis(case.min_ms) {
            let (seen, timed_out) = {
                let n = self.nav.borrow();
                (
                    Seen {
                        committed: n.committed.clone(),
                        committed_error: n.committed_error,
                        history: n.history.clone(),
                        events: n.events.clone(),
                        source: n.source_seen.clone(),
                    },
                    n.completed == 0,
                )
            };
            self.results.push(cases::evaluate(&case, &self.ports, seen, timed_out));
            self.idx += 1;
            self.started = None;
        }
        None
    }

    fn finish(&self) -> i32 {
        let ok = self.results.len() == self.cases.len() && self.results.iter().all(|r| r.ok);
        let json = serde_json::to_string_pretty(&cases::to_json(&self.results, ok)).unwrap_or_default();
        let md = cases::to_markdown(&self.results);
        let mut code = if ok { 0 } else { 1 };
        for (name, body) in [("w3-resultados.json", json.as_str()), ("w3-resultados.md", md.as_str())] {
            if let Err(e) = std::fs::write(self.out_dir.join(name), body) {
                eprintln!("No se pudo escribir {name}: {e}");
                code = 1;
            }
        }
        println!("{md}");
        println!("Resultado global: {}", if ok { "OK" } else { "FALLO" });
        code
    }
}

pub fn run(out_dir: PathBuf) -> i32 {
    let ports = match server::start() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("No se pudo abrir el servidor local: {e}");
            return 1;
        }
    };
    let event_loop = EventLoop::new();
    let window = match WindowBuilder::new().with_title("visor W3").build(&event_loop) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("No se pudo crear la ventana: {e}");
            return 1;
        }
    };
    // No initial URL: the handlers must be registered before the first navigation.
    let webview = match WebViewBuilder::new().build(&window) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("No se pudo crear WebView2: {e}");
            return 1;
        }
    };
    let wv = webview.webview();
    let nav = Rc::new(RefCell::new(Nav::default()));
    if let Err(e) = register(&wv, &nav) {
        eprintln!("No se pudieron registrar los eventos de navegación: {e}");
        return 1;
    }

    let mut run = Run {
        cases: cases::all(),
        ports,
        idx: 0,
        started: None,
        fired: 0,
        results: Vec::new(),
        wv,
        nav,
        global_start: Instant::now(),
        out_dir,
    };

    event_loop.run(move |event, _, control_flow| {
        // Keep the window and the webview alive for the whole run.
        let _ = (&window, &webview);
        *control_flow = ControlFlow::WaitUntil(Instant::now() + TICK);
        if let Event::NewEvents(_) = event {
            if let Some(code) = run.tick() {
                std::process::exit(code);
            }
        }
    })
}
