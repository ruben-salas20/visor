use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

const PALETTE: [&str; 8] = ["red", "orange", "yellow", "green", "cyan", "blue", "magenta", "brown"];

pub struct Theme {
    pub dark: bool,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub muted: String,
    pub selection: String,
    pub palette: Vec<(&'static str, String)>,
    pub mono: Option<String>,
}

/// The folder Omarchy rewrites on every theme switch.
pub fn dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".local/state/omarchy/current"))
}

impl Theme {
    /// `mono` comes from [`monospace_font`], which is slow enough to run apart.
    pub fn load(mono: Option<String>) -> Option<Theme> {
        let text = std::fs::read_to_string(dir()?.join("theme/colors.toml")).ok()?;
        let keys = parse(&text);
        let get = |names: &[&str]| names.iter().find_map(|n| keys.get(*n).cloned());
        let bg = get(&["background", "bg"])?;
        let fg = get(&["foreground", "fg"])?;
        let dark = match get(&["mode", "theme_type"]).as_deref() {
            Some("light") => false,
            Some("dark") => true,
            _ => luminance(&bg).is_some_and(|l| l < 0.5),
        };
        Some(Theme {
            dark,
            accent: get(&["accent", "blue"]).unwrap_or_else(|| fg.clone()),
            muted: get(&["muted", "color8"]).unwrap_or_else(|| fg.clone()),
            selection: get(&["selection", "selection_background"]).unwrap_or_else(|| bg.clone()),
            palette: PALETTE.iter().filter_map(|&n| Some((n, keys.get(n)?.clone()))).collect(),
            mono,
            bg,
            fg,
        })
    }

    /// Variables offered to pages. Author styles win over user styles, so pages
    /// that set their own colors are left alone.
    pub fn page_css(&self) -> String {
        let mut vars = vec![
            format!("--visor-mode: {}", if self.dark { "dark" } else { "light" }),
            format!("--visor-bg: {}", self.bg),
            format!("--visor-fg: {}", self.fg),
            format!("--visor-accent: {}", self.accent),
            format!("--visor-muted: {}", self.muted),
            format!("--visor-selection: {}", self.selection),
        ];
        vars.extend(self.palette.iter().map(|(name, value)| format!("--visor-{name}: {value}")));
        if let Some(font) = &self.mono {
            vars.push(format!("--visor-font-mono: \"{font}\", monospace"));
        }
        format!(":root {{ {}; }}", vars.join("; "))
    }

    pub fn rgba(&self) -> (u8, u8, u8, u8) {
        let [r, g, b] = rgb(&self.bg).unwrap_or([0, 0, 0]);
        (r, g, b, 255)
    }

    /// `pct` percent of the text color over the background, like the sidebar's
    /// `color-mix()` tones, which GTK's CSS lacks.
    pub fn tone(&self, pct: f32) -> String {
        mix(&self.bg, &self.fg, pct).unwrap_or_else(|| self.muted.clone())
    }

    pub fn accent_tone(&self, pct: f32) -> String {
        mix(&self.bg, &self.accent, pct).unwrap_or_else(|| self.selection.clone())
    }
}

fn mix(base: &str, over: &str, pct: f32) -> Option<String> {
    let (a, b) = (rgb(base)?, rgb(over)?);
    let channel = |i: usize| (a[i] as f32 * (100.0 - pct) / 100.0 + b[i] as f32 * pct / 100.0).round() as u8;
    Some(format!("#{:02x}{:02x}{:02x}", channel(0), channel(1), channel(2)))
}

/// Reads the flat `key = "value"` lines of colors.toml.
fn parse(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with('[') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            let value = match value.strip_prefix('"') {
                Some(quoted) => &quoted[..quoted.find('"')?],
                None => value.split(['#', ' ']).next()?,
            };
            Some((key.trim().to_string(), value.to_string()))
        })
        .collect()
}

fn rgb(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn luminance(hex: &str) -> Option<f32> {
    let [r, g, b] = rgb(hex)?;
    Some((0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0)
}

/// Asks fontconfig for the monospace family. Spawning `fc-match` takes ~20 ms.
pub fn monospace_font() -> Option<String> {
    let out = Command::new("fc-match").args(["monospace", "-f", "%{family}"]).output().ok()?;
    let family = String::from_utf8(out.stdout).ok()?;
    let first = family.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colors_toml() {
        let keys = parse("mode = \"dark\"\n# comentario\n[seccion]\naccent = \"#ff8a00\" # naranja\nbg=\"#000000\"\n");
        assert_eq!(keys["mode"], "dark");
        assert_eq!(keys["accent"], "#ff8a00");
        assert_eq!(keys["bg"], "#000000");
        assert!(!keys.contains_key("[seccion]"));
    }

    #[test]
    fn mixes_colors() {
        assert_eq!(mix("#000000", "#ffffff", 50.0).as_deref(), Some("#808080"));
    }

    #[test]
    fn luminance_detects_dark() {
        assert!(luminance("#000000").unwrap() < 0.5);
        assert!(luminance("#ffffff").unwrap() > 0.5);
    }
}
