//! Keeps the links of Markdown notes working after the sidebar moves a page
//! or folder, like Obsidian. HTML pages are left as they are.

use crate::vault::{self, markdown};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, LinkType, Parser, Tag, TagEnd};
use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// After `from` (file or folder, vault-relative) was moved to `to`, rewrites links in the vault's
/// Markdown notes so they still point at the same pages. Returns the notes it changed.
///
/// A note that can't be written doesn't stop the others; the error names it.
pub fn relink(root: &Path, from: &str, to: &str) -> Result<Vec<PathBuf>, String> {
    let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
    if from == to || from.is_empty() || to.is_empty() {
        return Ok(vec![]);
    }
    let new_pages = markdown::pages(root);
    let old_pages = new_pages.iter().map(|p| swap(p, to, from).unwrap_or_else(|| p.clone())).collect();
    let mut moved_names = HashSet::new();
    for page in new_pages.iter().filter(|p| swap(p, to, to).is_some()) {
        let file = vault::nfc(page.rsplit('/').next().unwrap_or(page)).to_lowercase();
        if let Some((stem, _)) = file.rsplit_once('.') {
            moved_names.insert(stem.to_string());
        }
        moved_names.insert(file);
    }
    let relinker = Relinker { from, to, old_pages, new_pages: new_pages.clone(), moved_names };

    let (mut changed, mut failed) = (vec![], vec![]);
    for note in new_pages.iter().filter(|p| vault::is_markdown(Path::new(p))) {
        let path = root.join(note);
        if vault::ignored(root, &path) {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else { continue };
        let old = swap(note, to, from).unwrap_or_else(|| note.clone());
        let Some(text) = relinker.rewrite(&source, &old, note) else { continue };
        match crate::vault::write_atomic(&path, &text) {
            Ok(()) => changed.push(path),
            Err(e) => failed.push(format!("{note} ({e})")),
        }
    }
    if failed.is_empty() {
        Ok(changed)
    } else {
        Err(format!("No se pudieron actualizar los enlaces de: {}", failed.join(", ")))
    }
}

struct Relinker<'a> {
    from: &'a str,
    to: &'a str,
    /// Every page before the move, in the same order as `new_pages`.
    old_pages: Vec<String>,
    new_pages: Vec<String>,
    /// Lowercase file names and stems of the moved pages: only `[[name]]`
    /// links to one of them can resolve differently in a note that stayed put.
    moved_names: HashSet<String>,
}

