//! Turns a Markdown note into a full page: themed, with math, code highlighting,
//! Mermaid diagrams and `[[wiki]]` links, plus the editor behind Ctrl+E.

use crate::vault::{
    self,
    front_matter::{self, Value},
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, html};
use std::cell::OnceCell;
use std::path::{Path, PathBuf};

const STYLE: &str = include_str!("../page/note.css");
const SCRIPT: &str = include_str!("../page/note.js");

/// URL path, under the vault base, where the bundled libraries are served.
pub const ASSETS: &str = "__visor__/";

/// Wraps a block's source lines until `mark_lines` moves them into its tag.
/// Raw NULs never reach the HTML: CommonMark replaces them.
const LINE_MARK: char = '\0';

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

/// Renders the note at `rel` inside `root`.
///
/// Blocks carry `data-line` and, if longer than a line, `data-end` (first and
/// last 1-based source lines), so the editor can open where
/// the reader was, and checkboxes carry `data-task` (byte offset of their `[ ]`),
/// so a click can tick exactly that task in the file.
pub fn page(source: &str, rel: &str, root: &Path) -> String {
    let lines = !source.contains(LINE_MARK);
    let line_starts: Vec<usize> = std::iter::once(0).chain(source.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset);
    let mut events: Vec<Event> = Vec::new();
    // The front matter's range and its text, between its start and end events.
    let mut front: Option<(std::ops::Range<usize>, String)> = None;
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::MetadataBlock(_)) => front = Some((range, String::new())),
            Event::Text(text) if front.is_some() => {
                if let Some((_, yaml)) = &mut front {
                    yaml.push_str(&text);
                }
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                if let Some((range, yaml)) = front.take() {
                    let attrs = if lines {
                        // Like other blocks, the front matter opens the editor where it starts, and ends on its closing `---`.
                        let content = source[range.clone()].trim_end().len();
                        let last = line_of(range.start + content.saturating_sub(1));
                        format!(" data-line=\"{}\" data-end=\"{last}\"", line_of(range.start))
                    } else {
                        String::new()
                    };
                    events.push(Event::Html(front_matter_html(&yaml, &attrs).into()));
                }
            }
            Event::TaskListMarker(done) => {
                let checked = if done { " checked" } else { "" };
                let html = format!("<input type=\"checkbox\" data-task=\"{}\"{checked}>", range.start);
                events.push(Event::InlineHtml(html.into()));
            }
            Event::Start(
                Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::List(_)
                | Tag::Item
                | Tag::Table(_)
                | Tag::TableRow
                | Tag::FootnoteDefinition(_),
            ) if lines => {
                // Ranges of list items include the blank lines after them.
                let content = source[range.clone()].trim_end().len();
                let (first, last) = (line_of(range.start), line_of(range.start + content.saturating_sub(1)));
                let end = if last > first { format!(" data-end=\"{last}\"") } else { String::new() };
                events.push(Event::Html(format!("{LINE_MARK} data-line=\"{first}\"{end}{LINE_MARK}").into()));
                events.push(event);
            }
            _ => events.push(event),
        }
    }
    // Walking the vault costs a stat per file; most notes have no [[links]].
    let index = Index::new(root);
    let (mut math, mut code, mut mermaid) = (false, false, false);
    let mut title = None;

    enum Fix {
        Mermaid,
        Heading,
        Wiki,
        Embed,
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
            Event::Start(Tag::Image { link_type: LinkType::WikiLink { .. }, .. }) => Some(Fix::Embed),
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
                    match wiki_target(dest_url, rel, index.pages()) {
                        Some(url) => *dest_url = url.into(),
                        None => {
                            *title = CowStr::from(format!("Crear la nota «{dest_url}»"));
                            let new = format!("#visor-new:{}", utf8_percent_encode(dest_url, NON_ALPHANUMERIC));
                            *dest_url = new.into();
                        }
                    }
                }
            }
            Some(Fix::Embed) => {
                if let Event::Start(Tag::Image { dest_url, .. }) = &mut events[i]
                    && let Some(url) = embed_in(dest_url, rel, &index)
                {
                    *dest_url = url.into();
                }
            }
            None => {}
        }
        i += 1;
    }

    let mut body = String::with_capacity(source.len() * 2);
    html::push_html(&mut body, events.into_iter());
    if lines {
        body = mark_lines(&body);
    }

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
    let config = serde_json::json!({ "math": math, "code": code, "mermaid": mermaid, "dir": dir, "assets": assets });
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title><style>{STYLE}</style>{head}</head><body>\
<main id=\"note\">{body}</main>\
<div id=\"edit\" hidden data-visor-skip><div id=\"status\" role=\"status\"></div>\
<div id=\"editor\"></div></div>\
<script>window.__visorNoteConfig = {config};\n{SCRIPT}</script></body></html>",
        title = escape(&title.unwrap_or(stem)),
    )
}

