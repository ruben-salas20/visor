//! The file panel, in native GTK. As a WebKit view it cost a web process of
//! its own (~65–100 MB) to list files. See docs/adr/0003.
use crate::i18n::tr;
use crate::trf;
use crate::ui::action::{Action, SidebarMsg, UserEvent};
use crate::ui::address::Engine;
use crate::ui::icons::{draw_chevron, draw_chevron_bold, draw_gear, draw_new_folder, draw_plus, drawn_icon, icon_button};
use crate::ui::theme::Theme;
use crate::vault::{self, Dir};
use gtk::cairo::{Context, Format, ImageSurface};
use gtk::gdk;
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tao::event_loop::EventLoopProxy;

const NAME: u32 = 0;
const PATH: u32 = 1;
const IS_DIR: u32 = 2;
const TIP: u32 = 3;
const KIND: u32 = 4;
const DRAG_TARGET: &str = "application/x-visor-path";

struct Colors {
    fg: Option<String>,
    muted: String,
    accent: String,
    active: String,
    hover: String,
    /// Files visor cannot open: fainter than folders, which share `muted`.
    faint: String,
}

impl Colors {
    fn new(theme: Option<&Theme>) -> Colors {
        match theme {
            Some(t) => Colors {
                fg: Some(t.fg.clone()),
                muted: t.muted.clone(),
                accent: t.accent.clone(),
                active: t.accent_tone(20.0),
                hover: t.tone(9.0),
                faint: t.tone(40.0),
            },
            None => Colors {
                fg: None,
                muted: "#6e6e76".into(),
                accent: "#3a63c7".into(),
                active: "#d8e0f4".into(),
                hover: "#e6e6ea".into(),
                faint: "#a4a4aa".into(),
            },
        }
    }
}

/// Size of the folder arrow cell, in logical px. The names start 4 px after it
/// (the text cell's padding); each level adds `LEVEL_STEP`, as GTK's expander did.
const ARROW_W: f64 = 18.0;
const ARROW_H: f64 = 16.0;
const LEVEL_STEP: i32 = 18;

/// Folder arrows as surfaces for the tree's pixbuf cell, in the muted color.
/// Drawn here because GTK's own expander is a theme icon, and loading one
/// costs a sandboxed SVG load (see icons.rs). Rebuilt when the theme or the
/// screen scale changes.
struct Arrows {
    scale: i32,
    right: ImageSurface,
    down: ImageSurface,
    blank: ImageSurface,
}

impl Arrows {
    fn new(muted: &str, scale: i32) -> Arrows {
        let rgba = gdk::RGBA::parse(muted).unwrap_or(gdk::RGBA::new(0.43, 0.43, 0.46, 1.0));
        Arrows {
            scale,
            right: arrow(&rgba, scale, Some(-std::f64::consts::FRAC_PI_2)),
            down: arrow(&rgba, scale, Some(0.0)),
            blank: arrow(&rgba, scale, None),
        }
    }
}

/// One arrow pointing down, turned by `turn` radians; `None` gives an empty surface.
fn arrow(rgba: &gdk::RGBA, scale: i32, turn: Option<f64>) -> ImageSurface {
    let px = |v: f64| (v * scale as f64).round() as i32;
    let surface = ImageSurface::create(Format::ARgb32, px(ARROW_W), px(ARROW_H)).expect("arrow surface");
    // The device scale also scales the new context, so no cr.scale here.
    surface.set_device_scale(scale as f64, scale as f64);
    let cr = Context::new(&surface).expect("arrow context");
    let Some(turn) = turn else { return surface };
    cr.set_source_rgba(rgba.red(), rgba.green(), rgba.blue(), rgba.alpha());
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    // The glyph is drawn on the 16-unit grid and enlarged to about 7 px;
    // its line width is set after the path, in grid units, to stay at 1.5 px.
    // The 1.5 px nudge evens the gaps on both sides of the arrow: the names
    // start 4 px after the cell, so a centred arrow sits too far from them.
    const GROW: f64 = 1.2;
    cr.translate(ARROW_W / 2.0 + 1.5, ARROW_H / 2.0);
    cr.rotate(turn);
    cr.scale(GROW, GROW);
    cr.translate(-5.0, -5.0);
    draw_chevron(&cr);
    cr.set_line_width(1.5 / GROW);
    let _ = cr.stroke();
    surface
}

struct State {
    has_vault: bool,
    /// The file on the focused side, marked in the tree.
    file: Option<String>,
    opened: HashSet<String>,
    /// Set while the tree is rebuilt, so restoring folders is not reported as a change.
    restoring: bool,
    new_dir: String,
    new_folder: bool,
    /// The row under the last left press, while it may still become a click.
    pressed: Option<gtk::TreePath>,
    dragged: Option<String>,
    /// The folder a drop would go into; empty for the vault root.
    drop_target: Option<String>,
    /// Bumped on every change of drop target and notice, to cancel their pending timers.
    drop_gen: u64,
    notice_gen: u64,
    /// The file the notice offers to open elsewhere, if any.
    notice_path: Option<String>,
    /// The row the context menu was opened on.
    menu_path: Option<String>,
    /// The row under the pointer; none on the notice of an empty folder.
    hover: Option<String>,
    /// Whether files visor cannot open are listed.
    show_other: bool,
    colors: Colors,
    arrows: Arrows,
}

#[derive(Clone)]
pub struct Sidebar {
    pub root: gtk::Box,
    vault_name: gtk::Label,
    popover: gtk::Popover,
    vault_list: gtk::Box,
    new_button: gtk::Button,
    folder_button: gtk::Button,
    settings: gtk::Popover,
    theme_buttons: ChoiceButtons,
    language_buttons: ChoiceButtons,
    language_restart: gtk::Box,
    zoom_label: gtk::Label,
    auto_update: gtk::CheckButton,
    show_other: gtk::CheckButton,
    search_engine: gtk::ComboBoxText,
    update: UpdateBox,
    /// Set while the settings are filled in, so that is not sent as a change.
    syncing: Rc<Cell<bool>>,
    form: gtk::Box,
    form_label: gtk::Label,
    entry: gtk::Entry,
    form_error: gtk::Label,
    scroll: gtk::ScrolledWindow,
    tree: gtk::TreeView,
    store: gtk::TreeStore,
    menu: gtk::Popover,
    folder_items: Vec<gtk::Widget>,
    separator: gtk::Separator,
    elsewhere: gtk::Button,
    notice: gtk::Box,
    notice_text: gtk::Label,
    notice_open: gtk::Button,
    state: Rc<RefCell<State>>,
    proxy: EventLoopProxy<UserEvent>,
}