impl Relinker<'_> {
    /// The note's new text, or `None` if none of its links needs to change.
    /// `old` and `new` are where the note was and is.
    fn rewrite(&self, source: &str, old: &str, new: &str) -> Option<String> {
        let parser = Parser::new_ext(source, markdown::options());
        let mut edits: Vec<(Range<usize>, String)> = vec![];
        for (_, def) in parser.reference_definitions().iter() {
            if let Some(span) = definition_dest(source, def.span.clone())
                && let Some(link) = self.relative(&def.dest, &source[span.clone()], old, new)
            {
                edits.push((span, link));
            }
        }
        let events: Vec<(Event, Range<usize>)> = parser.into_offset_iter().collect();
        for (i, (event, range)) in events.iter().enumerate() {
            let (Event::Start(Tag::Link { link_type, dest_url, .. }) | Event::Start(Tag::Image { link_type, dest_url, .. })) =
                event
            else {
                continue;
            };
            match link_type {
                LinkType::Inline => {
                    let Some(span) = inline_dest(source, inner_end(&events, i), range.end) else { continue };
                    if let Some(link) = self.relative(dest_url, &source[span.clone()], old, new) {
                        edits.push((span, link));
                    }
                }
                LinkType::WikiLink { has_pothole } => {
                    let Some(open) = source[range.clone()].find("[[") else { continue };
                    let start = range.start + open + 2;
                    let name = dest_url.split('#').next().unwrap_or("");
                    if source.get(start..start + name.len()) != Some(name) {
                        continue;
                    }
                    let lead = name.len() - name.trim_start().len();
                    let span = start + lead..start + name.trim_end().len();
                    if let Some(link) = self.wiki(name.trim(), old, new) {
                        // Without `|text` the link shows what was written; keep showing it.
                        let close = source[start..range.end].find("]]").map(|i| start + i);
                        match close.filter(|_| !has_pothole) {
                            Some(close) => {
                                let inner = &source[start..close];
                                let section = &inner[name.len()..];
                                let shown = inner.trim().replacen('#', " › ", 1);
                                edits.push((start..close, format!("{link}{section}|{shown}")));
                            }
                            None => edits.push((span, link)),
                        }
                    }
                }
                _ => {}
            }
        }
        if edits.is_empty() {
            return None;
        }
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
        let mut text = source.to_string();
        for (span, link) in edits {
            text.replace_range(span, &link);
        }
        (text != source).then_some(text)
    }

    /// New destination for a relative Markdown link, in the style of `raw` (its source text),
    /// or `None` if it still points at the same file.
    fn relative(&self, dest: &str, raw: &str, old: &str, new: &str) -> Option<String> {
        if dest.is_empty() || dest.starts_with(['#', '/', '\\']) || has_scheme(dest) {
            return None;
        }
        let path = &dest[..dest.find(['#', '?']).unwrap_or(dest.len())];
        let path = percent_decode_str(path).decode_utf8().ok()?;
        let target = resolve(dir(old), &path)?;
        let target = swap(&target, self.from, self.to).unwrap_or(target);
        if resolve(dir(new), &path).as_ref() == Some(&target) {
            return None;
        }
        let mut link = relative_path(dir(new), &target);
        if path.ends_with('/') {
            link.push('/');
        }
        if path.starts_with("./") && !link.starts_with("../") {
            link.insert_str(0, "./");
        }

        let angle = raw.starts_with('<');
        let inner = if angle { raw.trim_start_matches('<').trim_end_matches('>') } else { raw };
        let suffix = inner.find(['#', '?']).map_or("", |i| &inner[i..]);
        Some(if angle {
            format!("<{link}{suffix}>")
        } else {
            format!("{}{suffix}", encode(&link, encodes_non_ascii(inner)))
        })
    }

    /// New name for `[[name]]` if, after the move, it would resolve to another page
    /// (or to none): the shortest that resolves to where it pointed before.
    fn wiki(&self, name: &str, old: &str, new: &str) -> Option<String> {
        let last = vault::nfc(name.rsplit('/').next().unwrap_or(name)).to_lowercase();
        if old == new && !self.moved_names.contains(&last) {
            return None;
        }
        let before = markdown::wiki_page(name, old, &self.old_pages)?;
        let target = swap(before, self.from, self.to).unwrap_or_else(|| before.clone());
        if markdown::wiki_page(name, new, &self.new_pages) == Some(&target) {
            return None;
        }
        let file = target.rsplit('/').next().unwrap_or(&target);
        let with_ext = vault::nfc(file).to_lowercase() == last;
        let strip = |p: &str| p.rsplit_once('.').map_or(p.to_string(), |(s, _)| s.to_string());
        let mut candidates = vec![];
        if !name.contains('/') {
            candidates.push(if with_ext { file.to_string() } else { strip(file) });
        }
        candidates.push(if with_ext { target.clone() } else { strip(&target) });
        candidates.push(target.clone());
        candidates.into_iter().find(|c| markdown::wiki_page(c, new, &self.new_pages) == Some(&target))
    }
}

/// `path` with its prefix `from` (the path itself or a folder above it) replaced by `to`.
fn swap(path: &str, from: &str, to: &str) -> Option<String> {
    let rest = path.strip_prefix(from)?;
    (rest.is_empty() || rest.starts_with('/')).then(|| format!("{to}{rest}"))
}

fn dir(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(d, _)| d)
}

/// `path` relative to the folder `dir`, as a vault path; `None` if it leaves the vault.
fn resolve(dir: &str, path: &str) -> Option<String> {
    let mut segments: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(seg),
        }
    }
    Some(segments.join("/"))
}

fn relative_path(dir: &str, target: &str) -> String {
    let dir: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let target: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    let common = dir.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts = vec![".."; dir.len() - common];
    parts.extend(&target[common..]);
    if parts.is_empty() { ".".into() } else { parts.join("/") }
}

fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.len() > 1
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// True if the link spelled accented letters as `%C3%A9` rather than `é`.
fn encodes_non_ascii(raw: &str) -> bool {
    raw.as_bytes().windows(3).any(|w| w[0] == b'%' && matches!(w[1], b'8'..=b'9' | b'a'..=b'f' | b'A'..=b'F') && w[2].is_ascii_hexdigit())
}

