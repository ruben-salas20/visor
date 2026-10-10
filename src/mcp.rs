//! Model Context Protocol server over stdio. Each tool is one `visor` order,
//! sent to the app through the same socket the CLI uses.
use crate::cli;
use crate::command::{Command, tools};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

const VERSIONS: [&str; 4] = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const DEFAULT_VERSION: &str = "2025-06-18";
const GUIDE: &str = include_str!("help.txt");

pub fn run() -> i32 {
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return 0,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(reply) = handle(line) else { continue };
        if writeln!(output, "{reply}").and_then(|()| output.flush()).is_err() {
            return 0;
        }
    }
}

fn handle(line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(_) => return Some(rpc_error(Value::Null, -32700, "JSON no válido.")),
    };
    if !message.is_object() {
        return Some(rpc_error(Value::Null, -32600, "Petición no válida: debe ser un objeto."));
    }
    // Notifications carry no id and are never answered.
    let id = message.get("id")?.clone();
    let Some(method) = message["method"].as_str() else {
        return Some(rpc_error(id, -32600, "Petición no válida: falta el método."));
    };
    let result = match method {
        "initialize" => Ok(initialize(&message["params"])),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list() })),
        "tools/call" => Ok(call_tool(&message["params"])),
        _ => Err(format!("Método no encontrado: {method}.")),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => rpc_error(id, -32601, &message),
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let asked = params["protocolVersion"].as_str().unwrap_or_default();
    let version = if VERSIONS.contains(&asked) { asked } else { DEFAULT_VERSION };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "visor", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "visor es una app de escritorio para estudiar con IA: muestra bóvedas (carpetas) de páginas HTML y notas Markdown, y guarda lo que la persona subraya, comenta y responde. Antes de escribir páginas o notas para visor, llama a la herramienta guide, que explica cómo hacerlo.",
    })
}

fn call_tool(params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let args = if params["arguments"].is_null() { json!({}) } else { params["arguments"].clone() };
    let command = match Command::from_mcp(name, &args) {
        Ok(Command::Guide) => return text(GUIDE, false),
        Ok(command) => command,
        Err(e) => return text(&e, true),
    };
    let response = cli::call(&command);
    text(&serde_json::to_string_pretty(&response).unwrap_or_default(), response["ok"] != true)
}

fn text(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}
