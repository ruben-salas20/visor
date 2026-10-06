//! Turns a Markdown note into a full page: themed, with math, code highlighting,
//! Mermaid diagrams and `[[wiki]]` links, plus the editor behind Ctrl+E.

use crate::vault;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, html};
use std::fs;
use std::path::Path;

const STYLE: &str = include_str!("note.css");
const SCRIPT: &str = include_str!("note.js");

/// URL path, under the vault base, where the bundled libraries are served.
pub const ASSETS: &str = "__visor__/";

/// Renders the note at `rel` inside `root`.
pub fn page(source: &str, rel: &str, root: &Path) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut events: Vec<Event> = Parser::new_ext(source, options).collect();
    let pages = pages(root);
    let (mut math, mut code, mut mermaid) = (false, false, false);
    let mut title = None;

    enum Fix {
        Mermaid,
        Heading,
        Wiki,
    }
    let mut i = 0;
    while i < events.len() {
        let fix = match &events[i] {
            Event::InlineMath(_) | Event::DisplayMath(_) => {
                math = true;
                None
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                if lang.split_whitespace().next() == Some("mermaid") {
                    Some(Fix::Mermaid)
                } else {
                    code |= !lang.is_empty();
                    None
                }
            }
            Event::Start(Tag::Heading { .. }) => Some(Fix::Heading),
            Event::Start(Tag::Link { link_type: LinkType::WikiLink { .. }, .. }) => Some(Fix::Wiki),
            _ => None,
        };
        let end_of = |events: &[Event], done: fn(&Event) -> bool| events[i..].iter().position(done).map_or(i, |p| i + p);
        match fix {
            Some(Fix::Mermaid) => {
                mermaid = true;
                let end = end_of(&events, |e| matches!(e, Event::End(TagEnd::CodeBlock)));
                let text: String = events[i + 1..end]
                    .iter()
                    .filter_map(|e| if let Event::Text(t) = e { Some(t.as_ref()) } else { None })
                    .collect();
                let html = format!("<pre class=\"mermaid\">{}</pre>", escape(&text));
                events.splice(i..=end, [Event::Html(html.into())]);
            }
            Some(Fix::Heading) => {
                let end = end_of(&events, |e| matches!(e, Event::End(TagEnd::Heading(_))));
                let text: String = events[i + 1..end]
                    .iter()
                    .filter_map(|e| match e {
                        Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
                        _ => None,
                    })
                    .collect();
                if let Event::Start(Tag::Heading { level, id, .. }) = &mut events[i] {
                    if title.is_none() && *level == pulldown_cmark::HeadingLevel::H1 {
                        title = Some(text.clone());
                    }
                    if id.is_none() {
                        *id = Some(slug(&text).into());
                    }
                }
            }
            Some(Fix::Wiki) => {
                // [[note#Section]] reads as "note › Section", like in Obsidian.
                let piped = matches!(&events[i], Event::Start(Tag::Link { link_type: LinkType::WikiLink { has_pothole: true }, .. }));
                if let (false, Some(Event::Text(text))) = (piped, events.get_mut(i + 1))
                    && text.contains('#')
                {
                    *text = text.replacen('#', " › ", 1).into();
                }
                if let Event::Start(Tag::Link { dest_url, title, .. }) = &mut events[i] {
                    match wiki_target(dest_url, rel, &pages) {
                        Some(url) => *dest_url = url.into(),
                        None => {
                            *title = CowStr::from(format!("Crear la nota «{dest_url}»"));
                            let new = format!("#visor-new:{}", utf8_percent_encode(dest_url, NON_ALPHANUMERIC));
                            *dest_url = new.into();
                        }
                    }
                }
            }
            None => {}
        }
        i += 1;
    }

    // Front matter is metadata, not content.
    let mut skipping = false;
    events.retain(|e| match e {
        Event::Start(Tag::MetadataBlock(_)) => {
            skipping = true;
            false
        }
        Event::End(TagEnd::MetadataBlock(_)) => {
            skipping = false;
            false
        }
        _ => !skipping,
    });

    let mut body = String::with_capacity(source.len() * 2);
    html::push_html(&mut body, events.into_iter());

    let assets = format!("{}{ASSETS}", vault::base());
    let mut head = String::new();
    if math {
        head += &format!(
            "<link rel=\"stylesheet\" href=\"{assets}katex/katex.min.css\"><script defer src=\"{assets}katex/katex.min.js\"></script>"
        );
    }
    if code {
        head += &format!("<script defer src=\"{assets}hljs/highlight.min.js\"></script>");
    }
    if mermaid {
        head += &format!("<script defer src=\"{assets}mermaid/mermaid.min.js\"></script>");
    }
    let stem = Path::new(rel).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let config = serde_json::json!({ "math": math, "code": code, "mermaid": mermaid, "dir": dir });
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title><style>{STYLE}</style>{head}</head><body>\
<main id=\"note\">{body}</main>\
<div id=\"edit\" hidden data-visor-skip><div id=\"status\" role=\"status\"></div>\
<textarea id=\"editor\" spellcheck=\"false\" aria-label=\"Texto de la nota\"></textarea></div>\
<script>window.__visorNoteConfig = {config};\n{SCRIPT}</script></body></html>",
        title = escape(&title.unwrap_or(stem)),
    )
}

