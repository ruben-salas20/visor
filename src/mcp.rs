//! Model Context Protocol server over stdio. Each tool is one `visor` order,
//! sent to the app through the same socket the CLI uses.
use crate::cli;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;

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
        "tools/list" => Ok(json!({ "tools": tools() })),
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
    let Some(tool) = tools().into_iter().find(|t| t["name"] == name) else {
        return text(&format!("Herramienta desconocida: {name}. Usa tools/list para ver las disponibles."), true);
    };
    if let Err(e) = validate(&args, &tool["inputSchema"]) {
        return text(&e, true);
    }
    if name == "guide" {
        return text(GUIDE, false);
    }
    let request = match request(name, &args) {
        Ok(request) => request,
        Err(e) => return text(&e, true),
    };
    let response = cli::call(&request);
    text(&serde_json::to_string_pretty(&response).unwrap_or_default(), response["ok"] != true)
}

fn text(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

/// Turns checked arguments into the request the app reads, the same one the CLI sends.
fn request(name: &str, args: &Value) -> Result<Value, String> {
    Ok(match name {
        "open" => {
            let target = args["target"].as_str().unwrap_or_default();
            let place = args["place"].as_str();
            if target.starts_with("http://") || target.starts_with("https://") {
                json!({ "cmd": "open", "url": target, "place": place })
            } else if Path::new(target).is_absolute() {
                json!({ "cmd": "open", "path": target, "raw": target, "place": place })
            } else {
                // A relative path is left for the app to resolve inside the open vault. This
                // process's directory belongs to the client, not to the person's vault.
                json!({ "cmd": "open", "path": "", "raw": target, "place": place })
            }
        }
        "console" => json!({
            "cmd": "console",
            "level": (args["errors_only"] == true).then_some("error"),
            "clear": args["clear"] == true,
        }),
        "events" => json!({
            "cmd": "events",
            "after": args["after"].as_u64(),
            "wait": args["wait"].as_f64(),
            "limit": args["limit"].as_u64(),
        }),
        "notes" => match (args["page"].as_str(), args["all"] == true) {
            (Some(_), true) => return Err("Usa page o all, no los dos.".into()),
            (_, true) => json!({ "cmd": "notes", "all": true }),
            (Some(page), _) if Path::new(page).is_absolute() => json!({ "cmd": "notes", "path": page, "raw": page }),
            (Some(page), _) => json!({ "cmd": "notes", "path": "", "raw": page }),
            (None, _) => json!({ "cmd": "notes" }),
        },
        "notes_reply" => json!({ "cmd": "notes_reply", "id": args["id"], "text": args["text"] }),
        // status and reload take no arguments.
        _ => json!({ "cmd": name }),
    })
}

/// Checks arguments against a tool's schema. It reads only the keywords used in `tools()`.
fn validate(args: &Value, schema: &Value) -> Result<(), String> {
    let Some(args) = args.as_object() else {
        return Err("Los argumentos deben ser un objeto.".into());
    };
    let properties = &schema["properties"];
    if let Some(key) = args.keys().find(|k| properties[k.as_str()].is_null()) {
        return Err(format!("Argumento desconocido: {key}."));
    }
    for key in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if !args.contains_key(key) {
            return Err(format!("Falta el argumento obligatorio: {key}."));
        }
    }
    for (key, value) in args {
        let spec = &properties[key.as_str()];
        let fits = match spec["type"].as_str() {
            Some("string") => value.as_str().is_some_and(|s| {
                spec["minLength"].as_u64().is_none_or(|min| s.chars().count() as u64 >= min)
                    && spec["enum"].as_array().is_none_or(|options| options.contains(value))
            }),
            Some("boolean") => value.is_boolean(),
            Some("integer") => value.is_i64() || value.is_u64(),
            Some("number") => value.is_number(),
            _ => true,
        } && spec["minimum"].as_f64().is_none_or(|min| value.as_f64().is_some_and(|v| v >= min))
            && spec["maximum"].as_f64().is_none_or(|max| value.as_f64().is_some_and(|v| v <= max));
        if !fits {
            return Err(format!("El argumento {key} no es válido."));
        }
    }
    Ok(())
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn tools() -> Vec<Value> {
    vec![
        tool(
            "open",
            "Abre una bóveda, un archivo o una web en visor. Espera a que la página cargue y devuelve los errores y avisos que generó. Abre la app si está cerrada. Una carpeta se abre como bóveda; un .html que no está en ninguna bóveda se abre suelto.",
            object(
                json!({
                    "target": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Ruta absoluta, ruta relativa a la bóveda abierta, o URL http o https.",
                    },
                    "place": {
                        "type": "string",
                        "enum": ["here", "tab", "beside"],
                        "description": "here: sustituye la pestaña activa (por defecto). tab: abre una pestaña nueva. beside: la abre en el otro lado y divide la ventana si hace falta.",
                    },
                }),
                &["target"],
            ),
        ),
        tool(
            "status",
            "Dice qué bóveda y qué archivo está viendo la persona. Si está en una web, file es null y url tiene la dirección. Incluye las pestañas de cada lado de la ventana y cuál tiene el foco.",
            object(json!({}), &[]),
        ),
        tool(
            "reload",
            "Recarga la página actual y devuelve sus errores y avisos. Falla si no hay nada abierto.",
            object(json!({}), &[]),
        ),
        tool(
            "console",
            "Devuelve la consola de la página actual: console.log, errores de JavaScript, recursos que no cargaron y promesas rechazadas. Con errors_only, solo los errores. Con clear, vacía la consola después de leerla.",
            object(
                json!({
                    "errors_only": { "type": "boolean", "description": "Devuelve solo los errores." },
                    "clear": { "type": "boolean", "description": "Vacía la consola después de leerla." },
                }),
                &[],
            ),
        ),
        tool(
            "events",
            "Devuelve los datos que las páginas enviaron con visor.send(). Sin argumentos, los 20 últimos. Con after, solo los posteriores a ese id. Con wait, espera a que llegue un dato nuevo: úsalo cuando una página pida algo a la persona.",
            object(
                json!({
                    "after": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Id del último evento que ya conoces; devuelve solo los posteriores.",
                    },
                    "wait": {
                        "type": "number",
                        "minimum": 0,
                        "maximum": 600,
                        "description": "Espera hasta estos segundos (máximo 600) a que llegue un evento nuevo. Sin after, solo cuentan los eventos que lleguen desde ahora.",
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Cuántos de los eventos más recientes devolver si no usas after (por defecto 20).",
                    },
                }),
                &[],
            ),
        ),
        tool(
            "notes",
            "Devuelve las anotaciones de la persona: texto subrayado, comentarios o elementos marcados. Sin argumentos, las de la página abierta. Con page, las de otra página. Con all, las de toda la bóveda. En la página abierta, anchored indica si cada anotación todavía encuentra su texto o elemento.",
            object(
                json!({
                    "page": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Ruta de la página, absoluta o relativa a la bóveda.",
                    },
                    "all": { "type": "boolean", "description": "Devuelve las anotaciones de toda la bóveda." },
                }),
                &[],
            ),
        ),
        tool(
            "notes_reply",
            "Responde a una anotación de la persona; la respuesta aparece al momento en su nota. No edites a mano los archivos de anotaciones.",
            object(
                json!({
                    "id": { "type": "string", "minLength": 1, "description": "Id de la anotación, tal como lo devuelve notes." },
                    "text": { "type": "string", "minLength": 1, "description": "Texto de la respuesta." },
                }),
                &["id", "text"],
            ),
        ),
        tool(
            "guide",
            "Devuelve la guía de visor: cómo escribir páginas HTML y notas para visor, qué datos envía la persona y qué órdenes hay. Léela antes de escribir páginas o notas para visor.",
            object(json!({}), &[]),
        ),
    ]
}
