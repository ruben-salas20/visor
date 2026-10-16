//! Casos del experimento W3 y su evaluación. No usa nada de Windows: se prueba en cualquier sistema.
//! Los casos comparten una sola sesión de WebView2: cada uno empieza navegando a su punto de partida.

use serde_json::{json, Value};

use crate::server::Ports;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Port {
    Main,
    Other,
    Closed,
}

#[derive(Clone, Copy, Debug)]
pub struct Url {
    pub port: Port,
    pub path: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Navigate(Url),
    Back,
    Forward,
    Reload,
}

#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub action: Action,
    /// Milliseconds after the case starts.
    pub after_ms: u64,
}

/// Intermediate check: the committed URL at `at_ms` must be `expect`.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub at_ms: u64,
    pub expect: Url,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub name: &'static str,
    pub steps: Vec<Step>,
    pub samples: Vec<Sample>,
    /// URL that must be committed when the case ends.
    pub expect: Url,
    pub expect_error: bool,
    /// URLs that must never appear as committed during the case.
    pub must_not_commit: &'static [Url],
    /// Source() at the end must show this URL. Otherwise the case does not hit the risk it names.
    pub source_risk: Option<Url>,
    /// Minimum number of ContentLoading commits during the case.
    pub min_loads: usize,
    /// The case does not end before this time, so late events have a chance to arrive.
    pub min_ms: u64,
}

fn base(name: &'static str, steps: Vec<Step>, expect: Url, min_ms: u64) -> Case {
    Case {
        name,
        steps,
        samples: Vec::new(),
        expect,
        expect_error: false,
        must_not_commit: &[],
        source_risk: None,
        min_loads: 0,
        min_ms,
    }
}

fn nav(path: &'static str, after_ms: u64) -> Step {
    Step { action: Action::Navigate(principal(path)), after_ms }
}

fn act(action: Action, after_ms: u64) -> Step {
    Step { action, after_ms }
}

fn principal(path: &'static str) -> Url {
    Url { port: Port::Main, path }
}

fn sample(at_ms: u64, path: &'static str) -> Sample {
    Sample { at_ms, expect: principal(path) }
}

pub fn all() -> Vec<Case> {
    vec![
        base("redireccion", vec![nav("/redir", 0)], principal("/b"), 1500),
        Case {
            must_not_commit: &[Url { port: Port::Main, path: "/falsa" }],
            source_risk: Some(Url { port: Port::Main, path: "/falsa" }),
            ..base("pushstate", vec![nav("/push", 0)], principal("/push"), 1500)
        },
        base("fragmento", vec![nav("/frag", 0)], principal("/frag"), 1500),
        Case {
            must_not_commit: &[
                Url { port: Port::Other, path: "/hijo" },
                Url { port: Port::Other, path: "/hijo2" },
            ],
            ..base("iframe", vec![nav("/iframe", 0)], principal("/iframe"), 1500)
        },
        Case {
            must_not_commit: &[Url { port: Port::Main, path: "/lenta" }],
            // /lenta answers after 5 s; wait past that so a late commit would show up.
            ..base("solapada", vec![nav("/lenta", 0), nav("/a", 300)], principal("/a"), 6000)
        },
        Case {
            expect_error: true,
            ..base(
                "error_red",
                vec![Step { action: Action::Navigate(Url { port: Port::Closed, path: "/" }), after_ms: 0 }],
                Url { port: Port::Closed, path: "/" },
                1500,
            )
        },
        Case {
            samples: vec![sample(1200, "/b")],
            ..base(
                "atras_entre_documentos",
                vec![nav("/a", 0), nav("/b", 800), act(Action::Back, 1600)],
                principal("/a"),
                2500,
            )
        },
        Case {
            samples: vec![sample(2000, "/a")],
            ..base(
                "adelante",
                vec![nav("/a", 0), nav("/b", 800), act(Action::Back, 1600), act(Action::Forward, 2400)],
                principal("/b"),
                3000,
            )
        },
        // Chromium skips history entries a page adds with pushState without a user gesture, so
        // Back from /falsa leaves the document and lands on /b (seen in CI, run 38944867295).
        Case {
            must_not_commit: &[Url { port: Port::Main, path: "/falsa" }],
            ..base(
                "atras_a_pushstate",
                vec![nav("/b", 0), nav("/push", 1000), act(Action::Back, 2500)],
                principal("/b"),
                3500,
            )
        },
        base("script_navega", vec![nav("/script", 0)], principal("/b"), 2000),
        Case {
            min_loads: 2,
            ..base("recarga", vec![nav("/a", 0), act(Action::Reload, 1000)], principal("/a"), 2500)
        },
        base("sin_contenido", vec![nav("/a", 0), nav("/vacio", 1000)], principal("/a"), 2500),
        base("descarga", vec![nav("/a", 0), nav("/descarga", 1000)], principal("/a"), 2500),
        Case {
            samples: vec![sample(2000, "/a")],
            ..base("pendiente", vec![nav("/a", 0), nav("/lenta", 1000)], principal("/lenta"), 8000)
        },
    ]
}