impl Sidebar {
    pub fn new(proxy: &EventLoopProxy<UserEvent>) -> Sidebar {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.style_context().add_class("visor-sidebar");

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        head.style_context().add_class("visor-side-head");
        let vault_name = gtk::Label::new(Some(tr("Sin bóveda", "No vault")));
        vault_name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        vault_name.set_xalign(0.0);
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        inner.pack_start(&vault_name, true, true, 0);
        inner.pack_start(&drawn_icon(10, draw_chevron_bold), false, false, 0);
        let vault_button = gtk::MenuButton::new();
        vault_button.add(&inner);
        vault_button.set_tooltip_text(Some(tr("Cambiar de bóveda", "Change vault")));
        vault_button.style_context().add_class("visor-vault");
        let popover = gtk::Popover::new(Some(&vault_button));
        popover.style_context().add_class("visor-vaults");
        let vault_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        popover.add(&vault_list);
        vault_button.set_popover(Some(&popover));
        let new_button = icon_button(tr("Nueva nota (Ctrl+N)", "New note (Ctrl+N)"), draw_plus);
        let folder_button = icon_button(tr("Nueva carpeta", "New folder"), draw_new_folder);
        head.pack_start(&vault_button, true, true, 0);
        head.pack_start(&new_button, false, false, 0);
        head.pack_start(&folder_button, false, false, 0);
        let gear = gtk::MenuButton::new();
        gear.add(&drawn_icon(16, draw_gear));
        gear.set_tooltip_text(Some(tr("Ajustes (Ctrl+,)", "Settings (Ctrl+,)")));
        let foot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        foot.style_context().add_class("visor-side-foot");
        foot.pack_start(&gear, false, false, 0);
        let version = gtk::Label::new(Some(concat!("v", env!("CARGO_PKG_VERSION"))));
        version.style_context().add_class("visor-muted");
        // Smaller text sits high in a row sized by the button; center it on the gear.
        version.set_valign(gtk::Align::Center);
        version.set_tooltip_text(Some(tr("Versión de visor", "visor version")));
        foot.pack_end(&version, false, false, 0);
        let syncing = Rc::new(Cell::new(false));
        let update = UpdateBox::new(proxy);
        let (settings, theme_buttons, language_buttons, language_restart, zoom_label, auto_update, show_other, search_engine) =
            settings_popover(&gear, proxy, &syncing, &update);
        gear.set_popover(Some(&settings));

        let form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        form.style_context().add_class("visor-new");
        form.set_no_show_all(true);
        let form_label = gtk::Label::new(None);
        form_label.set_xalign(0.0);
        form_label.set_line_wrap(true);
        let entry = gtk::Entry::new();
        let form_error = gtk::Label::new(None);
        form_error.set_xalign(0.0);
        form_error.set_line_wrap(true);
        form_error.style_context().add_class("error");
        form_error.set_no_show_all(true);
        form.pack_start(&form_label, false, false, 0);
        form.pack_start(&entry, false, false, 0);
        form.pack_start(&form_error, false, false, 0);
        form_label.show();
        entry.show();

        let store = gtk::TreeStore::new(&[
            String::static_type(),
            String::static_type(),
            bool::static_type(),
            String::static_type(),
            u32::static_type(),
        ]);
        let tree = gtk::TreeView::with_model(&store);
        let scale = tree.scale_factor();
        tree.set_headers_visible(false);
        tree.set_show_expanders(false);
        tree.set_level_indentation(LEVEL_STEP);
        tree.selection().set_mode(gtk::SelectionMode::None);
        tree.set_tooltip_column(TIP as i32);
        let cell = gtk::CellRendererText::new();
        cell.set_property("ellipsize", gtk::pango::EllipsizeMode::End);
        cell.set_padding(4, 3);
        let arrow = gtk::CellRendererPixbuf::new();
        arrow.set_padding(0, 0);
        let column = gtk::TreeViewColumn::new();
        CellLayoutExt::pack_start(&column, &arrow, false);
        CellLayoutExt::pack_start(&column, &cell, true);
        CellLayoutExt::add_attribute(&column, &cell, "text", NAME as i32);
        tree.append_column(&column);
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.add(&tree);

        // A popover, not a gtk::Menu: a Menu is its own Wayland popup window, which Hyprland
        // makes see-through without blurring the tree behind it.
        let menu = gtk::Popover::new(Some(&tree));
        menu.style_context().add_class("visor-menu");
        menu.set_position(gtk::PositionType::Bottom);
        menu.set_modal(true);
        let menu_button = |label: &str, accel: &str| {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            row.pack_start(&text, true, true, 0);
            if !accel.is_empty() {
                let key = gtk::Label::new(Some(accel));
                key.style_context().add_class("visor-accel");
                row.pack_end(&key, false, false, 0);
            }
            let button = gtk::Button::new();
            button.set_relief(gtk::ReliefStyle::None);
            button.add(&row);
            button
        };
        let note_here = menu_button(tr("Nueva nota aquí", "New note here"), "");
        let folder_here = menu_button(tr("Nueva carpeta aquí", "New folder here"), "");
        let elsewhere = menu_button(tr("Abrir con otra aplicación", "Open with another application"), "");
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        let trash = menu_button(tr("Mover a la papelera", "Move to trash"), tr("Supr", "Del"));
        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for item in [note_here.clone().upcast::<gtk::Widget>(), folder_here.clone().upcast(), elsewhere.clone().upcast(), separator.clone().upcast(), trash.clone().upcast()] {
            menu_box.pack_start(&item, false, false, 0);
        }
        menu_box.show_all();
        menu.add(&menu_box);

        let notice_text = gtk::Label::new(None);
        notice_text.set_xalign(0.0);
        notice_text.set_line_wrap(true);
        notice_text.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        notice_text.show();
        let notice_open = gtk::Button::with_label(tr("Abrir con otra aplicación", "Open with another application"));
        notice_open.set_halign(gtk::Align::Start);
        notice_open.hide();
        let notice = gtk::Box::new(gtk::Orientation::Vertical, 4);
        notice.style_context().add_class("visor-notice");
        notice.pack_start(&notice_text, false, false, 0);
        notice.pack_start(&notice_open, false, false, 0);
        notice.set_no_show_all(true);

        root.pack_start(&head, false, false, 0);
        root.pack_start(&form, false, false, 0);
        root.pack_start(&scroll, true, true, 0);
        root.pack_start(&notice, false, false, 0);
        root.pack_start(&foot, false, false, 0);
        root.show_all();

        let sidebar = Sidebar {
            root,
            vault_name,
            popover,
            vault_list,
            new_button,
            folder_button,
            settings,
            theme_buttons,
            language_buttons,
            language_restart,
            zoom_label,
            auto_update,
            show_other,
            search_engine,
            update,
            syncing,
            form,
            form_label,
            entry,
            form_error,
            scroll,
            tree,
            store,
            menu,
            folder_items: vec![note_here.clone().upcast(), folder_here.clone().upcast()],
            separator,
            elsewhere: elsewhere.clone(),
            notice,
            notice_text,
            notice_open: notice_open.clone(),
            state: Rc::new(RefCell::new(State {
                has_vault: false,
                file: None,
                opened: HashSet::new(),
                restoring: false,
                new_dir: String::new(),
                new_folder: false,
                pressed: None,
                dragged: None,
                drop_target: None,
                drop_gen: 0,
                notice_gen: 0,
                notice_path: None,
                menu_path: None,
                hover: None,
                show_other: true,
                colors: Colors::new(None),
                arrows: Arrows::new("#6e6e76", scale),
            })),
            proxy: proxy.clone(),
        };
        sidebar.style_rows(&column, &cell, &arrow);
        sidebar.connect_header();
        sidebar.connect_form();
        sidebar.connect_clicks();
        sidebar.connect_hover();
        sidebar.connect_folders();
        sidebar.connect_drag();
        let s = sidebar.clone();
        note_here.connect_clicked(move |_| {
            s.menu.popdown();
            s.new_note_at_menu(false)
        });
        let s = sidebar.clone();
        folder_here.connect_clicked(move |_| {
            s.menu.popdown();
            s.new_note_at_menu(true)
        });
        let s = sidebar.clone();
        trash.connect_clicked(move |_| {
            s.menu.popdown();
            if let Some(path) = s.state.borrow_mut().menu_path.take() {
                s.send(SidebarMsg::Trash { path });
            }
        });
        let s = sidebar.clone();
        elsewhere.connect_clicked(move |_| {
            s.menu.popdown();
            if let Some(path) = s.state.borrow_mut().menu_path.take() {
                s.send(SidebarMsg::OpenElsewhere { path });
            }
        });
        let s = sidebar.clone();
        notice_open.connect_clicked(move |_| {
            let path = s.state.borrow().notice_path.clone();
            if let Some(path) = path {
                s.send(SidebarMsg::OpenElsewhere { path });
            }
        });
        sidebar
    }

