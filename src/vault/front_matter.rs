//! Reads the YAML front matter of a note: the first level of keys, which is all
//! that notes and agent skills use. No YAML library: deeper levels stay as text,
//! and any line this reader does not understand makes the block malformed, so
//! the caller shows it as written instead of guessing.

#[derive(Debug, PartialEq)]
pub(crate) enum Value {
    Text(String),
    List(Vec<String>),
}

/// The properties in file order, or `None` if a first-level line is not `clave: valor`.
pub(crate) fn parse(yaml: &str) -> Option<Vec<(String, Value)>> {
    let lines: Vec<&str> = yaml.lines().collect();
    let mut props = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        // Continuation lines are consumed by the key above them, so one reaching
        // here has no key to belong to.
        if line.starts_with([' ', '\t']) || strip_item(line).is_some() {
            return None;
        }
        let (key, rest) = line.split_once(':')?;
        let key = key.trim_end();
        if key.is_empty() || !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
            return None;
        }
        let start = i;
        while i < lines.len() && is_continuation(lines[i]) {
            i += 1;
        }
        props.push((key.to_string(), value(rest.trim(), &lines[start..i])?));
    }
    Some(props)
}

/// Blank lines, indented lines and list items at column 0 belong to the key above.
/// A comment at column 0 ends the key's block.
fn is_continuation(line: &str) -> bool {
    line.trim().is_empty() || line.starts_with([' ', '\t']) || strip_item(line).is_some()
}

/// The text after `- ` in a list item, or `None` if the line is not one.
fn strip_item(line: &str) -> Option<&str> {
    if line == "-" { Some("") } else { line.strip_prefix("- ") }
}

fn value(v: &str, block: &[&str]) -> Option<Value> {
    let nested = block.iter().any(|l| !l.trim().is_empty());
    if v.starts_with(['>', '|']) {
        return block_scalar(v, block).map(Value::Text);
    }
    if v.starts_with('[') {
        if nested {
            return None;
        }
        let inner = v.strip_prefix('[')?.strip_suffix(']')?;
        let items = split_items(inner)
            .into_iter()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(scalar)
            .collect::<Option<Vec<_>>>()?;
        return Some(Value::List(items));
    }
    if v.is_empty() {
        if !nested {
            return Some(Value::Text(String::new()));
        }
        let items: Vec<&str> =
            block.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        if items.is_empty() {
            return Some(Value::Text(String::new()));
        }
        // Keep the list only if every line is an item; anything else is a nested block shown as text.
        if let Some(stripped) = items.iter().map(|l| strip_item(l)).collect::<Option<Vec<_>>>() {
            let list = stripped
                .into_iter()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(scalar)
                .collect::<Option<Vec<_>>>()?;
            return Some(Value::List(list));
        }
        return Some(Value::Text(dedent(block).join("\n")));
    }
    let quoted = v.starts_with(['"', '\'']);
    if quoted && nested {
        // A quoted scalar spanning lines is not read.
        return None;
    }
    let text = scalar(v)?;
    if !nested {
        return Some(Value::Text(text));
    }
    // A plain scalar may continue on indented lines; YAML folds them with spaces.
    let mut parts = vec![text];
    parts.extend(block.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()));
    Some(Value::Text(parts.join(" ")))
}

/// `|` keeps the line breaks, `>` joins lines with spaces and turns blank lines into breaks.
/// Chomping marks (`-`, `+`) are accepted but both keep the text without trailing blank lines.
fn block_scalar(v: &str, block: &[&str]) -> Option<String> {
    if !v[1..].chars().all(|c| c == '-' || c == '+') {
        return None;
    }
    if block.iter().any(|l| !l.trim().is_empty() && !l.starts_with([' ', '\t'])) {
        return None;
    }
    let lines = dedent(block);
    Some(if v.starts_with('>') { fold(&lines) } else { lines.join("\n") })
}

fn fold(lines: &[String]) -> String {
    let mut out = String::new();
    let mut newlines = 0;
    for line in lines {
        if line.is_empty() {
            newlines += 1;
            continue;
        }
        if !out.is_empty() {
            if newlines == 0 {
                out.push(' ');
            } else {
                out.push_str(&"\n".repeat(newlines));
            }
        }
        newlines = 0;
        out.push_str(line);
    }
    out
}

/// Removes the indentation all non-blank lines share, and the blank lines at both ends.
fn dedent(block: &[&str]) -> Vec<String> {
    let indent =
        block.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    let mut lines: Vec<String> = block.iter().map(|l| l.get(indent..).unwrap_or("").trim_end().to_string()).collect();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let lead = lines.iter().take_while(|l| l.is_empty()).count();
    lines.drain(..lead);
    lines
}

/// Splits an inline list at commas that are not inside quotes.
fn split_items(inner: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, ',') => {
                items.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&inner[start..]);
    items
}