/// Moves the attributes in each line mark into the tag that follows it. A mark
/// followed by text (a paragraph in a tight list has no tag) is dropped.
fn mark_lines(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + html.len() / 8);
    let mut rest = html;
    while let Some(start) = rest.find(LINE_MARK) {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(LINE_MARK) else { break };
        let attributes = &after[..end];
        rest = after[end + 1..].trim_start();
        let name = rest.strip_prefix('<').filter(|r| r.starts_with(|c: char| c.is_ascii_alphabetic()));
        if let Some(name) = name {
            let len = 1 + name.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(name.len());
            out.push_str(&rest[..len]);
            out.push_str(attributes);
            rest = &rest[len..];
        }
    }
    out.push_str(rest);
    out
}

/// Ticks or unticks the task whose `[ ]` starts at byte `offset`. Returns
/// `None` if no task starts there any more, for example because the file changed.
pub fn toggle_task(source: &str, offset: usize, checked: bool) -> Option<String> {
    let found = Parser::new_ext(source, options())
        .into_offset_iter()
        .any(|(event, range)| matches!(event, Event::TaskListMarker(_)) && range.start == offset);
    if !found || !matches!(source.get(offset..offset + 3), Some("[ ]" | "[x]" | "[X]")) {
        return None;
    }
    let mut text = source.to_string();
    text.replace_range(offset + 1..offset + 2, if checked { "x" } else { " " });
    Some(text)
}

/// Every page in the vault, as relative paths, for resolving `[[wiki]]` links.
pub(crate) fn pages(root: &Path) -> Vec<String> {
    Index::new(root).pages().to_vec()
}

/// The pages and images of a vault, found by one walk the first time either is
/// asked for and shared by every link of a note.
struct Index<'a> {
    root: &'a Path,
    found: OnceCell<(Vec<String>, Vec<String>)>,
}

impl<'a> Index<'a> {
    fn new(root: &'a Path) -> Self {
        Index { root, found: OnceCell::new() }
    }

    fn found(&self) -> &(Vec<String>, Vec<String>) {
        self.found.get_or_init(|| {
            let all = vault::files(self.root, self.root, |p| vault::is_page(p) || is_image(p));
            let rel = |paths: Vec<&PathBuf>| paths.into_iter().map(|p| vault::rel(self.root, p)).collect();
            let (pages, images) = all.iter().partition(|p| vault::is_page(p));
            (rel(pages), rel(images))
        })
    }

    fn pages(&self) -> &[String] {
        &self.found().0
    }

    fn images(&self) -> &[String] {
        &self.found().1
    }
}

fn is_image(path: &Path) -> bool {
    vault::mime(path).starts_with("image/")
}

/// URL of the image that `![[name]]` in the note at `rel` shows: found by name
/// anywhere in the vault, like `[[links]]`, preferring the note's own folder.
pub fn embed_url(name: &str, rel: &str, root: &Path) -> Option<String> {
    embed_in(name, rel, &Index::new(root))
}

fn embed_in(name: &str, rel: &str, index: &Index) -> Option<String> {
    let name = name.split('|').next().unwrap_or(name);
    if !is_image(Path::new(name.trim())) {
        return None;
    }
    wiki_page(name, rel, index.images()).map(|p| vault::url(p))
}

