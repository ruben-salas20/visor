//! Definitions of the MCP tools: name, description and JSON schema. The checks
//! on their arguments live in `Command::from_mcp`; the test below keeps both in step.
use serde_json::{Value, json};

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

pub fn list() -> Vec<Value> {
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
            "Devuelve los datos que las páginas enviaron con visor.send() y los mensajes que la persona escribe en sus anotaciones (con annotation en lugar de data; action dice si es un comentario o una respuesta). Sin argumentos, los 20 últimos. Con after, solo los posteriores a ese id. Con wait, espera a que llegue uno nuevo: úsalo cuando una página pida algo a la persona o para seguir sus anotaciones.",
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
            "Devuelve las anotaciones de la persona: texto subrayado, comentarios o elementos marcados. Cada una es una conversación: comment es lo que escribió la persona y replies sus mensajes en orden, con author (\"persona\" o \"agente\"). pending indica que la persona espera tu respuesta. Sin argumentos, las de la página abierta. Con page, las de otra página. Con all, las de toda la bóveda. En la página abierta, anchored indica si cada anotación todavía encuentra su texto o elemento.",
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
            "Responde a una anotación de la persona; la respuesta aparece al momento en su nota y se añade al final de la conversación. Tras un evento de events con annotation, responde así. No edites a mano los archivos de anotaciones.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    /// A value the schema of `spec` accepts.
    fn sample(spec: &Value) -> Value {
        match spec["type"].as_str() {
            Some("boolean") => json!(true),
            Some("integer") => json!(spec["minimum"].as_u64().unwrap_or(1)),
            Some("number") => json!(spec["minimum"].as_f64().unwrap_or(1.0)),
            _ => spec["enum"][0].as_str().map_or(json!("x"), |s| json!(s)),
        }
    }

    #[test]
    fn schemas_match_the_checks() {
        for tool in list() {
            let name = tool["name"].as_str().unwrap();
            let schema = &tool["inputSchema"];
            let properties = schema["properties"].as_object().unwrap();
            let required: Vec<&str> = schema["required"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            // Each property is accepted on its own (with the required ones).
            let mut base = serde_json::Map::new();
            for key in &required {
                base.insert(key.to_string(), sample(&properties[*key]));
            }
            assert!(Command::from_mcp(name, &Value::Object(base.clone())).is_ok(), "{name}");
            for (key, spec) in properties {
                let mut args = base.clone();
                args.insert(key.clone(), sample(spec));
                assert!(Command::from_mcp(name, &Value::Object(args)).is_ok(), "{name}.{key}");
            }
            // The required ones are really required, and nothing else is accepted.
            for key in &required {
                let mut args = base.clone();
                args.remove(*key);
                assert_eq!(
                    Command::from_mcp(name, &Value::Object(args)),
                    Err(format!("Falta el argumento obligatorio: {key}."))
                );
            }
            let mut args = base;
            args.insert("inesperado".into(), json!(1));
            assert_eq!(
                Command::from_mcp(name, &Value::Object(args)),
                Err("Argumento desconocido: inesperado.".to_string())
            );
        }
    }

    #[test]
    fn the_tools_are_these() {
        let names: Vec<String> = list().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["open", "status", "reload", "console", "events", "notes", "notes_reply", "guide"]);
    }
}