/// Percent-encodes what a bare Markdown link destination can't hold.
fn encode(path: &str, non_ascii: bool) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        let escape = if c.is_ascii() { c.is_ascii_control() || " %()<>#?\\".contains(c) } else { non_ascii };
        if escape {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// End of the text of the link or image that starts at `events[i]`.
fn inner_end(events: &[(Event, Range<usize>)], i: usize) -> usize {
    let mut end = events[i].1.start + 1;
    let mut depth = 0;
    for (event, range) in &events[i + 1..] {
        match event {
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => depth += 1,
            Event::End(TagEnd::Link | TagEnd::Image) if depth == 0 => break,
            Event::End(TagEnd::Link | TagEnd::Image) => depth -= 1,
            _ => {}
        }
        end = end.max(range.end);
    }
    end
}

/// Source span of the destination of `[text](dest "title")`, given where its text ends.
fn inline_dest(source: &str, text_end: usize, end: usize) -> Option<Range<usize>> {
    let open = text_end + source.get(text_end..end)?.find("](")? + 2;
    destination(source, open, end)
}

/// Source span of the destination of `[label]: dest "title"`.
fn definition_dest(source: &str, span: Range<usize>) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let mut i = span.start;
    while i + 1 < span.end {
        match bytes[i] {
            b'\\' => i += 2,
            b']' if bytes[i + 1] == b':' => return destination(source, i + 2, span.end),
            _ => i += 1,
        }
    }
    None
}