/// URL of the `[[target]]` written in the note at `rel`, for the editor, which
/// sees links as they are typed. `None` if no page matches.
pub fn wiki_url(target: &str, rel: &str, root: &Path) -> Option<String> {
    wiki_target(target, rel, Index::new(root).pages())
}

/// Resolves `[[name]]`, `[[folder/name]]` or `[[name#Section]]` like Obsidian:
/// by file name anywhere in the vault, preferring the note's own folder and Markdown.
fn wiki_target(target: &str, from: &str, pages: &[String]) -> Option<String> {
    let (name, section) = target.split_once('#').map_or((target, None), |(n, s)| (n, Some(s)));
    let fragment = section.map_or(String::new(), |s| format!("#{}", slug(s)));
    if name.trim().is_empty() {
        return Some(fragment);
    }
    let best = wiki_page(name, from, pages)?;
    Some(format!("{}{fragment}", vault::url(best)))
}

/// The page that `[[name]]` (without `#Section`) in the note `from` points at.
pub(crate) fn wiki_page<'a>(name: &str, from: &str, pages: &'a [String]) -> Option<&'a String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let here = from.rsplit_once('/').map_or("", |(d, _)| d);
    // Unicode-aware, not ASCII: [[Árbol]] must find árbol.md. Composed and
    // decomposed accents are the same letters: [[árbol]] must find the NFD file.
    let same = |a: &str, b: &str| vault::nfc(a).to_lowercase() == vault::nfc(b).to_lowercase();
    let matches = |page: &&String| {
        let without_ext = page.rsplit_once('.').map_or(page.as_str(), |(p, _)| p);
        if name.contains('/') {
            same(page, name) || same(without_ext, name)
        } else {
            let file = page.rsplit('/').next().unwrap_or(page);
            let stem = without_ext.rsplit('/').next().unwrap_or(without_ext);
            same(file, name) || same(stem, name)
        }
    };
    pages.iter().filter(matches).min_by_key(|page| {
        let dir = page.rsplit_once('/').map_or("", |(d, _)| d);
        (dir != here, !page.ends_with(".md"), page.len())
    })
}