/// The text of a scalar with its quotes removed. `None` if a quote is not closed on this line.
fn scalar(v: &str) -> Option<String> {
    if let Some(inner) = v.strip_prefix('"') {
        let inner = inner.strip_suffix('"')?;
        Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
    } else if let Some(inner) = v.strip_prefix('\'') {
        let inner = inner.strip_suffix('\'')?;
        Some(inner.replace("''", "'"))
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }

    fn list(items: &[&str]) -> Value {
        Value::List(items.iter().map(|s| s.to_string()).collect())
    }

    fn one(key: &str, value: Value) -> Option<Vec<(String, Value)>> {
        Some(vec![(key.to_string(), value)])
    }

    #[test]
    fn simple_pairs() {
        let yaml = "name: orquestar-issues\ndescription: Revisa los issues abiertos del repo. Úsala cuando el usuario pida trabajar issues.\n";
        assert_eq!(
            parse(yaml),
            Some(vec![
                ("name".into(), text("orquestar-issues")),
                (
                    "description".into(),
                    text("Revisa los issues abiertos del repo. Úsala cuando el usuario pida trabajar issues.")
                ),
            ])
        );
    }

    #[test]
    fn value_may_contain_colons() {
        assert_eq!(parse("url: https://ejemplo.org/a:b"), one("url", text("https://ejemplo.org/a:b")));
    }

    #[test]
    fn empty_value_is_empty_text() {
        assert_eq!(parse("tags:\nautor: Ana\n"), Some(vec![("tags".into(), text("")), ("autor".into(), text("Ana"))]));
    }

    #[test]
    fn quotes_are_removed() {
        assert_eq!(parse("title: \"Hola: mundo\""), one("title", text("Hola: mundo")));
        assert_eq!(parse("title: 'it''s'"), one("title", text("it's")));
        assert_eq!(parse("title: \"di \\\"hola\\\"\""), one("title", text("di \"hola\"")));
    }

    #[test]
    fn unclosed_quote_is_malformed() {
        assert_eq!(parse("title: \"abierta"), None);
    }

    #[test]
    fn inline_list() {
        assert_eq!(parse("tags: [a, \"b, c\", d]"), one("tags", list(&["a", "b, c", "d"])));
        assert_eq!(parse("tags: []"), one("tags", list(&[])));
    }

    #[test]
    fn block_list_indented_and_not() {
        assert_eq!(parse("tags:\n  - a\n  - b\n"), one("tags", list(&["a", "b"])));
        assert_eq!(
            parse("tags:\n- x\n- \"y\"\nfin: 1\n"),
            Some(vec![("tags".into(), list(&["x", "y"])), ("fin".into(), text("1"))])
        );
    }

    #[test]
    fn folded_block_joins_lines() {
        let yaml = "description: >\n  Primera línea\n  segunda línea\n\n  nuevo párrafo\nname: x\n";
        assert_eq!(
            parse(yaml),
            Some(vec![
                ("description".into(), text("Primera línea segunda línea\nnuevo párrafo")),
                ("name".into(), text("x"))
            ])
        );
        assert_eq!(parse("d: >-\n  uno\n  dos\n"), one("d", text("uno dos")));
        assert_eq!(parse("d: >+\n  uno\n"), one("d", text("uno")));
    }

    #[test]
    fn literal_block_keeps_breaks() {
        assert_eq!(parse("notas: |\n  uno\n    dos\n  tres\n"), one("notas", text("uno\n  dos\ntres")));
        assert_eq!(parse("notas: |-\n  x\n"), one("notas", text("x")));
        assert_eq!(parse("notas: |+\n  x\n"), one("notas", text("x")));
    }

    #[test]
    fn nested_mapping_is_text() {
        assert_eq!(parse("metadata:\n  type: x\n  sub:\n    k: v\n"), one("metadata", text("type: x\nsub:\n  k: v")));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        assert_eq!(
            parse("# comentario\n\nname: x\n\n# otro\nversion: 2\n"),
            Some(vec![("name".into(), text("x")), ("version".into(), text("2"))])
        );
    }

    #[test]
    fn real_skill_front_matter() {
        let yaml = "name: orquestar-issues\ndescription: Revisa los issues abiertos del repo. Úsala cuando el usuario pida trabajar issues.";
        let props = parse(yaml).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].0, "name");
        assert_eq!(props[1].0, "description");
    }

    #[test]
    fn malformed_lines_give_none() {
        assert_eq!(parse("name: x\nesto no es clave"), None);
        assert_eq!(parse("name:x"), None);
        assert_eq!(parse("- suelto"), None);
        assert_eq!(parse("  sangrada: x"), None);
        assert_eq!(parse("name: [a, b"), None);
    }
}
