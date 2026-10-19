//! What the address field does with the text typed in it. No GTK and no disk:
//! the caller resolves the vault path and passes it in.

use crate::vault::is_web;
use crate::vault;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Endings that make a text a file name. `.md` is also Moldova's domain, but
/// a typed `ondas.md` is far more likely a note that is not in the vault.
const FILE_ENDINGS: &[&str] = &[
    "md", "html", "htm", "pdf", "txt", "json", "csv", "js", "css", "png", "jpg", "jpeg", "gif", "svg", "webp", "mp3", "mp4", "wav",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Plan<'a> {
    /// An absolute path of a page outside every vault.
    Loose,
    /// A web address, as typed.
    Web,
    /// A vault path, already resolved by the caller.
    Vault(&'a str),
    /// A bare domain; the caller adds `https://`.
    Domain,
    /// Text for the search engine, as it should be sent.
    Search(&'a str),
    /// Looks like a path that does not exist: shown as an error, not searched.
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    DuckDuckGo,
    Google,
}

impl Engine {
    /// In the order the settings list them; the first is the default.
    pub const ALL: [Engine; 2] = [Engine::DuckDuckGo, Engine::Google];

    /// The name stored in `config.json`.
    pub fn id(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "duckduckgo",
            Engine::Google => "google",
        }
    }

    // Phase 1, area E: the settings list the engines.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "DuckDuckGo",
            Engine::Google => "Google",
        }
    }

    /// An unknown or missing name gives the default, so a config written by
    /// another version still loads.
    pub fn from_id(id: Option<&str>) -> Engine {
        Engine::ALL.into_iter().find(|e| Some(e.id()) == id).unwrap_or(Engine::ALL[0])
    }
}

/// `text` is trimmed and not empty; `in_vault` is the vault path it resolves to, if any.
pub fn plan<'a>(text: &'a str, in_vault: Option<&'a str>) -> Plan<'a> {
    // A leading «?» means «search this», whatever else the text could be.
    if let Some(rest) = text.strip_prefix('?') {
        let query = rest.trim();
        return if query.is_empty() { Plan::Error } else { Plan::Search(query) };
    }
    if vault::is_loose(text) {
        return Plan::Loose;
    }
    if is_web(text) {
        return Plan::Web;
    }
    if let Some(rel) = in_vault {
        return Plan::Vault(rel);
    }
    if is_domain(text) {
        return Plan::Domain;
    }
    if looks_like_path(text) { Plan::Error } else { Plan::Search(text) }
}

/// A bare domain: one word with a dot. A file name such as `ondas.md` is not one.
fn is_domain(text: &str) -> bool {
    !text.contains(char::is_whitespace) && text.contains('.') && !has_file_ending(text)
}

/// Text with a «/» or a file ending is a path that failed to resolve; a search would send it out by mistake.
fn looks_like_path(text: &str) -> bool {
    text.contains('/') || has_file_ending(text)
}

fn has_file_ending(text: &str) -> bool {
    text.rsplit_once('.')
        .is_some_and(|(_, ending)| FILE_ENDINGS.contains(&ending.to_ascii_lowercase().as_str()))
}

/// The address that searches `query`. The query is percent-encoded so that
/// `&`, `#` or `?` in it cannot change the parameters.
pub fn search_url(engine: Engine, query: &str) -> String {
    const QUERY: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
    let query = utf8_percent_encode(query, QUERY);
    match engine {
        Engine::DuckDuckGo => format!("https://duckduckgo.com/?q={query}"),
        Engine::Google => format!("https://www.google.com/search?q={query}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_path_resolved_by_the_caller_wins() {
        assert_eq!(plan("estudio/ondas.md", Some("estudio/ondas.md")), Plan::Vault("estudio/ondas.md"));
    }

    #[test]
    fn absolute_path_is_loose() {
        assert_eq!(plan("/home/ruben/paginas/a.html", None), Plan::Loose);
    }

    #[test]
    fn web_address_is_web() {
        assert_eq!(plan("https://example.com/a?b=c", None), Plan::Web);
        assert_eq!(plan("http://localhost:8000", None), Plan::Web);
    }

    #[test]
    fn bare_domain_is_domain() {
        assert_eq!(plan("example.com", None), Plan::Domain);
        assert_eq!(plan("example.com/ruta", None), Plan::Domain);
    }

    #[test]
    fn text_with_spaces_is_searched() {
        assert_eq!(plan("fisica ondas sonoras", None), Plan::Search("fisica ondas sonoras"));
        assert_eq!(plan("pi 3.14", None), Plan::Search("pi 3.14"));
    }

    #[test]
    fn single_word_without_dot_is_searched() {
        assert_eq!(plan("ondas", None), Plan::Search("ondas"));
    }

    #[test]
    fn question_mark_forces_search_and_skips_the_rest() {
        assert_eq!(plan("? ondas", None), Plan::Search("ondas"));
        assert_eq!(plan("?ondas", None), Plan::Search("ondas"));
        assert_eq!(plan("?estudio/ondas.md", Some("estudio/ondas.md")), Plan::Search("estudio/ondas.md"));
        assert_eq!(plan("?https://example.com", None), Plan::Search("https://example.com"));
        assert_eq!(plan("?", None), Plan::Error);
        assert_eq!(plan("? ", None), Plan::Error);
    }

    #[test]
    fn misspelled_vault_path_is_an_error_not_a_search() {
        assert_eq!(plan("estudio/fisica/ondas", None), Plan::Error);
    }

    #[test]
    fn file_name_with_extension_is_an_error() {
        assert_eq!(plan("ondas.md", None), Plan::Error);
        assert_eq!(plan("ONDAS.PDF", None), Plan::Error);
        assert_eq!(plan("ondas.excalidraw.md", None), Plan::Error);
    }

    #[test]
    fn engine_names_round_trip_and_unknown_is_the_default() {
        for engine in Engine::ALL {
            assert_eq!(Engine::from_id(Some(engine.id())), engine);
        }
        assert_eq!(Engine::from_id(None), Engine::DuckDuckGo);
        assert_eq!(Engine::from_id(Some("bing")), Engine::DuckDuckGo);
    }

    #[test]
    fn search_url_encodes_special_characters() {
        assert_eq!(
            search_url(Engine::DuckDuckGo, "á & ?"),
            "https://duckduckgo.com/?q=%C3%A1%20%26%20%3F"
        );
        assert_eq!(
            search_url(Engine::Google, "á & ?"),
            "https://www.google.com/search?q=%C3%A1%20%26%20%3F"
        );
        assert_eq!(search_url(Engine::Google, "a-b_c.d~e"), "https://www.google.com/search?q=a-b_c.d~e");
        assert_eq!(search_url(Engine::DuckDuckGo, "x#y"), "https://duckduckgo.com/?q=x%23y");
    }
}
