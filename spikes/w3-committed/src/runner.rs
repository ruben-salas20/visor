//! Runner de Windows: un WebView2 real, los handlers de navegación y el bucle de casos.
//! La URL comprometida sale solo de NavigationStarting + ContentLoading (por NavigationId).
//! `Source()` se lee solo para el registro comparativo, nunca para el veredicto.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use tao::{event::Event, event_loop::{ControlFlow, EventLoop}, window::WindowBuilder};
use webview2_com::{
    take_pwstr, ContentLoadingEventHandler, DownloadStartingEventHandler, HistoryChangedEventHandler,
    NavigationCompletedEventHandler, NavigationStartingEventHandler, SourceChangedEventHandler,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2_4},
};
use windows_core::{Interface, BOOL, HSTRING, PWSTR};
use wry::{WebViewBuilder, WebViewExtWindows};

use crate::{
    cases::{self, Action, Case, CaseResult, Seen},
    server::{self, Ports},
};

const LIMITE_GLOBAL: Duration = Duration::from_secs(90);
const TICK: Duration = Duration::from_millis(50);

/// Estado de navegación de toda la sesión. No se reinicia entre casos: así un caso puede
/// depender de la página que dejó el anterior. Cada caso toma sus propios índices.
#[derive(Default)]
struct Nav {
    /// NavigationId -> URI, filled by NavigationStarting. A redirect overwrites the URI of its id.
    pending: HashMap<u64, String>,
    committed: Option<String>,
    committed_error: bool,
    history: Vec<String>,
    events: Vec<String>,
    completed: usize,
    /// Source() read at the last ContentLoading of the current case. Log only.
    source_al_cargar: String,
}

fn source_of(wv: &ICoreWebView2) -> String {
    let mut p = PWSTR::null();
    if unsafe { wv.Source(&mut p) }.is_ok() { take_pwstr(p) } else { String::new() }
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
        // Source() at load: recorded for comparison only, it never decides the committed URL.
        let source = webview.as_ref().map(source_of).unwrap_or_default();
        let mut n = nav_load.borrow_mut();
        n.source_al_cargar = source.clone();
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

    // Downloads are cancelled so that /descarga does not replace the page; the case checks the page stays.
    let nav_dl = nav.clone();
    let on_download = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let cancelada = unsafe { args.SetCancel(true) }.is_ok();
        nav_dl.borrow_mut().events.push(format!("DownloadStarting cancelada={cancelada}"));
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
    let wv4: ICoreWebView2_4 = wv.cast()?;
    unsafe { wv4.add_DownloadStarting(&on_download, &mut token)? };
    Ok(())
}

struct Run {
    cases: Vec<Case>,
    ports: Ports,
    idx: usize,
    started: Option<Instant>,
    fired: usize,
    next_sample: usize,
    samples_seen: Vec<Option<String>>,
    // Where this case's slices begin in the shared Nav.
    ev_start: usize,
    hist_start: usize,
    completed_start: usize,
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
            let motivo = format!("límite global de {} s alcanzado", LIMITE_GLOBAL.as_secs());
            for case in &self.cases[self.idx..] {
                self.results.push(cases::not_run(case, &self.ports, &motivo));
            }
            self.idx = self.cases.len();
            return Some(self.finish());
        }

        let case = self.cases[self.idx].clone();
        if self.started.is_none() {
            {
                let mut n = self.nav.borrow_mut();
                self.ev_start = n.events.len();
                self.hist_start = n.history.len();
                self.completed_start = n.completed;
                n.source_al_cargar = String::new();
            }
            self.started = Some(Instant::now());
            self.fired = 0;
            self.next_sample = 0;
            self.samples_seen.clear();
        }
        let elapsed = self.started.map(|s| s.elapsed()).unwrap_or_default();

        while self.fired < case.steps.len()
            && elapsed >= Duration::from_millis(case.steps[self.fired].after_ms)
        {
            let result = match case.steps[self.fired].action {
                Action::Navigate(url) => {
                    let url = cases::url_of(url, &self.ports);
                    let r = unsafe { self.wv.Navigate(&HSTRING::from(url.as_str())) };
                    (r, format!("Navigate a {url}"))
                }
                Action::Back => (unsafe { self.wv.GoBack() }, "GoBack".to_string()),
                Action::Forward => (unsafe { self.wv.GoForward() }, "GoForward".to_string()),
                Action::Reload => (unsafe { self.wv.Reload() }, "Reload".to_string()),
            };
            if let (Err(e), what) = result {
                self.nav.borrow_mut().events.push(format!("{what} fallo: {e}"));
            }
            self.fired += 1;
        }

        while self.next_sample < case.samples.len()
            && elapsed >= Duration::from_millis(case.samples[self.next_sample].at_ms)
        {
            self.samples_seen.push(self.nav.borrow().committed.clone());
            self.next_sample += 1;
        }

        if elapsed >= Duration::from_millis(case.min_ms) {
            // Source() at the end: must agree with the case's risk when it has one (see cases::evaluate).
            let source_al_final = source_of(&self.wv);
            let seen = {
                let n = self.nav.borrow();
                Seen {
                    committed: n.committed.clone(),
                    committed_error: n.committed_error,
                    history: n.history[self.hist_start..].to_vec(),
                    events: n.events[self.ev_start..].to_vec(),
                    samples: self.samples_seen.clone(),
                    completed: n.completed - self.completed_start,
                    source_al_cargar: n.source_al_cargar.clone(),
                    source_al_final,
                }
            };
            self.results.push(cases::evaluate(&case, &self.ports, seen));
            self.idx += 1;
            self.started = None;
        }
        None
    }

    fn finish(&self) -> i32 {
        let ok = self.results.len() == self.cases.len() && self.results.iter().all(|r| r.ok);
        let json = serde_json::to_string_pretty(&cases::to_json(&self.results, ok, LIMITE_GLOBAL.as_secs()))
            .unwrap_or_default();
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
        next_sample: 0,
        samples_seen: Vec::new(),
        ev_start: 0,
        hist_start: 0,
        completed_start: 0,
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
