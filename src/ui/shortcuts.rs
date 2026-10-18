//! The window that lists the keyboard shortcuts. It is built on the first
//! Ctrl+K and then only shown and hidden, so it costs nothing until asked for.

use crate::i18n::tr;
use gtk::gdk::{self, ModifierType, keys::constants as key};
use gtk::glib::Propagation;
use gtk::prelude::*;

/// The shortcuts of the app, as the window lists them. Each key is spelled as
/// `agent/help.txt` spells it, which a test checks.
/// Each description is (Spanish, English), picked at run time by `tr`.
pub const SHORTCUTS: &[(&str, (&str, &str))] = &[
    ("Ctrl+O", ("abrir carpeta", "open folder")),
    ("Ctrl+B", ("mostrar u ocultar el panel", "show or hide the panel")),
    ("Ctrl+R / F5", ("recargar", "reload")),
    ("Ctrl+L", ("barra de direcciones: URL, dominio, ruta de la bóveda, ruta absoluta o búsqueda", "address bar: URL, domain, vault path, absolute path or search")),
    ("Ctrl+Shift+B", ("mostrar u ocultar la barra de direcciones", "show or hide the address bar")),
    ("Ctrl+E", ("editar la nota o el dibujo abiertos, o volver a la vista", "edit the open note or drawing, or go back to the view")),
    ("Ctrl+F", ("buscar en la página", "find in the page")),
    ("Ctrl+N", ("nueva nota en la carpeta de la actual", "new note in the current folder")),
    ("Ctrl+Shift+A", ("anotar un elemento o un punto", "annotate an element or a point")),
    ("Ctrl+Shift+L", ("lista de anotaciones de la página", "list of the page's annotations")),
    ("Ctrl+T", ("pestaña nueva, con el cursor en la barra", "new tab, with the cursor in the bar")),
    ("Ctrl+W", ("cerrar la pestaña", "close the tab")),
    ("Ctrl+Tab", ("pestaña siguiente", "next tab")),
    ("Ctrl+Shift+Tab", ("pestaña anterior", "previous tab")),
    ("Ctrl+1 … Ctrl+8", ("pestaña número N del lado con foco", "tab number N on the focused side")),
    ("Ctrl+9", ("la última pestaña del lado con foco", "the last tab on the focused side")),
    ("Ctrl+| (o Ctrl+\\)", ("dividir la ventana o volver a un lado", "split the window, or go back to one side")),
    ("Ctrl+Alt+←", ("lado izquierdo", "left side")),
    ("Ctrl+Alt+→", ("lado derecho", "right side")),
    ("Ctrl++", ("aumentar el tamaño del texto", "increase the text size")),
    ("Ctrl+-", ("reducir el tamaño del texto", "decrease the text size")),
    ("Ctrl+0", ("restablecer el tamaño del texto", "reset the text size")),
    ("Ctrl+,", ("ajustes: tema, tamaño del texto y actualizaciones", "settings: theme, text size and updates")),
    ("Alt+←", ("atrás", "back")),
    ("Alt+→", ("adelante", "forward")),
    ("Ctrl+K", ("esta lista de atajos", "this shortcut list")),
];

/// The shortcuts window, made on first use and kept after that.
pub struct Shortcuts {
    parent: gtk::Window,
    window: Option<gtk::Window>,
}

impl Shortcuts {
    pub fn new(parent: &impl IsA<gtk::Window>) -> Self {
        Self { parent: parent.clone().upcast(), window: None }
    }

    /// Shows the window, or hides it if it is showing.
    pub fn toggle(&mut self) {
        let window = self.window.get_or_insert_with(|| build(&self.parent));
        if window.is_visible() {
            window.hide();
        } else {
            window.show_all();
            window.present();
        }
    }
}

fn build(parent: &gtk::Window) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(tr("Atajos de teclado", "Keyboard shortcuts"));
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_type_hint(gdk::WindowTypeHint::Dialog);
    window.set_position(gtk::WindowPosition::CenterOnParent);
    window.set_resizable(false);
    window.set_border_width(16);
    window.style_context().add_class("visor-shortcuts");

    let grid = gtk::Grid::new();
    grid.set_column_spacing(24);
    grid.set_row_spacing(6);
    for (row, (keys, (es, en))) in SHORTCUTS.iter().enumerate() {
        let keys_label = gtk::Label::new(Some(keys));
        keys_label.set_xalign(0.0);
        keys_label.style_context().add_class("visor-shortcut-key");
        let what_label = gtk::Label::new(Some(tr(es, en)));
        what_label.set_xalign(0.0);
        grid.attach(&keys_label, 0, row as i32, 1, 1);
        grid.attach(&what_label, 1, row as i32, 1, 1);
    }
    window.add(&grid);

    // Closing hides instead of destroying: the next Ctrl+K shows the same window.
    window.connect_delete_event(|w, _| {
        w.hide();
        Propagation::Stop
    });
    // This window has the focus while it shows, so the app's shortcut handler
    // does not see these keys. Ctrl+K closes it, as it opened it.
    window.connect_key_press_event(|w, event| {
        let ctrl = event.state().contains(ModifierType::CONTROL_MASK);
        let k = event.keyval().to_lower();
        if k == key::Escape || (ctrl && k == key::k) {
            w.hide();
            return Propagation::Stop;
        }
        Propagation::Proceed
    });
    window
}

#[cfg(test)]
mod tests {
    use super::SHORTCUTS;

    const HELP: &str = include_str!("../agent/help.txt");

    #[test]
    fn every_shortcut_is_named_in_help() {
        for (keys, _) in SHORTCUTS {
            assert!(HELP.contains(keys), "agent/help.txt no nombra el atajo {keys}");
        }
    }
}
