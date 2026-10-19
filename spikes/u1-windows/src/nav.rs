//! URL confirmada de una vista por la API COM de WebView2: NavigationStarting + ContentLoading
//! por NavigationId. Copiado de spikes/w3-committed/src/runner.rs. `Source()` solo se registra.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use webview2_com::{
    take_pwstr, ContentLoadingEventHandler, DownloadStartingEventHandler, HistoryChangedEventHandler,
    NavigationCompletedEventHandler, NavigationStartingEventHandler, SourceChangedEventHandler,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2_4},
};
use windows_core::{Interface, BOOL, PWSTR};

#[derive(Default)]
pub struct Nav {
    /// NavigationId -> URI, filled by NavigationStarting. A redirect overwrites the URI of its id.
    pub pending: HashMap<u64, String>,
    pub committed: Option<String>,
    pub committed_error: bool,
    pub history: Vec<String>,
    pub events: Vec<String>,
    pub completed: usize,
    /// Source() read at the last ContentLoading of the current case. Log only.
    pub source_al_cargar: String,
}

fn source_of(wv: &ICoreWebView2) -> String {
    let mut p = PWSTR::null();
    if unsafe { wv.Source(&mut p) }.is_ok() { take_pwstr(p) } else { String::new() }
}

pub fn register(wv: &ICoreWebView2, nav: &Rc<RefCell<Nav>>) -> windows_core::Result<()> {
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

