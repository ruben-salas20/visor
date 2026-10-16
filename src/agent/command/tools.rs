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
            "Opens a vault, a file or a web page in visor. Waits for the page to load and returns the errors and warnings it produced. Starts the app if it is closed. A folder opens as a vault; a .html that is not in any vault opens as a loose file; a .md that is not in any vault opens as a loose file only with loose set.",
            object(
                json!({
                    "target": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Absolute path, path relative to the open vault, or http or https URL.",
                    },
                    "place": {
                        "type": "string",
                        "enum": ["here", "tab", "beside"],
                        "description": "here: replaces the active tab (default). tab: opens a new tab. beside: opens it on the other side and splits the window if needed.",
                    },
                    "loose": {
                        "type": "boolean",
                        "description": "Opens a .md outside every vault alone, without a vault, its folder listed, notes or annotations (like a loose .html). A .md inside a vault is opened in its vault either way.",
                    },
                }),
                &["target"],
            ),
        ),
        tool(
            "close",
            "Closes tabs. Without arguments, closes the active tab of the focused side. With target, closes every tab (on either side) that shows that file. Returns how many it closed, plus the same as status.",
            object(
                json!({
                    "target": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Absolute path of the file whose tabs are closed.",
                    },
                }),
                &[],
            ),
        ),
        tool(
            "status",
            "Says which vault and which file the person is viewing. If they are on a web page, file is null and url holds the address. Includes the tabs on each side of the window and which one has focus.",
            object(json!({}), &[]),
        ),
        tool(
            "reload",
            "Reloads the current page and returns its errors and warnings. Fails if nothing is open.",
            object(json!({}), &[]),
        ),
        tool(
            "console",
            "Returns the console of the current page: console.log, JavaScript errors, resources that did not load and rejected promises. With errors_only, only the errors. With clear, empties the console after reading it.",
            object(
                json!({
                    "errors_only": { "type": "boolean", "description": "Returns only the errors." },
                    "clear": { "type": "boolean", "description": "Empties the console after reading it." },
                }),
                &[],
            ),
        ),
        tool(
            "screenshot",
            "Saves a PNG capture of what the person sees in the active tab, drawn by visor's engine (WebKit). Only for vault or loose pages; fails on an external web page. path must be absolute and end in .png.",
            object(
                json!({
                    "path": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Absolute path of the PNG file to write (replaces it if it exists).",
                    },
                }),
                &["path"],
            ),
        ),
        tool(
            "eval",
            "Runs JavaScript in the active page and returns its value as JSON (type says the JavaScript type). Useful for clicking a button, reading the DOM or measuring overflow. Only for vault or loose pages; fails on an external web page. The code can be one expression or several statements: the value of the last one is returned. It does not wait for promises.",
            object(
                json!({
                    "code": {
                        "type": "string",
                        "minLength": 1,
                        "description": "JavaScript code to run in the page.",
                    },
                }),
                &["code"],
            ),
        ),
        tool(
            "events",
            "Returns the data pages sent with visor.send() and the messages the person writes in their annotations (annotation instead of data; action says whether it is a comment or a reply). Without arguments, the last 20. With after, only those after that id. With wait, waits for a new one to arrive: use it when a page asks the person for something, or to follow their annotations.",
            object(
                json!({
                    "after": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Id of the last event you already know; returns only the later ones.",
                    },
                    "wait": {
                        "type": "number",
                        "minimum": 0,
                        "maximum": 600,
                        "description": "Waits up to this many seconds (maximum 600) for a new event to arrive. Without after, only events that arrive from now on count.",
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "How many of the most recent events to return if you do not use after (default 20).",
                    },
                }),
                &[],
            ),
        ),
        tool(
            "notes",
            "Returns the person's annotations: highlighted text, comments or marked elements. Each one is a conversation: comment is what the person wrote, and replies are the messages in order, with author (\"persona\" or \"agente\", the literal values). pending means the person is waiting for your reply. Without arguments, the annotations of the open page. With page, those of another page. With all, those of the whole vault. On the open page, anchored says whether each annotation still finds its text or element. On a PDF, target.page is the PDF page of the quote, and a note whose page is not shown yet is not counted as lost.",
            object(
                json!({
                    "page": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Path of the page, absolute or relative to the vault.",
                    },
                    "all": { "type": "boolean", "description": "Returns the annotations of the whole vault." },
                }),
                &[],
            ),
        ),
        tool(
            "notes_reply",
            "Replies to one of the person's annotations; the reply appears at once in their note and is added to the end of the conversation. After an events entry with annotation, reply this way. Do not edit the annotation files by hand.",
            object(
                json!({
                    "id": { "type": "string", "minLength": 1, "description": "Id of the annotation, as notes returns it." },
                    "text": { "type": "string", "minLength": 1, "description": "Text of the reply." },
                }),
                &["id", "text"],
            ),
        ),
        tool(
            "update",
            "Checks whether a new version of visor is available and installs it. Returns the result as text. Does not restart visor: the new version is used when it is opened again. Does not replace a build copy (target/).",
            object(json!({}), &[]),
        ),
        tool(
            "guide",
            "Returns the visor guide: how to write HTML pages and notes for visor, which data the person sends, and which commands exist. Read it before writing pages or notes for visor.",
            object(json!({}), &[]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::command::Command;

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
        assert_eq!(names, ["open", "close", "status", "reload", "console", "screenshot", "eval", "events", "notes", "notes_reply", "update", "guide"]);
    }
}
