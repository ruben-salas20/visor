//! Turns a Markdown note into a full page: themed, with math, code highlighting,
//! Pele diagrams (from ```mermaid blocks) and `[[wiki]]` links, plus the editor behind Ctrl+E.

use crate::trf;
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

/// Where a note's links may lead. A loose note reaches only its own folder, on
/// the loose origin; links to missing notes are not offered for creation there.
#[derive(Clone, Copy)]
enum Scope<'a> {
    Vault,
    /// The loose folder, as an absolute path.
    Loose(&'a str),
}

impl Scope<'_> {
    /// URL of the file at `rel`, which is relative to the vault or to the loose folder.
    fn url(self, rel: &str) -> String {
        match self {
            Scope::Vault => vault::url(rel),
            Scope::Loose(folder) => vault::url(&format!("{folder}/{rel}")),
        }
    }
}

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
    render(source, rel, root, Scope::Vault)
}

/// Renders a note opened on its own. `rel` is its path inside `folder`, and only
/// that folder and its subfolders, without hidden entries, supply its links.
pub fn loose_page(source: &str, rel: &str, folder: &Path) -> String {
    let base = folder.to_string_lossy();
    render(source, rel, folder, Scope::Loose(&base))
}

fn render(source: &str, rel: &str, root: &Path, scope: Scope) -> String {
    let lines = !source.contains(LINE_MARK);
    let line_starts: Vec<usize> = std::iter::once(0).chain(source.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset);
    let mut events: Vec<Event> = Vec::new();
    // The front matter's range and its text, between its start and end events.
    let mut front: Option<(std::ops::Range<usize>, String)> = None;
    let mut note_lang = None;
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
                    note_lang = declared_lang(&yaml);
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
        PdfLink,
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
            Event::Start(Tag::Link { link_type: LinkType::Inline, dest_url, .. }) if is_local_pdf_link(dest_url) => {
                Some(Fix::PdfLink)
            }
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
                    match wiki_target(dest_url, rel, index.links(), scope) {
                        Some(url) => *dest_url = url.into(),
                        // A missing PDF gets no «Crear la nota»: visor cannot create a PDF.
                        None if names_pdf(dest_url) => {
                            *title = CowStr::from(trf!("No existe el PDF «{}»", "The PDF «{}» does not exist", dest_url));
                            *dest_url = "#".into();
                        }
                        None if matches!(scope, Scope::Loose(_)) => {
                            *title = CowStr::from(trf!("No existe la nota «{}»", "The note «{}» does not exist", dest_url));
                            *dest_url = "#".into();
                        }
                        None => {
                            *title = CowStr::from(trf!("Crear la nota «{}»", "Create the note «{}»", dest_url));
                            let new = format!("#visor-new:{}", utf8_percent_encode(dest_url, NON_ALPHANUMERIC));
                            *dest_url = new.into();
                        }
                    }
                }
            }
            Some(Fix::PdfLink) => {
                if let Event::Start(Tag::Link { dest_url, .. }) = &mut events[i] {
                    let dest = percent_encoding::percent_decode_str(dest_url).decode_utf8_lossy();
                    // Left as written when no file matches: the link then fails as any broken link does.
                    if let Some(url) = wiki_target(&dest, rel, index.links(), scope) {
                        *dest_url = url.into();
                    }
                }
            }
            Some(Fix::Embed) => {
                if let Event::Start(Tag::Image { dest_url, .. }) = &mut events[i]
                    && let Some(url) = embed_in(dest_url, rel, &index, scope)
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

    let origin = match scope {
        Scope::Vault => vault::base(),
        Scope::Loose(_) => vault::loose_base(),
    };
    let assets = format!("{origin}{ASSETS}");
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
        head += &format!("<script defer src=\"{assets}pele/pele.global.js\"></script>");
    }
    let stem = Path::new(rel).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let lang = crate::i18n::lang().code();
    let script = SCRIPT.replace("__VISOR_LANG__", lang);
    let config = serde_json::json!({ "math": math, "code": code, "mermaid": mermaid, "dir": dir, "assets": assets, "lang": lang });
    // The interface language is a guess for the text; the note's own `lang` is not.
    // It drives spell checking, hyphenation and screen readers.
    let doc_lang = note_lang.as_deref().unwrap_or(lang);
    format!(
        "<!doctype html><html lang=\"{doc_lang}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title><style>{STYLE}</style>{head}</head><body>\
<main id=\"note\">{body}</main>\
<div id=\"edit\" hidden data-visor-skip><div id=\"status\" role=\"status\"></div>\
<div id=\"editor\"></div></div>\
<script>window.__visorNoteConfig = {config};\n{script}</script></body></html>",
        title = escape(&title.unwrap_or(stem)),
    )
}

