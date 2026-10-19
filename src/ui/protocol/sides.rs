//! Area B, the sides and their tab rows. Frame side: `marco/src/areas/sides/`.

use crate::vault;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What the tab rows ask for. Tabs and sides go by their ids in [`State`].
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    Select { tab: u64 },
    Close { tab: u64 },
    /// A new empty tab at the end of this side.
    New { side: u64 },
    /// The person clicked in this side: keys and agents act on it.
    Focus { side: u64 },
}

/// The sides, left first.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct State {
    pub sides: Vec<Side>,
    /// Index of the side keys and agents act on.
    pub focused: usize,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Side {
    pub id: u64,
    pub tabs: Vec<Tab>,
    pub active: usize,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Tab {
    pub id: u64,
    pub label: String,
    /// Vault path, absolute path of a loose page, web address, or empty.
    pub target: String,
    pub kind: Kind,
}

/// What a tab shows, for its icon.
#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Empty,
    Note,
    Drawing,
    Html,
    Pdf,
    Image,
    Web,
    Other,
}

/// The kind of a tab's target.
pub fn kind_of(target: &str) -> Kind {
    if target.is_empty() {
        return Kind::Empty;
    }
    if vault::is_web(target) {
        return Kind::Web;
    }
    let path = Path::new(target);
    if vault::is_drawing(path) {
        Kind::Drawing
    } else if vault::is_markdown(path) {
        Kind::Note
    } else if vault::is_html(path) {
        Kind::Html
    } else if vault::is_pdf(path) {
        Kind::Pdf
    } else if vault::is_image(path) {
        Kind::Image
    } else {
        Kind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_knows_what_it_shows() {
        assert_eq!(kind_of(""), Kind::Empty);
        assert_eq!(kind_of("https://ejemplo.com/a.md"), Kind::Web);
        assert_eq!(kind_of("temas/Nota.MD"), Kind::Note);
        assert_eq!(kind_of("dibujo.excalidraw.md"), Kind::Drawing);
        assert_eq!(kind_of("a.htm"), Kind::Html);
        assert_eq!(kind_of("apuntes.pdf"), Kind::Pdf);
        assert_eq!(kind_of("foto.webp"), Kind::Image);
        assert_eq!(kind_of("datos.csv"), Kind::Other);
    }
}
