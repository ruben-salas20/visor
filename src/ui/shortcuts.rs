//! The window that lists the keyboard shortcuts. It is built on the first
//! Ctrl+K and then only shown and hidden, so it costs nothing until asked for.

use gtk::gdk::{self, ModifierType, keys::constants as key};
use gtk::glib::Propagation;
use gtk::prelude::*;

/// The shortcuts of the app, as the window lists them. Each key is spelled as
/// `agent/help.txt` spells it, which a test checks.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+O", "abrir carpeta"),
    ("Ctrl+B", "mostrar u ocultar el panel"),
    ("Ctrl+R / F5", "recargar"),
    ("Ctrl+L", "barra de direcciones: URL, dominio, ruta de la bóveda o ruta absoluta"),
    ("Ctrl+Shift+B", "mostrar u ocultar la barra de direcciones"),
    ("Ctrl+E", "editar la nota o el dibujo abiertos, o volver a la vista"),
    ("Ctrl+N", "nueva nota en la carpeta de la actual"),
    ("Ctrl+Shift+A", "anotar un elemento o un punto"),
    ("Ctrl+Shift+L", "lista de anotaciones de la página"),
    ("Ctrl+T", "pestaña nueva"),
    ("Ctrl+W", "cerrar la pestaña"),
    ("Ctrl+Tab", "pestaña siguiente"),
    ("Ctrl+Shift+Tab", "pestaña anterior"),
    ("Ctrl+1 … Ctrl+8", "pestaña número N del lado con foco"),
    ("Ctrl+9", "la última pestaña del lado con foco"),
    ("Ctrl+| (o Ctrl+\\)", "dividir la ventana o volver a un lado"),
    ("Ctrl+Alt+←", "lado izquierdo"),
    ("Ctrl+Alt+→", "lado derecho"),
    ("Ctrl++", "aumentar el tamaño del texto"),
    ("Ctrl+-", "reducir el tamaño del texto"),
    ("Ctrl+0", "restablecer el tamaño del texto"),
    ("Ctrl+,", "ajustes: tema, tamaño del texto y actualizaciones"),
    ("Alt+←", "atrás"),
    ("Alt+→", "adelante"),
    ("Ctrl+K", "esta lista de atajos"),
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
    window.set_title("Atajos de teclado");
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
    for (row, (keys, what)) in SHORTCUTS.iter().enumerate() {
        let keys_label = gtk::Label::new(Some(keys));
        keys_label.set_xalign(0.0);
        keys_label.style_context().add_class("visor-shortcut-key");
        let what_label = gtk::Label::new(Some(what));
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