/// The destination token starting at `pos` (after optional blanks): `<...>` or a bare URL.
fn destination(source: &str, pos: usize, end: usize) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let mut i = pos;
    while i < end && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let start = i;
    if bytes.get(i) == Some(&b'<') {
        i += 1;
        while i < end {
            match bytes[i] {
                b'\\' => i += 2,
                b'>' => return Some(start..i + 1),
                b'\n' | b'<' => return None,
                _ => i += 1,
            }
        }
        return None;
    }
    let mut depth = 0;
    while i < end {
        match bytes[i] {
            b'\\' => i += 2,
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' if depth == 0 => break,
            b')' => {
                depth -= 1;
                i += 1;
            }
            b if b.is_ascii_whitespace() => break,
            _ => i += 1,
        }
    }
    let i = i.min(end);
    (i > start).then_some(start..i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::files::move_to;

    fn vault(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("visor-links-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (rel, text) in files {
            let path = root.join(rel);
            if rel.ends_with('/') {
                fs::create_dir_all(path).unwrap();
            } else {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
        }
        root.canonicalize().unwrap()
    }

    fn moved(root: &Path, rel: &str, dir: &str) -> Vec<String> {
        let to = move_to(&vault::Vault::open(root), rel, dir).unwrap().unwrap();
        let mut changed: Vec<String> = relink(root, rel, &to).unwrap().iter().map(|p| vault::rel(root, p)).collect();
        changed.sort();
        changed
    }

    fn read(root: &Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap()
    }

    #[test]
    fn links_follow_a_moved_note() {
        let index = "[a](tema/nota.md#seccion) [b](<tema/nota.md>) [c](tema/mi%20nota.md) [r][ref]\n\n[ref]: tema/nota.md \"Nota\"\n";
        let root = vault(
            "note",
            &[
                ("indice.md", index),
                ("tema/nota.md", "# Nota"),
                ("tema/mi nota.md", ""),
                ("tema/otra.md", "[d](nota.md) [e](./mi%20nota.md) [f](otra.md)"),
                ("archivo/", ""),
                (".visor/x.md", "[a](../tema/nota.md)"),
            ],
        );
        fs::write(root.join("roto.md"), b"[a](tema/nota.md) \xff").unwrap();

        assert_eq!(moved(&root, "tema/nota.md", "archivo"), ["indice.md", "tema/otra.md"]);
        assert_eq!(
            read(&root, "indice.md"),
            "[a](archivo/nota.md#seccion) [b](<archivo/nota.md>) [c](tema/mi%20nota.md) [r][ref]\n\n[ref]: archivo/nota.md \"Nota\"\n"
        );
        assert_eq!(read(&root, "tema/otra.md"), "[d](../archivo/nota.md) [e](./mi%20nota.md) [f](otra.md)");

        assert_eq!(moved(&root, "tema/mi nota.md", "archivo"), ["indice.md", "tema/otra.md"]);
        assert!(read(&root, "indice.md").contains("[c](archivo/mi%20nota.md)"));
        assert_eq!(read(&root, "tema/otra.md"), "[d](../archivo/nota.md) [e](../archivo/mi%20nota.md) [f](otra.md)");
        assert_eq!(read(&root, ".visor/x.md"), "[a](../tema/nota.md)");
        assert_eq!(fs::read(root.join("roto.md")).unwrap(), b"[a](tema/nota.md) \xff");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wikilinks_follow_a_moved_note() {
        let root = vault(
            "wiki",
            &[
                ("a/x.md", "[[tema/nota]] [[nota#Sección|alias]] [[ tema/nota.md |Nota]] [[otra]]"),
                ("tema/nota.md", ""),
                ("tema/otra.md", ""),
                ("archivo/", ""),
            ],
        );
        assert_eq!(moved(&root, "tema/nota.md", "archivo"), ["a/x.md"]);
        assert_eq!(read(&root, "a/x.md"), "[[archivo/nota|tema/nota]] [[nota#Sección|alias]] [[ archivo/nota.md |Nota]] [[otra]]");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ambiguous_wikilink_gets_a_path() {
        let root = vault("ambiguous", &[("a/x.md", "[[ondas]] y [[Ondas#Fase]]"), ("a/ondas.md", ""), ("b/ondas.md", ""), ("zzz/", "")]);
        assert_eq!(moved(&root, "a/ondas.md", "zzz"), ["a/x.md"]);
        assert_eq!(read(&root, "a/x.md"), "[[zzz/ondas|ondas]] y [[zzz/ondas#Fase|Ondas › Fase]]");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn links_out_of_a_moved_note() {
        let note = "[o](otra.md) [i](../indice.md#top) ![img](img/f.png) [[otra]] [w](https://e.com/otra.md) \
[m](mailto:a@b.c) `[o](otra.md)`\n\n```\n[o](otra.md)\n```\n\n    [o](otra.md)\n";
        let root = vault(
            "out",
            &[
                ("indice.md", ""),
                ("otra.md", ""),
                ("tema/nota.md", note),
                ("tema/otra.md", ""),
                ("tema/img/f.png", ""),
                ("archivo/2026/", ""),
            ],
        );
        assert_eq!(moved(&root, "tema/nota.md", "archivo/2026"), ["archivo/2026/nota.md"]);
        let expected = note
            .replacen("[o](otra.md)", "[o](../../tema/otra.md)", 1)
            .replace("../indice.md", "../../indice.md")
            .replace("img/f.png", "../../tema/img/f.png")
            .replace("[[otra]]", "[[tema/otra|otra]]");
        assert_eq!(read(&root, "archivo/2026/nota.md"), expected);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moved_folder_keeps_inner_links() {
        let root = vault(
            "folder",
            &[
                ("indice.md", "[a](tema/a.md) [[tema/b]] [[a]]"),
                ("suelta.md", "[s](otra/cosa.md)"),
                ("tema/a.md", "[b](b.md) [[b]] [i](../indice.md)"),
                ("tema/b.md", ""),
                ("archivo/", ""),
            ],
        );
        assert_eq!(moved(&root, "tema", "archivo"), ["archivo/tema/a.md", "indice.md"]);
        assert_eq!(read(&root, "indice.md"), "[a](archivo/tema/a.md) [[archivo/tema/b|tema/b]] [[a]]");
        assert_eq!(read(&root, "archivo/tema/a.md"), "[b](b.md) [[b]] [i](../../indice.md)");
        assert_eq!(read(&root, "suelta.md"), "[s](otra/cosa.md)");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encoding_keeps_its_style() {
        assert_eq!(encode("a b/é(1).md", false), "a%20b/é%281%29.md");
        assert_eq!(encode("é.md", true), "%C3%A9.md");
        assert!(encodes_non_ascii("caf%C3%A9.md") && !encodes_non_ascii("a%20b.md"));
        assert_eq!(relative_path("a/b", "a/c/d.md"), "../c/d.md");
        assert_eq!(resolve("a", "../../x.md"), None);
        assert!(has_scheme("https://x") && has_scheme("mailto:a") && !has_scheme("nota.md") && !has_scheme("C:/x"));
    }
}