pub fn url_of(url: Url, ports: &Ports) -> String {
    let port = match url.port {
        Port::Main => ports.main,
        Port::Other => ports.other,
        Port::Closed => ports.closed,
    };
    format!("http://127.0.0.1:{port}{}", url.path)
}

/// What the runner saw during one case. Only `committed` (and `samples`) decide the verdict;
/// the Source() fields are recorded to show where `Source()` would have been wrong.
pub struct Seen {
    pub committed: Option<String>,
    pub committed_error: bool,
    pub history: Vec<String>,
    pub events: Vec<String>,
    /// Committed URL at each `Case::samples` entry, in order.
    pub samples: Vec<Option<String>>,
    pub completed: usize,
    pub source_al_cargar: String,
    pub source_al_final: String,
}

pub struct CaseResult {
    pub name: &'static str,
    pub expected: String,
    pub committed: Option<String>,
    pub committed_error: bool,
    pub history: Vec<String>,
    pub source_al_cargar: String,
    pub source_al_final: String,
    pub events: Vec<String>,
    pub ok: bool,
    pub reason: String,
}

pub fn evaluate(case: &Case, ports: &Ports, seen: Seen) -> CaseResult {
    let expected = url_of(case.expect, ports);
    let mut problems = Vec::new();
    if seen.completed == 0 {
        problems.push("tiempo agotado sin NavigationCompleted".to_string());
    }
    if seen.committed.as_deref() != Some(expected.as_str()) {
        problems.push(format!(
            "comprometida {} (esperada {expected})",
            seen.committed.as_deref().unwrap_or("ninguna")
        ));
    }
    if seen.committed_error != case.expect_error {
        problems.push(format!(
            "error_comprometida={} (esperado {})",
            seen.committed_error, case.expect_error
        ));
    }
    for url in case.must_not_commit {
        let forbidden = url_of(*url, ports);
        if seen.history.contains(&forbidden) {
            problems.push(format!("llegó a comprometerse {forbidden}"));
        }
    }
    for (sample, observed) in case.samples.iter().zip(seen.samples.iter()) {
        let esperada = url_of(sample.expect, ports);
        if observed.as_deref() != Some(esperada.as_str()) {
            problems.push(format!(
                "a los {} ms: comprometida {} (esperada {esperada})",
                sample.at_ms,
                observed.as_deref().unwrap_or("ninguna")
            ));
        }
    }
    if seen.history.len() < case.min_loads {
        problems.push(format!(
            "ContentLoading comprometidos: {} (mínimo {})",
            seen.history.len(),
            case.min_loads
        ));
    }
    if let Some(risk) = case.source_risk {
        let riesgo = url_of(risk, ports);
        if seen.source_al_final != riesgo {
            problems.push(format!(
                "el caso no ejercita el riesgo: Source() final es {} y no {riesgo}",
                seen.source_al_final
            ));
        }
    }
    CaseResult {
        name: case.name,
        expected,
        ok: problems.is_empty(),
        reason: if problems.is_empty() { "ok".into() } else { problems.join("; ") },
        committed: seen.committed,
        committed_error: seen.committed_error,
        history: seen.history,
        source_al_cargar: seen.source_al_cargar,
        source_al_final: seen.source_al_final,
        events: seen.events,
    }
}

/// A case that never ran because the global time limit was reached.
pub fn not_run(case: &Case, ports: &Ports, reason: &str) -> CaseResult {
    CaseResult {
        name: case.name,
        expected: url_of(case.expect, ports),
        committed: None,
        committed_error: false,
        history: Vec::new(),
        source_al_cargar: String::new(),
        source_al_final: String::new(),
        events: Vec::new(),
        ok: false,
        reason: reason.to_string(),
    }
}

pub fn to_json(results: &[CaseResult], ok: bool, limite_s: u64) -> Value {
    let casos: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "caso": r.name,
                "esperado": r.expected,
                "comprometida": r.committed,
                "error_comprometida": r.committed_error,
                "historial_comprometidas": r.history,
                "source_al_cargar": r.source_al_cargar,
                "source_al_final": r.source_al_final,
                "resultado": if r.ok { "ok" } else { "fallo" },
                "motivo": r.reason,
                "eventos": r.events,
            })
        })
        .collect();
    json!({
        "experimento": "W3: URL comprometida en WebView2 por NavigationId",
        "ok": ok,
        "limite_global_s": limite_s,
        "casos": casos,
    })
}

pub fn to_markdown(results: &[CaseResult]) -> String {
    let mut out = String::from(
        "| Caso | Esperada | Comprometida | Error | Source al cargar | Source al final | Resultado | Motivo |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    for r in results {
        let cell = |s: &str| s.replace('|', "\\|");
        out.push_str(&format!(
            "| {} | `{}` | `{}` | {} | `{}` | `{}` | {} | {} |\n",
            r.name,
            r.expected,
            r.committed.as_deref().unwrap_or("ninguna"),
            if r.committed_error { "sí" } else { "no" },
            cell(&r.source_al_cargar),
            cell(&r.source_al_final),
            if r.ok { "OK" } else { "**FALLO**" },
            cell(&r.reason),
        ));
    }
    out
}