    fn send(&self, msg: SidebarMsg) {
        let _ = self.proxy.send_event(UserEvent::Sidebar(msg));
    }

    pub fn set_language_pending(&self, pending: bool) {
        self.language_restart.set_visible(pending);
    }

    /// Fills in the settings without reporting them as changes. Theme names
    /// other than «claro» and «oscuro» show as «Sistema», as the app treats them.
    pub fn set_settings(&self, theme: Option<&str>, zoom: f64, auto_update: bool, show_other: bool, search_engine: Engine, language: Option<&str>) {
        let theme = match theme {
            Some(t @ ("claro" | "oscuro")) => Some(t),
            _ => None,
        };
        self.syncing.set(true);
        // Only the languages the settings offer; anything else shows as the system's choice.
        let language = match language {
            Some(l @ ("es" | "en")) => Some(l),
            _ => None,
        };
        for (choice, button) in &self.theme_buttons {
            button.set_active(*choice == theme);
        }
        for (choice, button) in &self.language_buttons {
            button.set_active(*choice == language);
        }
        self.auto_update.set_active(auto_update);
        self.show_other.set_active(show_other);
        self.search_engine.set_active_id(Some(search_engine.id()));
        self.syncing.set(false);
        self.set_show_other(show_other);
        self.set_zoom(zoom);
    }

    /// Takes effect on the next `set_tree`.
    pub fn set_show_other(&self, show: bool) {
        self.state.borrow_mut().show_other = show;
    }

    pub fn set_zoom(&self, zoom: f64) {
        // A non-breaking space keeps «100 %» on one line, as Spanish typesetting does.
        self.zoom_label.set_text(&format!("{}\u{a0}%", (zoom * 100.0).round() as i32));
    }

    pub fn popup_settings(&self) {
        self.settings.popup();
    }

    /// The check has started: no second one until it ends.
    pub fn update_busy(&self) {
        self.update.busy();
    }

    /// The check has ended with `message`; `restart` offers the restart button.
    pub fn update_done(&self, message: &str, restart: bool) {
        self.update.done(message, restart);
    }

    /// Folders in the muted color; the open file and a drop target, highlighted.
    fn style_rows(&self, column: &gtk::TreeViewColumn, cell: &gtk::CellRendererText, arrow: &gtk::CellRendererPixbuf) {
        let (state, tree) = (self.state.clone(), self.tree.clone());
        CellLayoutExt::set_cell_data_func(
            column,
            arrow,
            Some(Box::new(move |_, cell, model, iter| {
                let scale = tree.scale_factor();
                if state.borrow().arrows.scale != scale {
                    let mut st = state.borrow_mut();
                    let muted = st.colors.muted.clone();
                    st.arrows = Arrows::new(&muted, scale);
                }
                let st = state.borrow();
                let dir = is_dir(model, iter);
                let surface = if !dir {
                    &st.arrows.blank
                } else if model.path(iter).is_some_and(|p| tree.row_expanded(&p)) {
                    &st.arrows.down
                } else {
                    &st.arrows.right
                };
                cell.set_property("surface", surface);
                // The highlight covers the arrow too, so it reads as one row.
                let path = rel_path(model, iter);
                let marked = highlighted(&st, &path, dir);
                let hovered = !marked && st.hover.as_deref() == Some(path.as_str());
                cell.set_property("cell-background-set", marked || hovered);
                if marked || hovered {
                    cell.set_property("cell-background", if marked { &st.colors.active } else { &st.colors.hover });
                }
            })),
        );
        let state = self.state.clone();
        CellLayoutExt::set_cell_data_func(
            column,
            cell,
            Some(Box::new(move |_, cell, model, iter| {
                let path: String = rel_path(model, iter);
                let is_dir: bool = is_dir(model, iter);
                let kind = Row::of(model, iter);
                let state = state.borrow();
                let c = &state.colors;
                let active = !is_dir && state.file.as_deref() == Some(path.as_str());
                let marked = highlighted(&state, &path, is_dir);
                let hovered = !marked && state.hover.as_deref() == Some(path.as_str());
                let fg = if marked {
                    Some(&c.accent)
                } else if kind.inert() {
                    Some(&c.faint)
                } else if is_dir {
                    Some(&c.muted)
                } else {
                    c.fg.as_ref()
                };
                cell.set_property("foreground-set", fg.is_some());
                if let Some(fg) = fg {
                    cell.set_property("foreground", fg);
                }
                cell.set_property("weight", if active || is_dir { 500 } else { 400 });
                cell.set_property("style-set", kind.inert());
                if kind.inert() {
                    cell.set_property("style", gtk::pango::Style::Italic);
                }
                cell.set_property("cell-background-set", marked || hovered);
                if marked || hovered {
                    cell.set_property("cell-background", if marked { &c.active } else { &c.hover });
                }
            })),
        );
    }

    fn connect_header(&self) {
        let s = self.clone();
        self.new_button.connect_clicked(move |_| s.new_or_pick(false));
        let s = self.clone();
        self.folder_button.connect_clicked(move |_| s.new_or_pick(true));
    }

    fn new_or_pick(&self, folder: bool) {
        if !self.state.borrow().has_vault {
            return self.send(SidebarMsg::PickVault);
        }
        let dir = self.state.borrow().file.as_deref().and_then(|f| f.rsplit_once('/')).map_or("", |(d, _)| d).to_string();
        self.new_note(&dir, folder);
    }

    fn new_note_at_menu(&self, folder: bool) {
        let dir = self.state.borrow_mut().menu_path.take();
        if let Some(dir) = dir {
            self.new_note(&dir, folder);
        }
    }

    fn connect_form(&self) {
        let s = self.clone();
        self.entry.connect_activate(move |entry| {
            let name = entry.text().to_string();
            let (dir, folder) = {
                let state = s.state.borrow();
                (state.new_dir.clone(), state.new_folder)
            };
            if folder {
                // Open the new folder and its parents once the tree shows it.
                let path = if dir.is_empty() { name.trim().to_string() } else { format!("{dir}/{}", name.trim()) };
                let mut state = s.state.borrow_mut();
                state.opened.extend(parents(&format!("{path}/x")));
                let open: Vec<String> = state.opened.iter().cloned().collect();
                drop(state);
                s.send(SidebarMsg::OpenFolders(open));
                s.send(SidebarMsg::CreateFolder { dir, name });
            } else {
                s.send(SidebarMsg::CreateNote { dir, name });
            }
        });
        let s = self.clone();
        self.entry.connect_key_press_event(move |_, event| {
            if event.keyval() == gdk::keys::constants::Escape {
                s.note_created();
                return Propagation::Stop;
            }
            Propagation::Proceed
        });
        let s = self.clone();
        self.entry.connect_focus_out_event(move |entry, _| {
            if entry.text().is_empty() {
                s.note_created();
            }
            Propagation::Proceed
        });
    }