/// The `lang` key of the front matter (as in Pandoc), if it looks like a language
/// tag. Only letters, digits and hyphens pass, so it goes into the attribute unescaped.
fn declared_lang(yaml: &str) -> Option<String> {
    front_matter::parse(yaml)?
        .into_iter()
        .find_map(|(key, value)| match value {
            Value::Text(text) if key == "lang" => Some(text),
            _ => None,
        })
        .filter(|tag| tag.len() <= 35 && tag.split('-').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric())))
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

/// The pages, images and PDFs of a vault, found by one walk the first time any
/// is asked for and shared by every link of a note.
struct Index<'a> {
    root: &'a Path,
    found: OnceCell<Found>,
}

struct Found {
    pages: Vec<String>,
    images: Vec<String>,
    /// What `[[links]]` may name: the pages, then the PDFs.
    links: Vec<String>,
}

impl<'a> Index<'a> {
    fn new(root: &'a Path) -> Self {
        Index { root, found: OnceCell::new() }
    }

    fn found(&self) -> &Found {
        self.found.get_or_init(|| {
            let all = vault::files(self.root, self.root, |p| vault::is_page(p) || is_image(p) || vault::is_pdf(p));
            let rel = |paths: &[&PathBuf]| -> Vec<String> { paths.iter().map(|p| vault::rel(self.root, p)).collect() };
            let (pages, rest): (Vec<&PathBuf>, Vec<&PathBuf>) = all.iter().partition(|p| vault::is_page(p));
            let (pdfs, images): (Vec<&PathBuf>, Vec<&PathBuf>) = rest.into_iter().partition(|p| vault::is_pdf(p));
            let pages = rel(&pages);
            let mut links = pages.clone();
            links.extend(rel(&pdfs));
            Found { pages, images: rel(&images), links }
        })
    }

    fn pages(&self) -> &[String] {
        &self.found().pages
    }

    fn images(&self) -> &[String] {
        &self.found().images
    }

    fn links(&self) -> &[String] {
        &self.found().links
    }
}

fn is_image(path: &Path) -> bool {
    vault::mime(path).starts_with("image/")
}

/// A link target that names a PDF, with any `#fragment` left out.
fn names_pdf(target: &str) -> bool {
    vault::is_pdf(Path::new(target.split('#').next().unwrap_or(target).trim()))
}

/// A `[text](file.pdf)` link to a file of the vault itself, not to a web address or a `/` path.
fn is_local_pdf_link(dest: &str) -> bool {
    !dest.contains("://") && !dest.starts_with(['/', '#']) && names_pdf(dest)
}

/// URL of the image that `![[name]]` in the note at `rel` shows: found by name
/// anywhere in the vault, like `[[links]]`, preferring the note's own folder.
pub fn embed_url(name: &str, rel: &str, root: &Path) -> Option<String> {
    embed_in(name, rel, &Index::new(root), Scope::Vault)
}

fn embed_in(name: &str, rel: &str, index: &Index, scope: Scope) -> Option<String> {
    let name = name.split('|').next().unwrap_or(name);
    if !is_image(Path::new(name.trim())) {
        return None;
    }
    wiki_page(name, rel, index.images()).map(|p| scope.url(p))
}

/// URL of the `[[target]]` written in the note at `rel`, for the editor, which
/// sees links as they are typed. `None` if no page matches.
pub fn wiki_url(target: &str, rel: &str, root: &Path) -> Option<String> {
    wiki_target(target, rel, Index::new(root).links(), Scope::Vault)
}

/// [`wiki_url`] for a loose note: `rel` is its path inside `folder`, the only folder searched.
pub fn loose_wiki_url(target: &str, rel: &str, folder: &Path) -> Option<String> {
    let base = folder.to_string_lossy();
    wiki_target(target, rel, Index::new(folder).links(), Scope::Loose(&base))
}

/// [`embed_url`] for a loose note, searching only `folder`.
pub fn loose_embed_url(name: &str, rel: &str, folder: &Path) -> Option<String> {
    let base = folder.to_string_lossy();
    embed_in(name, rel, &Index::new(folder), Scope::Loose(&base))
}