/// Heading anchor: lowercase words joined by dashes, accents kept.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push('-');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The front matter as a table of properties. YAML this reader does not
/// understand is shown as written, in a `<pre>`. Every value is escaped: the note's
/// own text must not inject HTML.
fn front_matter_html(yaml: &str, attrs: &str) -> String {
    match front_matter::parse(yaml) {
        Some(props) if props.is_empty() => String::new(),
        Some(props) => {
            let rows: String = props
                .iter()
                .map(|(key, value)| {
                    let cell = match value {
                        Value::Text(text) => escape(text).replace('\n', "<br>"),
                        Value::List(items) => {
                            let items: String = items.iter().map(|item| format!("<li>{}</li>", escape(item))).collect();
                            format!("<ul>{items}</ul>")
                        }
                    };
                    format!("<tr><th>{}</th><td>{cell}</td></tr>", escape(key))
                })
                .collect();
            format!("<table class=\"properties\"{attrs}><tbody>{rows}</tbody></table>")
        }
        None => format!("<pre class=\"properties\"{attrs}><code>{}</code></pre>", escape(yaml)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn wiki_links_and_images_found_by_name() {
        vault::set_base("http://127.0.0.1:1/s/".into());
        let root = std::env::temp_dir().join(format!("visor-embed-{}", std::process::id()));
        fs::create_dir_all(root.join("img")).unwrap();
        fs::write(root.join("img/foto.png"), "").unwrap();
        fs::write(root.join("img/nota.md"), "").unwrap();
        let html = page("![[foto.png]] ![[nota]] [[nota]] [[otra]]\n", "x.md", &root);
        assert!(html.contains("src=\"http://127.0.0.1:1/s/img/foto.png\""), "{html}");
        assert!(html.contains("href=\"http://127.0.0.1:1/s/img/nota.md\""), "{html}");
        assert!(html.contains("href=\"#visor-new:otra\""), "{html}");
        assert_eq!(embed_url("nota", "x.md", &root), None, "only images");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn slugs_keep_accents() {
        assert_eq!(slug("Ondas y frecuencia: ¿qué son?"), "ondas-y-frecuencia-qué-son");
    }

    #[test]
    fn blocks_carry_their_line() {
        let html = page("# Título\n\nUno\ndos\n\n- a\n- b\n\n```\n- [ ] no\n```\n", "a.md", Path::new("/nonexistent"));
        assert!(html.contains("<h1 data-line=\"1\" id=\"título\">"), "{html}");
        assert!(html.contains("<p data-line=\"3\" data-end=\"4\">Uno"), "{html}");
        assert!(html.contains("<ul data-line=\"6\" data-end=\"7\">"), "{html}");
        assert!(html.contains("<li data-line=\"7\">b"), "{html}");
        assert!(html.contains("<pre data-line=\"9\" data-end=\"11\"><code>- [ ] no"), "{html}");
        assert!(!html.contains('\0'));
    }

    #[test]
    fn tasks_toggle_by_offset() {
        let source = "- [ ] uno\n- [x] dos\n\n```\n- [ ] código\n```\n";
        let html = page(source, "a.md", Path::new("/nonexistent"));
        assert!(html.contains("<input type=\"checkbox\" data-task=\"2\">"), "{html}");
        assert!(html.contains("<input type=\"checkbox\" data-task=\"12\" checked>"), "{html}");
        assert_eq!(toggle_task(source, 2, true).unwrap(), source.replacen("[ ] uno", "[x] uno", 1));
        assert_eq!(toggle_task(source, 12, false).unwrap(), source.replacen("[x] dos", "[ ] dos", 1));
        let in_code = source.find("[ ] código").unwrap();
        assert!(toggle_task(source, in_code, true).is_none());
        assert!(toggle_task(source, 0, true).is_none());
        assert!(toggle_task(source, 999, true).is_none());
    }

    #[test]
    fn front_matter_is_a_table_and_keeps_lines() {
        let source = "---\nname: orquestar-issues\ndescription: Revisa los issues. Úsala cuando pidan trabajar issues.\n---\n\n# Título\n\nTexto\n";
        let html = page(source, "SKILL.md", Path::new("/nonexistent"));
        assert!(html.contains("<table class=\"properties\" data-line=\"1\" data-end=\"4\"><tbody>"), "{html}");
        assert!(html.contains("<th>name</th><td>orquestar-issues</td>"), "{html}");
        assert!(html.contains("<h1 data-line=\"6\" id=\"título\">"), "{html}");
        assert!(html.contains("<p data-line=\"8\">Texto"), "{html}");
    }

    #[test]
    fn front_matter_values_are_escaped() {
        let html = page("---\ntitle: <script>alert(1)</script>\n---\n", "a.md", Path::new("/nonexistent"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "{html}");
        assert!(!html.contains("<script>alert(1)"), "{html}");
    }

    #[test]
    fn malformed_front_matter_is_shown_raw() {
        let html = page("---\nname: x\nesto no es clave\n---\n\nTexto\n", "a.md", Path::new("/nonexistent"));
        assert!(
            html.contains("<pre class=\"properties\" data-line=\"1\" data-end=\"4\"><code>name: x\nesto no es clave"),
            "{html}"
        );
        assert!(html.contains("<p data-line=\"6\">Texto"), "{html}");
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
        let pages = vec!["árbol.md".to_string()];
        assert!(wiki_target("Árbol", "x.md", &pages).is_some(), "capitals with accents");
        let decomposed = vec!["pra\u{301}ctica.md".to_string()];
        assert!(wiki_target("práctica", "x.md", &decomposed).is_some(), "NFC name finds an NFD file");
        assert!(wiki_target("pra\u{301}ctica", "x.md", &decomposed).is_some(), "NFD name too");
    }
}