    /// Clicks act on release, so that pressing to drag opens nothing.
    fn connect_clicks(&self) {
        let s = self.clone();
        self.tree.connect_button_press_event(move |tree, event| {
            let (x, y) = event.position();
            let hit = tree.path_at_pos(x as i32, y as i32);
            if event.button() == 3 {
                if let Some((Some(path), ..)) = hit {
                    s.open_menu(&path, event);
                }
                return Propagation::Stop;
            }
            if event.button() == 1 && event.event_type() == gdk::EventType::ButtonPress {
                s.state.borrow_mut().pressed = hit.and_then(|(path, column, ..)| {
                    let (path, column) = (path?, column?);
                    let iter = s.store.iter(&path)?;
                    // A file visor cannot open still answers, with a notice.
                    let clickable = Row::of(&s.store, &iter) != Row::Empty;
                    (clickable && x as i32 >= tree.cell_area(Some(&path), Some(&column)).x()).then_some(path)
                });
            }
            Propagation::Proceed
        });
        let s = self.clone();
        self.tree.connect_button_release_event(move |tree, event| {
            let pressed = s.state.borrow_mut().pressed.take();
            let (x, y) = event.position();
            if event.button() == 1
                && let Some(pressed) = pressed
                && let Some((Some(path), ..)) = tree.path_at_pos(x as i32, y as i32)
                && path == pressed
            {
                let state = event.state();
                let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
                s.activate(&path, ctrl, ctrl && state.contains(gdk::ModifierType::MOD1_MASK));
            }
            Propagation::Proceed
        });
        // Enter on the row under the keyboard cursor. Double clicks also come
        // here and are skipped: their first click already acted.
        let s = self.clone();
        self.tree.connect_row_activated(move |_, path, _| {
            if gtk::current_event().is_some_and(|e| e.event_type() == gdk::EventType::KeyPress) {
                s.activate(path, false, false);
            }
        });
        let s = self.clone();
        self.tree.connect_key_press_event(move |tree, event| {
            if event.keyval() != gdk::keys::constants::Delete {
                return Propagation::Proceed;
            }
            if let (Some(path), _) = tree.cursor()
                && let Some(iter) = s.store.iter(&path)
                && Row::of(&s.store, &iter) != Row::Empty
            {
                s.send(SidebarMsg::Trash { path: rel_path(&s.store, &iter) });
            }
            Propagation::Stop
        });
    }

    /// Opens a file, with Ctrl in a new tab and with Ctrl+Alt on the other side; opens or closes a folder.
    fn activate(&self, path: &gtk::TreePath, tab: bool, beside: bool) {
        let Some(iter) = self.store.iter(path) else { return };
        match Row::of(&self.store, &iter) {
            Row::Empty => return,
            Row::Other => {
                let path = rel_path(&self.store, &iter);
                self.notice_elsewhere(&cannot_open(&path), &path);
                return;
            }
            _ => {}
        }
        if is_dir(&self.store, &iter) {
            if self.tree.row_expanded(path) {
                self.tree.collapse_row(path);
            } else {
                self.tree.expand_row(path, false);
            }
        } else {
            self.send(SidebarMsg::Open { path: rel_path(&self.store, &iter), tab, beside });
        }
    }

    fn open_menu(&self, path: &gtk::TreePath, event: &gdk::EventButton) {
        let Some(iter) = self.store.iter(path) else { return };
        // The notice of an empty folder is not a file: the menu would act on the root.
        if Row::of(&self.store, &iter) == Row::Empty {
            return;
        }
        let is_dir: bool = is_dir(&self.store, &iter);
        let other = Row::of(&self.store, &iter) == Row::Other;
        for item in &self.folder_items {
            item.set_visible(is_dir);
        }
        self.elsewhere.set_visible(other);
        self.separator.set_visible(is_dir || other);
        self.state.borrow_mut().menu_path = Some(rel_path(&self.store, &iter));
        // The event is in the tree's bin window; the popover is positioned against the tree widget.
        let (x, y) = event.position();
        let (x, y) = self.tree.convert_bin_window_to_widget_coords(x as i32, y as i32);
        self.menu.set_pointing_to(&gdk::Rectangle::new(x, y, 1, 1));
        self.menu.popup();
    }

