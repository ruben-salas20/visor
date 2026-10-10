//! Annotations: a layer of highlights and comments over vault pages, stored
//! apart from them in `.visor/anotaciones/<page>.json`, so pages never change.
//! The app is the only writer; pages propose changes and agents reply.
//! Each annotation is a conversation: the person's comment, then `replies`
//! from either side, in order. Messages are only ever added.

use serde_json::{Value, json};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const DIR: &str = ".visor/anotaciones";
const COLORS: [&str; 4] = ["amarillo", "verde", "azul", "rosa"];
const MAX_TEXT: usize = 20_000;
const MAX_TITLE: usize = 200;
const MAX_REPLIES: usize = 200;

/// Page writes come from the event loop and agent replies from the socket
/// threads; one lock keeps them from overwriting each other.
static LOCK: Mutex<()> = Mutex::new(());

pub fn file(root: &Path, page: &str) -> PathBuf {
    root.join(DIR).join(format!("{page}.json"))
}

/// The page an annotation file belongs to, if `path` is one.
pub fn page_of(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root.join(DIR)).ok()?;
    let rel = crate::vault::rel(Path::new(""), rel);
    rel.strip_suffix(".json").map(str::to_string)
}

fn load(root: &Path, page: &str) -> Value {
    fs::read(file(root, page))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|doc| doc["annotations"].is_array())
        .unwrap_or_else(|| json!({ "page": page, "annotations": [] }))
}

fn store(root: &Path, page: &str, doc: &Value) -> io::Result<()> {
    let path = file(root, page);
    if doc["annotations"].as_array().is_some_and(Vec::is_empty) {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    crate::vault::write_atomic(&path, serde_json::to_string_pretty(doc).unwrap())
}

fn text(value: &Value) -> Option<String> {
    let s = value.as_str()?;
    (s.chars().count() <= MAX_TEXT).then(|| s.to_string())
}

/// Optional one-line title. Absent or blank means none; notes saved before
/// titles existed have none either.
fn title(value: &Value) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let s = value.as_str().ok_or("El título no es texto")?;
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > MAX_TITLE {
        return Err("Título demasiado largo".into());
    }
    Ok((!s.is_empty()).then_some(s))
}

fn set_title(note: &mut Value, title: Option<String>) {
    let map = note.as_object_mut().unwrap();
    match title {
        Some(t) => map.insert("title".into(), json!(t)),
        None => map.remove("title"),
    };
}

/// Adds or updates an annotation sent by a page. Only the fields a person
/// edits are taken from it; creation time and replies are kept. Returns true
/// when the comment is new or changed and not blank, which agents must hear about.
pub fn upsert(root: &Path, page: &str, incoming: &Value) -> Result<bool, String> {
    let id = incoming["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let id = id.ok_or("Anotación sin id válido")?;
    let kind = incoming["target"]["kind"].as_str();
    if !matches!(kind, Some("text" | "element")) {
        return Err("Tipo de anotación desconocido".into());
    }
    let color = incoming["color"].as_str().filter(|c| COLORS.contains(c)).unwrap_or("amarillo");
    let comment = text(&incoming["comment"]).ok_or("Comentario demasiado largo")?;
    let title = title(&incoming["title"])?;
    let target = incoming["target"].clone();
    if target.to_string().len() > MAX_TEXT {
        return Err("Ancla demasiado grande".into());
    }

    let _guard = LOCK.lock().unwrap();
    let mut doc = load(root, page);
    let list = doc["annotations"].as_array_mut().unwrap();
    let now = crate::session::now();
    let mut before = String::new();
    match list.iter_mut().find(|a| a["id"] == id) {
        Some(existing) => {
            before = existing["comment"].as_str().unwrap_or_default().to_string();
            existing["color"] = json!(color);
            existing["comment"] = json!(comment);
            existing["target"] = target;
            existing["updated"] = json!(now);
            set_title(existing, title);
        }
        None => {
            let mut note = json!({
                "id": id,
                "author": "persona",
                "created": now,
                "updated": now,
                "color": color,
                "comment": comment,
                "target": target,
                "replies": [],
            });
            set_title(&mut note, title);
            list.push(note);
        }
    }
    store(root, page, &doc).map_err(|e| e.to_string())?;
    Ok(comment != before && !comment.trim().is_empty())
}

/// Appends one message to an annotation's conversation. Nothing else changes.
fn add_message(note: &mut Value, author: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.chars().count() > MAX_TEXT {
        return Err("La respuesta está vacía o es demasiado larga.".into());
    }
    if !note["replies"].is_array() {
        note["replies"] = json!([]);
    }
    let replies = note["replies"].as_array_mut().unwrap();
    if replies.len() >= MAX_REPLIES {
        return Err(format!("La anotación ya tiene {MAX_REPLIES} mensajes y no admite más."));
    }
    replies.push(json!({ "author": author, "text": text, "created": crate::session::now() }));
    Ok(())
}

/// The person's reply to annotation `id` of `page`. A page can only answer
/// its own notes, and only add a message.
pub fn person_reply(root: &Path, page: &str, id: &str, text: &str) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap();
    let mut doc = load(root, page);
    let note = doc["annotations"].as_array_mut().unwrap().iter_mut().find(|a| a["id"] == id);
    let note = note.ok_or_else(|| format!("No hay ninguna anotación con id {id}"))?;
    add_message(note, "persona", text)?;
    store(root, page, &doc).map_err(|e| e.to_string())
}

