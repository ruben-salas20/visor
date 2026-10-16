//! Casos del experimento W3 y su evaluación. No usa nada de Windows: se prueba en cualquier sistema.

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
pub struct Step {
    pub url: Url,
    /// Milliseconds after the case starts.
    pub after_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub name: &'static str,
    pub steps: Vec<Step>,
    /// URL that must be committed when the case ends.
    pub expect: Url,
    pub expect_error: bool,
    /// URLs that must never appear as committed during the case.
    pub must_not_commit: &'static [Url],
    /// The case does not end before this time, so late events have a chance to arrive.
    pub min_ms: u64,
}

pub fn all() -> Vec<Case> {
    const fn at(path: &'static str, after_ms: u64) -> Step {
        Step { url: Url { port: Port::Main, path }, after_ms }
    }
    vec![
        Case {
            name: "redireccion",
            steps: vec![at("/redir", 0)],
            expect: Url { port: Port::Main, path: "/b" },
            expect_error: false,
            must_not_commit: &[],
            min_ms: 1500,
        },
        Case {
            name: "pushstate",
            steps: vec![at("/push", 0)],
            expect: Url { port: Port::Main, path: "/push" },
            expect_error: false,
            must_not_commit: &[Url { port: Port::Main, path: "/falsa" }],
            min_ms: 1500,
        },
        Case {
            name: "fragmento",
            steps: vec![at("/frag", 0)],
            expect: Url { port: Port::Main, path: "/frag" },
            expect_error: false,
            must_not_commit: &[],
            min_ms: 1500,
        },
        Case {
            name: "iframe",
            steps: vec![at("/iframe", 0)],
            expect: Url { port: Port::Main, path: "/iframe" },
            expect_error: false,
            must_not_commit: &[Url { port: Port::Other, path: "/hijo" }, Url { port: Port::Other, path: "/hijo2" }],
            min_ms: 1500,
        },
        Case {
            name: "solapada",
            steps: vec![at("/lenta", 0), at("/a", 300)],
            expect: Url { port: Port::Main, path: "/a" },
            expect_error: false,
            must_not_commit: &[Url { port: Port::Main, path: "/lenta" }],
            // /lenta answers after 5 s; wait past that so a late commit would show up.
            min_ms: 6000,
        },
        Case {
            name: "error_red",
            steps: vec![Step { url: Url { port: Port::Closed, path: "/" }, after_ms: 0 }],
            expect: Url { port: Port::Closed, path: "/" },
            expect_error: true,
            must_not_commit: &[],
            min_ms: 1500,
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

/// What the runner saw during one case. Only `committed` decides the verdict;
/// `source` is recorded to show where `Source()` would have been wrong.
pub struct Seen {
    pub committed: Option<String>,
    pub committed_error: bool,
    pub history: Vec<String>,
    pub events: Vec<String>,
    pub source: String,
}

pub struct CaseResult {
    pub name: &'static str,
    pub expected: String,
    pub committed: Option<String>,
    pub committed_error: bool,
    pub history: Vec<String>,
    pub source: String,
    pub events: Vec<String>,
    pub ok: bool,
    pub reason: String,
}

pub fn evaluate(case: &Case, ports: &Ports, seen: Seen, timed_out: bool) -> CaseResult {
    let expected = url_of(case.expect, ports);
    let mut problems = Vec::new();
    if timed_out {
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
    CaseResult {
        name: case.name,
        expected,
        ok: problems.is_empty(),
        reason: if problems.is_empty() { "ok".into() } else { problems.join("; ") },
        committed: seen.committed,
        committed_error: seen.committed_error,
        history: seen.history,
        source: seen.source,
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
        source: String::new(),
        events: Vec::new(),
        ok: false,
        reason: reason.to_string(),
    }
}

pub fn to_json(results: &[CaseResult], ok: bool) -> Value {
    let casos: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "caso": r.name,
                "esperado": r.expected,
                "comprometida": r.committed,
                "error_comprometida": r.committed_error,
                "historial_comprometidas": r.history,
                "source": r.source,
                "source_igual_a_esperada": r.source == r.expected,
                "resultado": if r.ok { "ok" } else { "fallo" },
                "motivo": r.reason,
                "eventos": r.events,
            })
        })
        .collect();
    json!({
        "experimento": "W3: URL comprometida en WebView2 por NavigationId",
        "ok": ok,
        "limite_global_s": 90,
        "casos": casos,
    })
}

pub fn to_markdown(results: &[CaseResult]) -> String {
    let mut out = String::from(
        "| Caso | Esperada | Comprometida | Error | Source() | Resultado | Motivo |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for r in results {
        let cell = |s: &str| s.replace('|', "\\|");
        out.push_str(&format!(
            "| {} | `{}` | `{}` | {} | `{}` | {} | {} |\n",
            r.name,
            r.expected,
            r.committed.as_deref().unwrap_or("ninguna"),
            if r.committed_error { "sí" } else { "no" },
            cell(&r.source),
            if r.ok { "OK" } else { "**FALLO**" },
            cell(&r.reason),
        ));
    }
    out
}