    /// Paints the row under the pointer. Only a change of row redraws the tree.
    fn connect_hover(&self) {
        self.tree.add_events(gdk::EventMask::POINTER_MOTION_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        let s = self.clone();
        self.tree.connect_motion_notify_event(move |tree, event| {
            let (x, y) = event.position();
            let hover = tree
                .path_at_pos(x as i32, y as i32)
                .and_then(|(path, ..)| s.store.iter(&path?))
                .filter(|iter| Row::of(&s.store, iter) != Row::Empty)
                .map(|iter| rel_path(&s.store, &iter));
            s.set_hover(hover);
            Propagation::Proceed
        });
        let s = self.clone();
        self.tree.connect_leave_notify_event(move |_, _| {
            s.set_hover(None);
            Propagation::Proceed
        });
    }

    fn set_hover(&self, hover: Option<String>) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let changed = state.hover != hover;
            state.hover = hover;
            changed
        };
        if changed {
            self.tree.queue_draw();
        }
    }

    /// Keeps `opened` in step with the tree. Opening a folder also reopens
    /// the subfolders that were open inside it, as GTK forgets them.
    fn connect_folders(&self) {
        let s = self.clone();
        self.tree.connect_row_expanded(move |tree, iter, _| {
            tree.queue_draw();
            let path: String = rel_path(&s.store, iter);
            let report = {
                let mut state = s.state.borrow_mut();
                // Folders that `current` or a rebuild expand are shown, not recorded.
                !state.restoring && state.opened.insert(path)
            };
            if report {
                s.report_open();
            }
            let Some(child) = s.store.iter_children(Some(iter)) else { return };
            loop {
                let open = is_dir(&s.store, &child) && s.state.borrow().opened.contains(&rel_path(&s.store, &child));
                if open && let Some(path) = s.store.path(&child) {
                    tree.expand_row(&path, false);
                }
                if !s.store.iter_next(&child) {
                    break;
                }
            }
        });
        let s = self.clone();
        self.tree.connect_row_collapsed(move |tree, iter, _| {
            tree.queue_draw();
            let path: String = rel_path(&s.store, iter);
            let report = {
                let mut state = s.state.borrow_mut();
                state.opened.remove(&path) && !state.restoring
            };
            if report {
                s.report_open();
            }
        });
    }

    fn report_open(&self) {
        let open: Vec<String> = self.state.borrow().opened.iter().cloned().collect();
        self.send(SidebarMsg::OpenFolders(open));
    }

    /// Dragging moves files and folders between folders. A dragged file can
    /// also be dropped on the pages, which the app marks while the drag lasts.
    fn connect_drag(&self) {
        let targets = [gtk::TargetEntry::new(DRAG_TARGET, gtk::TargetFlags::SAME_APP, 0)];
        self.tree.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &targets, gdk::DragAction::MOVE);
        self.tree.drag_dest_set(gtk::DestDefaults::empty(), &targets, gdk::DragAction::MOVE);
        let s = self.clone();
        self.tree.connect_drag_begin(move |tree, context| {
            let Some(path) = s.state.borrow_mut().pressed.take() else { return };
            let Some(iter) = s.store.iter(&path) else { return };
            let rel: String = rel_path(&s.store, &iter);
            if let Some(icon) = tree.create_row_drag_icon(&path) {
                context.drag_set_icon_surface(&icon);
            }
            s.state.borrow_mut().dragged = Some(rel.clone());
            if !is_dir(&s.store, &iter) {
                s.send(SidebarMsg::DragStart { path: rel });
            }
        });
        let s = self.clone();
        self.tree.connect_drag_motion(move |_, context, x, y, time| {
            if s.state.borrow().dragged.is_none() {
                context.drag_status(gdk::DragAction::empty(), time);
                return false;
            }
            let target = s.folder_at(x, y);
            s.set_drop_target(Some(target));
            context.drag_status(gdk::DragAction::MOVE, time);
            true
        });
        let s = self.clone();
        self.tree.connect_drag_leave(move |_, _, _| s.set_drop_target(None));
        let s = self.clone();
        self.tree.connect_drag_drop(move |_, context, x, y, time| {
            let dragged = s.state.borrow().dragged.clone();
            if let Some(path) = dragged {
                let (_, dir) = s.folder_at(x, y);
                s.send(SidebarMsg::Move { path, dir });
            }
            s.set_drop_target(None);
            context.drag_finish(true, false, time);
            true
        });
        let s = self.clone();
        self.tree.connect_drag_end(move |_, _| {
            s.state.borrow_mut().dragged = None;
            s.set_drop_target(None);
            s.send(SidebarMsg::DragEnd);
        });
    }

    /// The folder a drop at (x, y) goes into: the folder itself, the folder of
    /// a file, or the vault root on empty space. Also its row, if it has one.
    fn folder_at(&self, x: i32, y: i32) -> (Option<gtk::TreePath>, String) {
        let (bx, by) = self.tree.convert_widget_to_bin_window_coords(x, y);
        let Some((Some(path), ..)) = self.tree.path_at_pos(bx, by) else { return (None, String::new()) };
        let Some(iter) = self.store.iter(&path) else { return (None, String::new()) };
        let folder = if is_dir(&self.store, &iter) { Some(iter) } else { self.store.iter_parent(&iter) };
        match folder {
            Some(f) => (self.store.path(&f), rel_path(&self.store, &f)),
            None => (None, String::new()),
        }
    }

    /// Marks where a drop would go. A closed folder opens after a moment under the pointer.
    fn set_drop_target(&self, target: Option<(Option<gtk::TreePath>, String)>) {
        let dir = target.as_ref().map(|(_, d)| d.clone());
        let generation = {
            let mut state = self.state.borrow_mut();
            if state.drop_target == dir {
                return;
            }
            state.drop_target = dir.clone();
            state.drop_gen += 1;
            state.drop_gen
        };
        let root = self.scroll.style_context();
        if dir.as_deref() == Some("") { root.add_class("drop-root") } else { root.remove_class("drop-root") }
        self.tree.queue_draw();
        if let Some((Some(row), _)) = target
            && !self.tree.row_expanded(&row)
        {
            let s = self.clone();
            glib::timeout_add_local_once(Duration::from_millis(700), move || {
                if s.state.borrow().drop_gen == generation {
                    s.tree.expand_row(&row, false);
                }
            });
        }
    }

    /// Shows the vault list and a new tree, with the folders that were open.
    pub fn show(&self, vaults: &[PathBuf], current: Option<&Path>, tree: Option<Dir>, opened: &[String]) {
        {
            let mut state = self.state.borrow_mut();
            state.has_vault = current.is_some();
            state.file = None;
            state.opened = opened.iter().cloned().collect();
        }
        self.render_vaults(vaults, current);
        self.set_tree(tree);
    }

    fn render_vaults(&self, vaults: &[PathBuf], current: Option<&Path>) {
        let name = |v: &Path| v.file_name().map_or_else(|| v.display().to_string(), |n| n.to_string_lossy().into_owned());
        self.vault_name.set_text(&current.map_or_else(|| tr("Sin bóveda", "No vault").to_string(), name));
        for child in self.vault_list.children() {
            self.vault_list.remove(&child);
        }
        for vault in vaults {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let title = gtk::Label::new(Some(&name(vault)));
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            if Some(vault.as_path()) == current {
                title.style_context().add_class("current");
            }
            let full = gtk::Label::new(Some(&vault.display().to_string()));
            full.set_xalign(0.0);
            full.set_ellipsize(gtk::pango::EllipsizeMode::Start);
            full.set_max_width_chars(32);
            full.style_context().add_class("visor-muted");
            text.pack_start(&title, false, false, 0);
            text.pack_start(&full, false, false, 0);
            let switch = gtk::Button::new();
            switch.add(&text);
            let (s, path) = (self.clone(), vault.clone());
            switch.connect_clicked(move |_| {
                s.popover.popdown();
                s.send(SidebarMsg::SwitchVault { path: path.clone() });
            });
            let remove = gtk::Button::with_label("×");
            remove.set_tooltip_text(Some(tr("Quitar de la lista (no borra la carpeta)", "Remove from the list (does not delete the folder)")));
            let (s, path) = (self.clone(), vault.clone());
            remove.connect_clicked(move |_| s.send(SidebarMsg::RemoveVault { path: path.clone() }));
            row.pack_start(&switch, true, true, 0);
            row.pack_start(&remove, false, false, 0);
            self.vault_list.pack_start(&row, false, false, 0);
        }
        if !vaults.is_empty() {
            self.vault_list.pack_start(&gtk::Separator::new(gtk::Orientation::Horizontal), false, false, 4);
        }
        let pick = gtk::Button::new();
        let text = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let label = gtk::Label::new(Some(tr("Abrir carpeta…", "Open folder…")));
        label.set_xalign(0.0);
        let keys = gtk::Label::new(Some("Ctrl+O"));
        keys.style_context().add_class("visor-muted");
        text.pack_start(&label, true, true, 0);
        text.pack_start(&keys, false, false, 0);
        pick.add(&text);
        let s = self.clone();
        pick.connect_clicked(move |_| {
            s.popover.popdown();
            s.send(SidebarMsg::PickVault);
        });
        self.vault_list.pack_start(&pick, false, false, 0);
        self.vault_list.show_all();
    }

    /// Replaces the tree after a change on disk, keeping the open folders and the scroll.
    pub fn set_tree(&self, tree: Option<Dir>) {
        let scroll = self.scroll.vadjustment().value();
        let file = self.state.borrow().file.clone();
        // A folder shown only to reveal the current file is not in `opened`;
        // without this a save on disk would close it under the user.
        let file_shown = file.as_deref().and_then(|f| self.find(f)).is_some_and(|mut row| {
            row.up();
            row.depth() == 0 || self.tree.row_expanded(&row)
        });
        self.state.borrow_mut().restoring = true;
        self.store.clear();
        if let Some(tree) = &tree {
            let show_other = self.state.borrow().show_other;
            fill(&self.store, None, tree, show_other);
            // Opening the top folders reopens the ones inside them.
            if let Some(iter) = self.store.iter_first() {
                loop {
                    let open = is_dir(&self.store, &iter) && self.state.borrow().opened.contains(&rel_path(&self.store, &iter));
                    if open && let Some(row) = self.store.path(&iter) {
                        self.tree.expand_row(&row, false);
                    }
                    if !self.store.iter_next(&iter) {
                        break;
                    }
                }
            }
            if file_shown
                && let Some(mut row) = file.as_deref().and_then(|f| self.find(f))
                && row.up()
                && row.depth() > 0
            {
                self.tree.expand_to_path(&row);
            }
        }
        self.state.borrow_mut().restoring = false;
        self.current(file);
        let adjustment = self.scroll.vadjustment();
        glib::idle_add_local_once(move || adjustment.set_value(scroll));
    }

    /// Marks `file` as the one on screen, opening its folders and scrolling to it.
    pub fn current(&self, file: Option<String>) {
        let previous = std::mem::replace(&mut self.state.borrow_mut().file, file.clone());
        self.tree.queue_draw();
        let Some(file) = file else { return };
        let Some(path) = self.find(&file) else { return };
        let mut parent = path.clone();
        if parent.up() && parent.depth() > 0 && reveal_on_arrival(previous.as_deref(), &file) {
            // Showing the folders is not the user opening them: nothing is recorded.
            let was = std::mem::replace(&mut self.state.borrow_mut().restoring, true);
            self.tree.expand_to_path(&parent);
            self.state.borrow_mut().restoring = was;
        }
        self.tree.scroll_to_cell(Some(&path), None::<&gtk::TreeViewColumn>, false, 0.0, 0.0);
    }

    fn find(&self, rel: &str) -> Option<gtk::TreePath> {
        let mut found = None;
        self.store.foreach(|model, path, iter| {
            let hit = rel_path(model, iter) == rel;
            if hit {
                found = Some(path.clone());
            }
            hit
        });
        found
    }

    /// Asks for the name of a new note, or folder, inside `dir`.
    pub fn new_note(&self, dir: &str, folder: bool) {
        {
            let mut state = self.state.borrow_mut();
            state.new_dir = dir.to_string();
            state.new_folder = folder;
        }
        let place = if dir.is_empty() { tr("la raíz de la bóveda", "the vault root") } else { dir };
        let label = if folder { trf!("Nueva carpeta en {place}", "New folder in {place}") } else { trf!("Nueva nota en {place}", "New note in {place}") };
        self.form_label.set_text(&label);
        self.entry.set_placeholder_text(Some(if folder { tr("Nombre de la carpeta", "Folder name") } else { tr("Nombre de la nota", "Note name") }));
        self.entry.set_text("");
        self.form_error.hide();
        self.form.show();
        self.entry.grab_focus();
    }

    pub fn note_created(&self) {
        self.form.hide();
        self.form_error.hide();
    }

    pub fn note_error(&self, message: &str) {
        self.form_error.set_text(message);
        self.form_error.show();
        self.entry.grab_focus();
    }

    /// A short message under the tree, which goes away by itself.
    pub fn notice(&self, message: &str, error: bool) {
        self.show_notice(message, error, None);
    }

    /// A notice about a file visor cannot show, with a button to open it elsewhere.
    fn notice_elsewhere(&self, message: &str, path: &str) {
        self.show_notice(message, false, Some(path));
    }

    fn show_notice(&self, message: &str, error: bool, elsewhere: Option<&str>) {
        self.notice_text.set_text(message);
        let style = self.notice.style_context();
        if error { style.add_class("error") } else { style.remove_class("error") }
        self.notice_open.set_visible(elsewhere.is_some());
        self.notice.show();
        let generation = {
            let mut state = self.state.borrow_mut();
            state.notice_gen += 1;
            state.notice_path = elsewhere.map(str::to_string);
            state.notice_gen
        };
        let s = self.clone();
        glib::timeout_add_local_once(Duration::from_secs(if error { 8 } else { 4 }), move || {
            if s.state.borrow().notice_gen == generation {
                s.notice.hide();
            }
        });
    }

    pub fn set_theme(&self, theme: Option<&Theme>) {
        let mut st = self.state.borrow_mut();
        let st = &mut *st;
        st.colors = Colors::new(theme);
        st.arrows = Arrows::new(&st.colors.muted, st.arrows.scale);
        self.tree.queue_draw();
    }

    pub fn css(theme: &Theme) -> String {
        format!(
            ".visor-sidebar, .visor-sidebar treeview.view {{ background: {side}; color: {fg}; }}
.visor-sidebar .drop-root treeview.view {{ background: {active}; }}
.visor-side-head {{ padding: 10px 8px 6px; }}
.visor-side-foot {{ padding: 6px 8px 8px; border-top: 1px solid {line}; }}
.visor-side-foot button {{ color: {muted}; }}
.visor-sidebar button, .visor-vaults button {{ background: none; border: none; box-shadow: none;
  color: {fg}; border-radius: 6px; padding: 4px 8px; min-height: 0; }}
.visor-sidebar button:hover, .visor-vaults button:hover {{ background: {hover}; }}
.visor-side-head button {{ color: {muted}; }}
.visor-side-head button.visor-vault {{ color: {fg}; font-weight: 600; }}
.visor-vaults {{ background: {bg}; color: {fg}; border: 1px solid {line}; border-radius: 10px; padding: 4px; }}
.visor-vaults .current {{ color: {accent}; font-weight: 600; }}
.visor-vaults separator {{ background: {line}; min-height: 1px; }}
.visor-muted {{ color: {muted}; font-size: smaller; }}
.visor-new {{ margin: 0 8px 6px; padding: 8px; border-radius: 14px; background: {hover}; }}
.visor-new > label {{ color: {muted}; font-size: smaller; }}
.visor-new entry {{ background: {bg}; color: {fg}; border: 1px solid {line}; border-radius: 6px;
  box-shadow: none; min-height: 26px; padding: 0 8px; caret-color: {fg}; }}
.visor-new entry:focus {{ border-color: {accent}; }}
.visor-new .error, .visor-notice.error {{ color: {red}; }}
.visor-notice {{ margin: 0 8px 8px; padding: 8px 10px; border-radius: 8px; background: {hover}; font-size: smaller; }}
.visor-sidebar .visor-notice button {{ background: {bg}; border: 1px solid {line}; padding: 3px 8px; }}
.visor-sidebar .visor-notice button:hover {{ background: {active}; }}
.visor-menu {{ background: {bg}; color: {fg}; border: 1px solid {line}; border-radius: 10px; padding: 4px; }}
.visor-menu button {{ background: none; border: none; box-shadow: none; color: {fg}; border-radius: 6px; padding: 6px 8px; min-height: 0; }}
.visor-menu button:hover {{ background: {hover}; }}
.visor-menu .visor-accel {{ color: {muted}; margin-left: 24px; }}
.visor-menu separator {{ background: {line}; min-height: 1px; margin: 4px 0; }}
.visor-settings {{ background: {bg}; color: {fg}; border: 1px solid {line}; border-radius: 10px; padding: 10px; }}
.visor-settings button {{ background: none; border: none; box-shadow: none; color: {fg}; border-radius: 6px; padding: 4px 8px; min-height: 0; }}
.visor-settings button:hover {{ background: {hover}; }}
.visor-settings button:checked {{ background: {active}; color: {fg}; font-weight: 600; }}
.visor-settings .visor-zoom {{ min-width: 4em; }}
.visor-shortcuts {{ background: {bg}; color: {fg}; }}
.visor-shortcuts .visor-shortcut-key {{ font-weight: 600; }}",
            bg = theme.bg,
            fg = theme.fg,
            muted = theme.muted,
            accent = theme.accent,
            red = theme.palette.iter().find(|(n, _)| *n == "red").map_or("#d33", |(_, c)| c.as_str()),
            side = theme.tone(4.0),
            line = theme.tone(14.0),
            hover = theme.tone(9.0),
            active = theme.accent_tone(20.0),
        )
    }
}