pub fn delete(root: &Path, page: &str, id: &str) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap();
    let mut doc = load(root, page);
    doc["annotations"].as_array_mut().unwrap().retain(|a| a["id"] != id);
    store(root, page, &doc).map_err(|e| e.to_string())
}

/// Whether the person is waiting for an agent: their message is the last one.
/// Without replies, their comment counts as that message.
fn pending(note: &Value) -> bool {
    match note["replies"].as_array().and_then(|r| r.last()) {
        Some(last) => last["author"] == "persona",
        None => note["comment"].as_str().is_some_and(|c| !c.trim().is_empty()),
    }
}

/// Annotations of one page, or of the whole vault when `page` is `None`.
pub fn list(root: &Path, page: Option<&str>) -> Vec<Value> {
    let _guard = LOCK.lock().unwrap();
    let pages = match page {
        Some(page) => vec![page.to_string()],
        None => {
            let mut files = vec![];
            collect(&root.join(DIR), &mut files);
            files.iter().filter_map(|f| page_of(root, f)).collect()
        }
    };
    pages
        .iter()
        .flat_map(|page| {
            let doc = load(root, page);
            doc["annotations"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(move |mut a| {
                    a["page"] = json!(page);
                    a["pending"] = json!(pending(&a));
                    a
                })
        })
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

/// Adds an agent's reply to the annotation `id`, wherever it is. Returns its page.
pub fn reply(root: &Path, id: &str, reply: &str) -> Result<String, String> {
    let _guard = LOCK.lock().unwrap();
    let mut files = vec![];
    collect(&root.join(DIR), &mut files);
    for page in files.iter().filter_map(|f| page_of(root, f)) {
        let mut doc = load(root, &page);
        let Some(note) = doc["annotations"].as_array_mut().unwrap().iter_mut().find(|a| a["id"] == id) else {
            continue;
        };
        add_message(note, "agente", reply)?;
        store(root, &page, &doc).map_err(|e| e.to_string())?;
        return Ok(page);
    }
    Err(format!("No hay ninguna anotación con id {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("visor-annot-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_reply_and_delete() {
        let root = vault();
        let note = json!({ "id": "a1", "color": "verde", "comment": "duda",
            "target": { "kind": "text", "quote": { "exact": "onda" } } });
        upsert(&root, "física/ondas.html", &note).unwrap();
        assert_eq!(reply(&root, "a1", "Es la propagación").unwrap(), "física/ondas.html");
        // Editing the note again must keep the agent's reply.
        upsert(&root, "física/ondas.html", &json!({ "id": "a1", "color": "rosa", "comment": "ya",
            "target": { "kind": "text", "quote": { "exact": "onda" } } })).unwrap();
        let all = list(&root, None);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0]["replies"][0]["author"], "agente");
        assert_eq!(all[0]["color"], "rosa");
        assert_eq!(all[0]["page"], "física/ondas.html");
        assert!(all[0].get("title").is_none());
        delete(&root, "física/ondas.html", "a1").unwrap();
        assert!(!file(&root, "física/ondas.html").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn element_title() {
        let root = vault();
        let target = json!({ "kind": "element", "element": { "tag": "canvas", "css": "#onda" } });
        upsert(&root, "p.html", &json!({ "id": "e1", "comment": "", "title": "  Diagrama en\n «Ondas» ", "target": target })).unwrap();
        assert_eq!(list(&root, None)[0]["title"], "Diagrama en «Ondas»");
        upsert(&root, "p.html", &json!({ "id": "e1", "comment": "", "title": "", "target": target })).unwrap();
        assert!(list(&root, None)[0].get("title").is_none());
        assert!(upsert(&root, "p.html", &json!({ "id": "e1", "comment": "", "title": "x".repeat(201), "target": target })).is_err());
        assert!(upsert(&root, "p.html", &json!({ "id": "e1", "comment": "", "title": 3, "target": target })).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_bad_input() {
        let root = vault();
        assert!(upsert(&root, "p.html", &json!({ "id": "../x", "target": { "kind": "text" } })).is_err());
        assert!(upsert(&root, "p.html", &json!({ "id": "ok", "target": { "kind": "script" } })).is_err());
        assert!(reply(&root, "nada", "hola").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    fn quote_note(id: &str, comment: &str) -> Value {
        json!({ "id": id, "comment": comment, "target": { "kind": "text", "quote": { "exact": "onda" } } })
    }

    #[test]
    fn conversation_and_pending() {
        let root = vault();
        let page = "física/ondas.html";
        // A comment alone waits for an agent; a blank one does not.
        assert_eq!(upsert(&root, page, &quote_note("c1", "¿Qué es?")), Ok(true));
        upsert(&root, page, &quote_note("b1", "   ")).unwrap();
        let all = list(&root, None);
        assert_eq!(all.iter().find(|a| a["id"] == "c1").unwrap()["pending"], true);
        assert_eq!(all.iter().find(|a| a["id"] == "b1").unwrap()["pending"], false);

        // The person answers; the agent answers; the person answers again.
        person_reply(&root, page, "c1", "Gracias, y ¿la frecuencia?").unwrap();
        let note = list(&root, Some(page))[0].clone();
        assert_eq!(note["replies"][0]["author"], "persona");
        assert_eq!(note["pending"], true);
        reply(&root, "c1", "Es una perturbación.").unwrap();
        assert_eq!(list(&root, Some(page))[0]["pending"], false);
        person_reply(&root, page, "c1", "Vale").unwrap();
        // Editing the comment keeps the whole conversation.
        assert_eq!(upsert(&root, page, &quote_note("c1", "¿Qué es? (editado)")), Ok(true));
        let note = list(&root, Some(page))[0].clone();
        let authors: Vec<_> = note["replies"].as_array().unwrap().iter().map(|r| r["author"].as_str().unwrap()).collect();
        assert_eq!(authors, ["persona", "agente", "persona"]);
        assert_eq!(note["pending"], true);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn upsert_reports_only_new_comments() {
        let root = vault();
        let page = "p.html";
        assert_eq!(upsert(&root, page, &quote_note("a", "")), Ok(false));
        assert_eq!(upsert(&root, page, &quote_note("a", "duda")), Ok(true));
        assert_eq!(upsert(&root, page, &quote_note("a", "duda")), Ok(false));
        assert_eq!(upsert(&root, page, &quote_note("a", "  ")), Ok(false));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn person_reply_limits() {
        let root = vault();
        let page = "p.html";
        upsert(&root, page, &quote_note("a", "")).unwrap();
        // Only the note's own page, and only notes that exist.
        assert!(person_reply(&root, "otra.html", "a", "hola").is_err());
        assert!(person_reply(&root, page, "nada", "hola").is_err());
        assert!(person_reply(&root, page, "a", "  ").is_err());
        assert!(person_reply(&root, page, "a", &"x".repeat(MAX_TEXT + 1)).is_err());
        assert!(person_reply(&root, page, "a", &"x".repeat(MAX_TEXT)).is_ok());
        // Without a comment, the person's reply is the last message.
        assert_eq!(list(&root, Some(page))[0]["pending"], true);
        for _ in 1..MAX_REPLIES {
            person_reply(&root, page, "a", "más").unwrap();
        }
        let full = person_reply(&root, page, "a", "una más").unwrap_err();
        assert!(full.contains("200"), "{full}");
        assert_eq!(list(&root, Some(page))[0]["replies"].as_array().unwrap().len(), MAX_REPLIES);
        // The agent is also capped: the same function decides.
        assert!(reply(&root, "a", "agente").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