/// Resolves `[[name]]`, `[[folder/name]]` or `[[name#Section]]` like Obsidian:
/// by file name anywhere in the vault, preferring the note's own folder and Markdown.
fn wiki_target(target: &str, from: &str, pages: &[String], scope: Scope) -> Option<String> {
    let (name, section) = target.split_once('#').map_or((target, None), |(n, s)| (n, Some(s)));
    let fragment = section.map_or(String::new(), |s| format!("#{}", slug(s)));
    if name.trim().is_empty() {
        return Some(fragment);
    }
    let best = wiki_page(name, from, pages)?;
    Some(format!("{}{fragment}", scope.url(best)))
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
    fn without_ext(page: &str) -> &str {
        page.rsplit_once('.').map_or(page, |(p, _)| p)
    }
    // A path may be the whole path from the root or any end of it, cut at a
    // folder boundary: `fases/x` ends `NN/fases/x.md`, not `NN/subfases/x.md`.
    let ends_at_folder = |path: &str| path.match_indices('/').any(|(i, _)| same(&path[i + 1..], name));
    let exact = |page: &str| same(page, name) || same(without_ext(page), name);
    // `[[x]]` never finds `x.pdf`, and `[[x.pdf]]` finds nothing but PDFs.
    let want_pdf = names_pdf(name);
    let matches = |page: &&String| {
        if vault::is_pdf(Path::new(page.as_str())) != want_pdf {
            false
        } else if name.contains('/') {
            exact(page) || ends_at_folder(page) || ends_at_folder(without_ext(page))
        } else {
            let file = page.rsplit('/').next().unwrap_or(page);
            let stem = without_ext(page).rsplit('/').next().unwrap_or(without_ext(page));
            same(file, name) || same(stem, name)
        }
    };
    pages.iter().filter(matches).min_by_key(|page| {
        let dir = page.rsplit_once('/').map_or("", |(d, _)| d);
        (name.contains('/') && !exact(page), dir != here, !page.ends_with(".md"), page.len())
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
    fn wiki_url_finds_a_pdf() {
        vault::set_base("http://127.0.0.1:1/s/".into());
        let root = std::env::temp_dir().join(format!("visor-pdf-link-{}", std::process::id()));
        fs::create_dir_all(root.join("libros")).unwrap();
        fs::write(root.join("libros/archivo.pdf"), "").unwrap();
        fs::write(root.join("nota.md"), "").unwrap();
        assert_eq!(wiki_url("archivo.pdf", "nota.md", &root).unwrap(), "http://127.0.0.1:1/s/libros/archivo.pdf");
        assert_eq!(wiki_url("libros/archivo.pdf", "nota.md", &root).unwrap(), "http://127.0.0.1:1/s/libros/archivo.pdf");
        let html = page("[[archivo.pdf]] [texto](archivo.pdf) [otro](libros/archivo.pdf)\n", "nota.md", &root);
        assert_eq!(html.matches("href=\"http://127.0.0.1:1/s/libros/archivo.pdf\"").count(), 3, "{html}");
        assert!(!html.contains("href=\"#visor-new"), "{html}");
        assert_eq!(embed_url("archivo.pdf", "nota.md", &root), None, "a PDF is not embedded");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_pdf_does_not_resolve() {
        vault::set_base("http://127.0.0.1:1/s/".into());
        let root = std::env::temp_dir().join(format!("visor-pdf-missing-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("archivo.pdf"), "").unwrap();
        assert_eq!(wiki_url("falta.pdf", "nota.md", &root), None);
        assert_eq!(wiki_url("archivo", "nota.md", &root), None, "[[name]] does not name a PDF");
        let html = page("[[falta.pdf]] [t](falta.pdf)\n", "nota.md", &root);
        assert!(!html.contains("href=\"#visor-new"), "no «Crear la nota» for a PDF: {html}");
        assert!(html.contains("href=\"#\""), "{html}");
        assert!(html.contains("href=\"falta.pdf\""), "a plain link stays as written: {html}");
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
    fn front_matter_lang_sets_the_document_language() {
        let page_lang = |source: &str| {
            let html = page(source, "a.md", Path::new("/nonexistent"));
            html.split("<html lang=\"").nth(1).and_then(|r| r.split('"').next()).unwrap().to_string()
        };
        assert_eq!(page_lang("---\nlang: en-GB\n---\n\nText\n"), "en-GB");
        assert_eq!(page_lang("---\nlang: \"fr\"\n---\n\nTexte\n"), "fr");
        // Without a valid tag, the interface language (Spanish in tests).
        assert_eq!(page_lang("Texto\n"), "es");
        assert_eq!(page_lang("---\nlang: en\" onload=\"x\n---\n"), "es");
        assert_eq!(page_lang("---\nlang: [en, es]\n---\n"), "es");
        assert_eq!(page_lang("---\nlang:\n---\n"), "es");
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
        assert_eq!(wiki_target("ondas", "fisica/x.md", &pages, Scope::Vault).unwrap(), "http://127.0.0.1:1/s/fisica/ondas.md");
        assert_eq!(wiki_target("ondas", "historia/y.md", &pages, Scope::Vault).unwrap(), "http://127.0.0.1:1/s/historia/ondas.md");
        assert_eq!(wiki_target("Roma#El Senado", "a.md", &pages, Scope::Vault).unwrap(), "http://127.0.0.1:1/s/roma.html#el-senado");
        assert_eq!(wiki_target("historia/ondas", "a.md", &pages, Scope::Vault).unwrap(), "http://127.0.0.1:1/s/historia/ondas.md");
        assert!(wiki_target("no existe", "a.md", &pages, Scope::Vault).is_none());
        let pages = vec!["árbol.md".to_string()];
        assert!(wiki_target("Árbol", "x.md", &pages, Scope::Vault).is_some(), "capitals with accents");
        let decomposed = vec!["pra\u{301}ctica.md".to_string()];
        assert!(wiki_target("práctica", "x.md", &decomposed, Scope::Vault).is_some(), "NFC name finds an NFD file");
        assert!(wiki_target("pra\u{301}ctica", "x.md", &decomposed, Scope::Vault).is_some(), "NFD name too");
    }

    #[test]
    fn wiki_path_may_start_at_any_folder_boundary() {
        let pages: Vec<String> = ["NN/fases/1 — Repaso de derivadas.md", "NN/subfases/x.md"].map(String::from).into();
        let found = wiki_page("fases/1 — Repaso de derivadas", "NN/PLAN.md", &pages);
        assert_eq!(found.map(String::as_str), Some("NN/fases/1 — Repaso de derivadas.md"));
        assert!(wiki_page("fases/x", "NN/PLAN.md", &pages).is_none(), "cut only after a «/»");
        assert!(wiki_page("fases/x.md", "NN/PLAN.md", &pages).is_none());

        let pages: Vec<String> = ["fases/x.md", "NN/fases/x.md"].map(String::from).into();
        let found = wiki_page("fases/x", "NN/PLAN.md", &pages);
        assert_eq!(found.map(String::as_str), Some("fases/x.md"), "the exact path from the root wins");

        // Suffix in the note's own folder must still lose to the exact path.
        let pages: Vec<String> = ["f/x.md", "NN/f/x.md"].map(String::from).into();
        let found = wiki_page("f/x", "NN/f/PLAN.md", &pages);
        assert_eq!(found.map(String::as_str), Some("f/x.md"));
    }

    #[test]
    fn loose_notes_reach_only_their_folder() {
        vault::set_base("http://127.0.0.1:1/s/".into());
        vault::set_loose_base("http://127.0.0.2:1/s/".into());
        let dir = std::env::temp_dir().join(format!("visor-loose-links-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for sub in ["nota/img", "nota/sub", "nota/.oculta"] {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
        for file in ["nota/x.md", "nota/img/foto.png", "nota/sub/otra.md", "nota/.oculta/secreto.md", "afuera.md"] {
            fs::write(dir.join(file), "").unwrap();
        }
        let folder = dir.canonicalize().unwrap().join("nota");
        let source = "![[foto.png]] [[otra]] [[secreto]] [[afuera]] [[nueva]]\n\n- [ ] tarea\n";
        let html = loose_page(source, "x.md", &folder);
        let folder_text = folder.to_string_lossy();
        assert!(html.contains(&vault::url(&format!("{folder_text}/img/foto.png"))), "image by name: {html}");
        assert!(html.contains(&vault::url(&format!("{folder_text}/sub/otra.md"))), "subfolders: {html}");
        assert!(!html.contains("secreto.md"), "hidden folder: {html}");
        assert!(!html.contains("afuera.md"), "outside the folder: {html}");
        assert!(!html.contains("href=\"#visor-new:"), "no creation link from a loose note: {html}");
        assert!(html.contains("href=\"#\""), "a missing note goes nowhere: {html}");
        assert!(!html.contains("127.0.0.1"), "never on the vault origin: {html}");
        assert!(html.contains("data-task=\"") && !html.contains(" disabled"), "checkboxes can be ticked: {html}");
        assert_eq!(loose_wiki_url("otra", "x.md", &folder), Some(vault::url(&format!("{folder_text}/sub/otra.md"))));
        assert_eq!(loose_wiki_url("afuera", "x.md", &folder), None, "the editor's links stay in the folder too");
        assert_eq!(loose_embed_url("foto.png", "x.md", &folder), Some(vault::url(&format!("{folder_text}/img/foto.png"))));
        fs::remove_dir_all(dir).unwrap();
    }
}