/// The buttons of one settings row: `None` is the system's choice.
type ChoiceButtons = Vec<(Option<&'static str>, gtk::ToggleButton)>;

/// A row of toggle buttons where one choice is always on. Choosing one sends
/// `action` with that choice; `Sidebar::set_settings` fills the row in without sending.
fn choice_row(
    choices: &[(Option<&'static str>, &'static str)],
    syncing: &Rc<Cell<bool>>,
    proxy: &EventLoopProxy<UserEvent>,
    action: fn(Option<String>) -> Action,
) -> (gtk::Box, ChoiceButtons) {
    let buttons: ChoiceButtons = choices
        .iter()
        .map(|&(choice, label)| (choice, gtk::ToggleButton::with_label(label)))
        .collect();
    let all: Vec<gtk::ToggleButton> = buttons.iter().map(|(_, b)| b.clone()).collect();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for (choice, button) in &buttons {
        row.pack_start(button, true, true, 0);
        let (all, syncing, choice) = (all.clone(), syncing.clone(), *choice);
        let proxy = proxy.clone();
        button.connect_toggled(move |b| {
            if syncing.get() {
                return;
            }
            // Clicking the active choice would clear it, and one choice always stays on.
            if !b.is_active() {
                syncing.set(true);
                b.set_active(true);
                syncing.set(false);
                return;
            }
            syncing.set(true);
            for other in all.iter().filter(|o| *o != b) {
                other.set_active(false);
            }
            syncing.set(false);
            let _ = proxy.send_event(UserEvent::Action(action(choice.map(String::from))));
        });
    }
    (row, buttons)
}

/// The update part of the settings: a check now, the result of the last one,
/// and a restart once a new version is on disk.
#[derive(Clone)]
struct UpdateBox {
    area: gtk::Box,
    check: gtk::Button,
    result: gtk::Label,
    restart: gtk::Button,
}

impl UpdateBox {
    fn new(proxy: &EventLoopProxy<UserEvent>) -> UpdateBox {
        let area = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let check = gtk::Button::with_label(tr("Buscar actualización ahora", "Check for update now"));
        check.set_tooltip_text(Some(tr("Las copias compiladas en target/ no se reemplazan.", "Compiled copies in target/ are not replaced.")));
        let result = gtk::Label::new(None);
        result.set_xalign(0.0);
        result.set_line_wrap(true);
        result.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        result.set_no_show_all(true);
        let restart = gtk::Button::with_label(tr("Reiniciar ahora", "Restart now"));
        restart.set_no_show_all(true);
        let p = proxy.clone();
        check.connect_clicked(move |_| {
            let _ = p.send_event(UserEvent::Action(Action::CheckUpdates));
        });
        let p = proxy.clone();
        restart.connect_clicked(move |_| {
            let _ = p.send_event(UserEvent::Action(Action::Restart));
        });
        area.pack_start(&check, false, false, 0);
        area.pack_start(&result, false, false, 0);
        area.pack_start(&restart, false, false, 0);
        UpdateBox { area, check, result, restart }
    }

    fn busy(&self) {
        self.check.set_sensitive(false);
        self.restart.hide();
        self.result.set_text(tr("Buscando…", "Checking…"));
        self.result.show();
    }

    fn done(&self, message: &str, restart: bool) {
        self.check.set_sensitive(true);
        self.result.set_text(message);
        self.result.show();
        if restart { self.restart.show() } else { self.restart.hide() }
    }
}

/// The gear's popover: a language and a theme choice, the text size, the update switch and the
/// update check. Its controls send actions; `Sidebar::set_settings` fills them in silently.
fn settings_popover(
    gear: &gtk::MenuButton,
    proxy: &EventLoopProxy<UserEvent>,
    syncing: &Rc<Cell<bool>>,
    update: &UpdateBox,
) -> (gtk::Popover, ChoiceButtons, ChoiceButtons, gtk::Box, gtk::Label, gtk::CheckButton, gtk::CheckButton, gtk::ComboBoxText) {
    let caption = |text: &str| {
        let label = gtk::Label::new(Some(text));
        label.set_xalign(0.0);
        label.style_context().add_class("visor-muted");
        label
    };

    let (theme_row, themes) = choice_row(
        &[(None, tr("Sistema", "System")), (Some("claro"), tr("Claro", "Light")), (Some("oscuro"), tr("Oscuro", "Dark"))],
        syncing,
        proxy,
        Action::SetTheme,
    );
    let (language_row, languages) = choice_row(
        &[(None, tr("Sistema", "System")), (Some("es"), "Español"), (Some("en"), "English")],
        syncing,
        proxy,
        Action::SetLanguage,
    );
    // The language only changes on restart, so this shows while the choice differs from the one in use.
    let language_restart = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let pending = gtk::Label::new(Some(tr("Se aplicará al reiniciar visor.", "Takes effect when visor restarts.")));
    pending.set_xalign(0.0);
    pending.set_line_wrap(true);
    pending.set_max_width_chars(20);
    pending.style_context().add_class("visor-muted");
    let restart = gtk::Button::with_label(tr("Reiniciar ahora", "Restart now"));
    let p = proxy.clone();
    restart.connect_clicked(move |_| {
        let _ = p.send_event(UserEvent::Action(Action::Restart));
    });
    language_restart.pack_start(&pending, true, true, 0);
    language_restart.pack_start(&restart, false, false, 0);
    pending.show();
    restart.show();
    language_restart.set_no_show_all(true);

    let zoom_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let smaller = gtk::Button::with_label("−");
    smaller.set_tooltip_text(Some(tr("Reducir (Ctrl+-)", "Zoom out (Ctrl+-)")));
    let zoom_label = gtk::Label::new(None);
    zoom_label.style_context().add_class("visor-zoom");
    let reset = gtk::Button::new();
    reset.set_relief(gtk::ReliefStyle::None);
    reset.add(&zoom_label);
    reset.set_tooltip_text(Some(tr("Restablecer (Ctrl+0)", "Reset (Ctrl+0)")));
    let bigger = gtk::Button::with_label("+");
    bigger.set_tooltip_text(Some(tr("Aumentar (Ctrl++)", "Zoom in (Ctrl++)")));
    zoom_row.pack_start(&smaller, false, false, 0);
    zoom_row.pack_start(&reset, true, true, 0);
    zoom_row.pack_start(&bigger, false, false, 0);
    let p = proxy.clone();
    smaller.connect_clicked(move |_| {
        let _ = p.send_event(UserEvent::Action(Action::Zoom(-1)));
    });
    let p = proxy.clone();
    reset.connect_clicked(move |_| {
        let _ = p.send_event(UserEvent::Action(Action::Zoom(0)));
    });
    let p = proxy.clone();
    bigger.connect_clicked(move |_| {
        let _ = p.send_event(UserEvent::Action(Action::Zoom(1)));
    });

    let auto_update = gtk::CheckButton::with_label(tr("Buscar e instalar actualizaciones", "Check for and install updates"));
    let (syncing_flag, p) = (syncing.clone(), proxy.clone());
    auto_update.connect_toggled(move |c| {
        if !syncing_flag.get() {
            let _ = p.send_event(UserEvent::Action(Action::SetAutoUpdate(c.is_active())));
        }
    });

    let show_other = gtk::CheckButton::with_label(tr("Mostrar archivos que visor no abre", "Show files visor does not open"));
    let (syncing_flag, p) = (syncing.clone(), proxy.clone());
    show_other.connect_toggled(move |c| {
        if !syncing_flag.get() {
            let _ = p.send_event(UserEvent::Action(Action::SetShowOtherFiles(c.is_active())));
        }
    });

    let search_engine = gtk::ComboBoxText::new();
    for engine in Engine::ALL {
        search_engine.append(Some(engine.id()), engine.label());
    }
    let (syncing_flag, p) = (syncing.clone(), proxy.clone());
    search_engine.connect_changed(move |c| {
        if !syncing_flag.get() {
            let engine = Engine::from_id(c.active_id().as_deref());
            let _ = p.send_event(UserEvent::Action(Action::SetSearchEngine(engine)));
        }
    });
    let search_note = gtk::Label::new(Some(tr("Lo escrito en la barra que no sea una dirección se envía al buscador.", "What you type in the bar that is not an address goes to the search engine.")));
    search_note.set_xalign(0.0);
    search_note.set_line_wrap(true);
    search_note.set_max_width_chars(32);
    search_note.style_context().add_class("visor-muted");

    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.pack_start(&caption(tr("Idioma", "Language")), false, false, 0);
    column.pack_start(&language_row, false, false, 0);
    column.pack_start(&language_restart, false, false, 0);
    column.pack_start(&caption(tr("Tema", "Theme")), false, false, 0);
    column.pack_start(&theme_row, false, false, 0);
    column.pack_start(&caption(tr("Tamaño del texto", "Text size")), false, false, 0);
    column.pack_start(&zoom_row, false, false, 0);
    column.pack_start(&show_other, false, false, 4);
    column.pack_start(&auto_update, false, false, 4);
    column.pack_start(&caption(tr("Buscador", "Search engine")), false, false, 0);
    column.pack_start(&search_engine, false, false, 0);
    column.pack_start(&search_note, false, false, 0);
    column.pack_start(&update.area, false, false, 4);
    column.show_all();

    let settings = gtk::Popover::new(Some(gear));
    settings.style_context().add_class("visor-settings");
    settings.set_position(gtk::PositionType::Top);
    settings.add(&column);
    (settings, themes, languages, language_restart, zoom_label, auto_update, show_other, search_engine)
}

/// The open file and a drop target get the accent color and a background.
fn highlighted(state: &State, path: &str, is_dir: bool) -> bool {
    let active = !is_dir && state.file.as_deref() == Some(path);
    let drop = is_dir && state.drop_target.as_deref() == Some(path);
    active || drop
}

fn rel_path(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter) -> String {
    model.value(iter, PATH as i32).get().unwrap_or_default()
}

fn is_dir(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter) -> bool {
    model.value(iter, IS_DIR as i32).get().unwrap_or_default()
}

/// What a row stands for. Folders also keep `IS_DIR`, which the arrows read.
#[derive(Clone, Copy, PartialEq)]
enum Row {
    Folder,
    Page,
    Image,
    /// A file the viewer cannot show. Listed so the folder is complete, but inert.
    Other,
    /// The notice inside an empty folder; it gives the folder an arrow.
    Empty,
}

impl Row {
    const ALL: [Row; 5] = [Row::Folder, Row::Page, Row::Image, Row::Other, Row::Empty];

    fn of(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter) -> Row {
        let code: u32 = model.value(iter, KIND as i32).get().unwrap_or_default();
        Self::ALL.get(code as usize).copied().unwrap_or(Row::Other)
    }

    /// Rows that open nothing and cannot be dragged. A click on a file
    /// visor cannot open only shows a notice.
    fn inert(self) -> bool {
        matches!(self, Row::Other | Row::Empty)
    }
}

fn fill(store: &gtk::TreeStore, parent: Option<&gtk::TreeIter>, dir: &Dir, show_other: bool) {
    for sub in &dir.dirs {
        let iter = store.insert_with_values(
            parent,
            None,
            &[(NAME, &sub.name), (PATH, &sub.path), (IS_DIR, &true), (KIND, &(Row::Folder as u32)), (TIP, &"")],
        );
        if let Some(text) = placeholder(sub, show_other) {
            store.insert_with_values(
                Some(&iter),
                None,
                &[(NAME, &text), (PATH, &""), (IS_DIR, &false), (KIND, &(Row::Empty as u32)), (TIP, &"")],
            );
        }
        fill(store, Some(&iter), sub, show_other);
    }
    for file in dir.files.iter().filter(|f| show_other || f.kind != vault::Kind::Other) {
        let tip = glib::markup_escape_text(&file.path);
        let kind = match file.kind {
            vault::Kind::Page => Row::Page,
            vault::Kind::Image => Row::Image,
            vault::Kind::Other => Row::Other,
        };
        store.insert_with_values(
            parent,
            None,
            &[(NAME, &file.name), (PATH, &file.path), (IS_DIR, &false), (KIND, &(kind as u32)), (TIP, &tip.as_str())],
        );
    }
}

/// "a/b/c" → ["a", "a/b"].
fn parents(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).map(|i| parts[..i].join("/")).collect()
}