/// Every page in the vault, as relative paths, for resolving `[[wiki]]` links.
fn pages(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else if vault::is_page(&path) {
                out.push(vault::rel(root, &path));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out
}

/// Resolves `[[name]]`, `[[folder/name]]` or `[[name#Section]]` like Obsidian:
/// by file name anywhere in the vault, preferring the note's own folder and Markdown.
fn wiki_target(target: &str, from: &str, pages: &[String]) -> Option<String> {
    let (name, section) = target.split_once('#').map_or((target, None), |(n, s)| (n, Some(s)));
    let fragment = section.map_or(String::new(), |s| format!("#{}", slug(s)));
    let name = name.trim();
    if name.is_empty() {
        return Some(fragment);
    }
    let here = from.rsplit_once('/').map_or("", |(d, _)| d);
    let matches = |page: &&String| {
        let without_ext = page.rsplit_once('.').map_or(page.as_str(), |(p, _)| p);
        if name.contains('/') {
            page.eq_ignore_ascii_case(name) || without_ext.eq_ignore_ascii_case(name)
        } else {
            let file = page.rsplit('/').next().unwrap_or(page);
            let stem = without_ext.rsplit('/').next().unwrap_or(without_ext);
            file.eq_ignore_ascii_case(name) || stem.eq_ignore_ascii_case(name)
        }
    };
    let best = pages.iter().filter(matches).min_by_key(|page| {
        let dir = page.rsplit_once('/').map_or("", |(d, _)| d);
        (dir != here, !page.ends_with(".md"), page.len())
    })?;
    Some(format!("{}{fragment}", vault::url(best)))
}

/// Heading anchor: lowercase words joined by dashes, accents kept.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push('-');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_keep_accents() {
        assert_eq!(slug("Ondas y frecuencia: ¿qué son?"), "ondas-y-frecuencia-qué-son");
    }

    #[test]
    fn wiki_prefers_same_folder_and_markdown() {
        vault::set_base("http://127.0.0.1:1/s/".into());
        let pages: Vec<String> = ["fisica/ondas.html", "fisica/ondas.md", "historia/ondas.md", "roma.html"]
            .map(String::from)
            .into();
        assert_eq!(wiki_target("ondas", "fisica/x.md", &pages).unwrap(), "http://127.0.0.1:1/s/fisica/ondas.md");
        assert_eq!(wiki_target("ondas", "historia/y.md", &pages).unwrap(), "http://127.0.0.1:1/s/historia/ondas.md");
        assert_eq!(wiki_target("Roma#El Senado", "a.md", &pages).unwrap(), "http://127.0.0.1:1/s/roma.html#el-senado");
        assert_eq!(wiki_target("historia/ondas", "a.md", &pages).unwrap(), "http://127.0.0.1:1/s/historia/ondas.md");
        assert!(wiki_target("no existe", "a.md", &pages).is_none());
    }
}