/// The line inside a folder that would otherwise show nothing, and so get no arrow.
fn placeholder(dir: &Dir, show_other: bool) -> Option<&'static str> {
    if !dir.dirs.is_empty() {
        None
    } else if dir.files.is_empty() {
        Some(tr("Carpeta vacía", "Empty folder"))
    } else if !show_other && dir.files.iter().all(|f| f.kind == vault::Kind::Other) {
        Some(tr("Sin archivos que visor abra", "No files that visor opens"))
    } else {
        None
    }
}

fn cannot_open(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => trf!("visor no abre archivos .{}.", "visor does not open .{} files.", ext.to_lowercase()),
        _ => tr("visor no abre este archivo.", "visor does not open this file.").into(),
    }
}

/// What the notice says when the system has no program for a file.
pub fn no_application(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    trf!("No hay ninguna aplicación para abrir «{name}».", "No application can open «{name}».")
}

/// Whether showing `next` should open its folders: only when the file on
/// screen changes. Otherwise a folder the user closed would reopen on the next
/// focus change or tree rebuild.
fn reveal_on_arrival(previous: Option<&str>, next: &str) -> bool {
    previous != Some(next)
}

#[cfg(test)]
mod tests {
    use super::{cannot_open, no_application, parents, placeholder, reveal_on_arrival};
    use crate::vault::{Dir, File, Kind};

    #[test]
    fn lists_parent_folders() {
        assert_eq!(parents("a/b/c"), ["a", "a/b"]);
        assert!(parents("nota").is_empty());
    }

    #[test]
    fn names_the_kind_of_file_visor_cannot_open() {
        assert_eq!(cannot_open("datos/Libro.XLSX"), "visor no abre archivos .xlsx.");
        assert_eq!(cannot_open("Makefile"), "visor no abre este archivo.");
        assert_eq!(cannot_open(".env"), "visor no abre este archivo.");
    }

    #[test]
    fn names_the_file_no_program_opens() {
        assert_eq!(no_application("datos/Libro.xlsx"), "No hay ninguna aplicación para abrir «Libro.xlsx».");
        assert_eq!(no_application("Makefile"), "No hay ninguna aplicación para abrir «Makefile».");
    }

    #[test]
    fn says_why_a_folder_shows_nothing() {
        let file = |kind| File { name: "x".into(), path: "d/x".into(), kind };
        let dir = |files| Dir { name: "d".into(), path: "d".into(), dirs: vec![], files };
        assert_eq!(placeholder(&dir(vec![]), true), Some("Carpeta vacía"));
        assert_eq!(placeholder(&dir(vec![file(Kind::Other)]), true), None);
        assert_eq!(placeholder(&dir(vec![file(Kind::Other)]), false), Some("Sin archivos que visor abra"));
        assert_eq!(placeholder(&dir(vec![file(Kind::Other), file(Kind::Image)]), false), None);
    }

    #[test]
    fn reveals_only_a_new_file() {
        assert!(reveal_on_arrival(None, "a/b.md"));
        assert!(reveal_on_arrival(Some("x.md"), "a/b.md"));
        assert!(!reveal_on_arrival(Some("a/b.md"), "a/b.md"));
    }
}
