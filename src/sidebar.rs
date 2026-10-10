//! The file panel, in native GTK. As a WebKit view it cost a web process of
//! its own (~65–100 MB) to list files. See docs/decisiones.md.
use crate::app::UserEvent;
use crate::icons::{draw_chevron, draw_chevron_bold, draw_new_folder, draw_plus, drawn_icon, icon_button};
use crate::theme::Theme;
use crate::vault::Dir;
use gtk::cairo::{Context, Format, ImageSurface};
use gtk::gdk;
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use tao::event_loop::EventLoopProxy;

const NAME: u32 = 0;
const PATH: u32 = 1;
const IS_DIR: u32 = 2;
const TIP: u32 = 3;
const DRAG_TARGET: &str = "application/x-visor-path";

/// What the person did in the panel. Paths are relative to the vault and
/// unchecked: Rust validates each one before acting.
pub enum SidebarMsg {
    PickVault,
    SwitchVault { path: PathBuf },
    RemoveVault { path: PathBuf },
    Open { path: String, tab: bool, beside: bool },
    CreateNote { dir: String, name: String },
    CreateFolder { dir: String, name: String },
    Trash { path: String },
    Move { path: String, dir: String },
    /// A file started being dragged; the pages can take it while it lasts.
    DragStart { path: String },
    DragEnd,
    /// The folders now open in the tree, to restore them next time.
    OpenFolders(Vec<String>),
}

struct Colors {
    fg: Option<String>,
    muted: String,
    accent: String,
    active: String,
}

impl Colors {
    fn new(theme: Option<&Theme>) -> Colors {
        match theme {
            Some(t) => Colors { fg: Some(t.fg.clone()), muted: t.muted.clone(), accent: t.accent.clone(), active: t.accent_tone(20.0) },
            None => Colors { fg: None, muted: "#6e6e76".into(), accent: "#3a63c7".into(), active: "#d8e0f4".into() },
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
    /// The row the context menu was opened on.
    menu_path: Option<String>,
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
    form: gtk::Box,
    form_label: gtk::Label,
    entry: gtk::Entry,
    form_error: gtk::Label,
    scroll: gtk::ScrolledWindow,
    tree: gtk::TreeView,
    store: gtk::TreeStore,
    menu: gtk::Popover,
    folder_items: Vec<gtk::Widget>,
    notice: gtk::Label,
    state: Rc<RefCell<State>>,
    proxy: EventLoopProxy<UserEvent>,
}

impl Sidebar {
    pub fn new(proxy: &EventLoopProxy<UserEvent>) -> Sidebar {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.style_context().add_class("visor-sidebar");

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        head.style_context().add_class("visor-side-head");
        let vault_name = gtk::Label::new(Some("Sin bóveda"));
        vault_name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        vault_name.set_xalign(0.0);
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        inner.pack_start(&vault_name, true, true, 0);
        inner.pack_start(&drawn_icon(10, draw_chevron_bold), false, false, 0);
        let vault_button = gtk::MenuButton::new();
        vault_button.add(&inner);
        vault_button.set_tooltip_text(Some("Cambiar de bóveda"));
        vault_button.style_context().add_class("visor-vault");
        let popover = gtk::Popover::new(Some(&vault_button));
        popover.style_context().add_class("visor-vaults");
        let vault_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        popover.add(&vault_list);
        vault_button.set_popover(Some(&popover));
        let new_button = icon_button("Nueva nota (Ctrl+N)", draw_plus);
        let folder_button = icon_button("Nueva carpeta", draw_new_folder);
        head.pack_start(&vault_button, true, true, 0);
        head.pack_start(&new_button, false, false, 0);
        head.pack_start(&folder_button, false, false, 0);

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
        let note_here = menu_button("Nueva nota aquí", "");
        let folder_here = menu_button("Nueva carpeta aquí", "");
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        let trash = menu_button("Mover a la papelera", "Supr");
        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for item in [note_here.clone().upcast::<gtk::Widget>(), folder_here.clone().upcast(), separator.clone().upcast(), trash.clone().upcast()] {
            menu_box.pack_start(&item, false, false, 0);
        }
        menu_box.show_all();
        menu.add(&menu_box);

        let notice = gtk::Label::new(None);
        notice.set_xalign(0.0);
        notice.set_line_wrap(true);
        notice.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        notice.style_context().add_class("visor-notice");
        notice.set_no_show_all(true);

        root.pack_start(&head, false, false, 0);
        root.pack_start(&form, false, false, 0);
        root.pack_start(&scroll, true, true, 0);
        root.pack_start(&notice, false, false, 0);
        root.show_all();

        let sidebar = Sidebar {
            root,
            vault_name,
            popover,
            vault_list,
            new_button,
            folder_button,
            form,
            form_label,
            entry,
            form_error,
            scroll,
            tree,
            store,
            menu,
            folder_items: vec![note_here.clone().upcast(), folder_here.clone().upcast(), separator.upcast()],
            notice,
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
                menu_path: None,
                colors: Colors::new(None),
                arrows: Arrows::new("#6e6e76", scale),
            })),
            proxy: proxy.clone(),
        };
        sidebar.style_rows(&column, &cell, &arrow);
        sidebar.connect_header();
        sidebar.connect_form();
        sidebar.connect_clicks();
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
        sidebar
    }

    fn send(&self, msg: SidebarMsg) {
        let _ = self.proxy.send_event(UserEvent::Sidebar(msg));
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
                let marked = highlighted(&st, &rel_path(model, iter), dir);
                cell.set_property("cell-background-set", marked);
                if marked {
                    cell.set_property("cell-background", &st.colors.active);
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
                let state = state.borrow();
                let c = &state.colors;
                let active = !is_dir && state.file.as_deref() == Some(path.as_str());
                let marked = highlighted(&state, &path, is_dir);
                let fg = if marked { Some(&c.accent) } else if is_dir { Some(&c.muted) } else { c.fg.as_ref() };
                cell.set_property("foreground-set", fg.is_some());
                if let Some(fg) = fg {
                    cell.set_property("foreground", fg);
                }
                cell.set_property("weight", if active || is_dir { 500 } else { 400 });
                cell.set_property("cell-background-set", marked);
                if marked {
                    cell.set_property("cell-background", &c.active);
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
                    (x as i32 >= tree.cell_area(Some(&path), Some(&column)).x()).then_some(path)
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
            {
                s.send(SidebarMsg::Trash { path: rel_path(&s.store, &iter) });
            }
            Propagation::Stop
        });
    }

    /// Opens a file, with Ctrl in a new tab and with Ctrl+Alt on the other side; opens or closes a folder.
    fn activate(&self, path: &gtk::TreePath, tab: bool, beside: bool) {
        let Some(iter) = self.store.iter(path) else { return };
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
        let is_dir: bool = is_dir(&self.store, &iter);
        for item in &self.folder_items {
            item.set_visible(is_dir);
        }
        self.state.borrow_mut().menu_path = Some(rel_path(&self.store, &iter));
        // The event is in the tree's bin window; the popover is positioned against the tree widget.
        let (x, y) = event.position();
        let (x, y) = self.tree.convert_bin_window_to_widget_coords(x as i32, y as i32);
        self.menu.set_pointing_to(&gdk::Rectangle::new(x, y, 1, 1));
        self.menu.popup();
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
                state.opened.insert(path) && !state.restoring
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
        self.vault_name.set_text(&current.map_or_else(|| "Sin bóveda".to_string(), name));
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
            remove.set_tooltip_text(Some("Quitar de la lista (no borra la carpeta)"));
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
        let label = gtk::Label::new(Some("Abrir carpeta…"));
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
        self.state.borrow_mut().restoring = true;
        self.store.clear();
        if let Some(tree) = &tree {
            fill(&self.store, None, tree);
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
        }
        self.state.borrow_mut().restoring = false;
        let file = self.state.borrow().file.clone();
        self.current(file);
        let adjustment = self.scroll.vadjustment();
        glib::idle_add_local_once(move || adjustment.set_value(scroll));
    }

    /// Marks `file` as the one on screen, opening its folders and scrolling to it.
    pub fn current(&self, file: Option<String>) {
        self.state.borrow_mut().file = file.clone();
        self.tree.queue_draw();
        let Some(file) = file else { return };
        let Some(path) = self.find(&file) else { return };
        let mut parent = path.clone();
        if parent.up() && parent.depth() > 0 {
            self.tree.expand_to_path(&parent);
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
        let place = if dir.is_empty() { "la raíz de la bóveda" } else { dir };
        self.form_label.set_text(&format!("{} {place}", if folder { "Nueva carpeta en" } else { "Nueva nota en" }));
        self.entry.set_placeholder_text(Some(if folder { "Nombre de la carpeta" } else { "Nombre de la nota" }));
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
        self.notice.set_text(message);
        let style = self.notice.style_context();
        if error { style.add_class("error") } else { style.remove_class("error") }
        self.notice.show();
        let generation = {
            let mut state = self.state.borrow_mut();
            state.notice_gen += 1;
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
.visor-menu {{ background: {bg}; color: {fg}; border: 1px solid {line}; border-radius: 10px; padding: 4px; }}
.visor-menu button {{ background: none; border: none; box-shadow: none; color: {fg}; border-radius: 6px; padding: 6px 8px; min-height: 0; }}
.visor-menu button:hover {{ background: {hover}; }}
.visor-menu .visor-accel {{ color: {muted}; margin-left: 24px; }}
.visor-menu separator {{ background: {line}; min-height: 1px; margin: 4px 0; }}",
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

fn fill(store: &gtk::TreeStore, parent: Option<&gtk::TreeIter>, dir: &Dir) {
    for sub in &dir.dirs {
        let iter = store.insert_with_values(parent, None, &[(NAME, &sub.name), (PATH, &sub.path), (IS_DIR, &true), (TIP, &"")]);
        fill(store, Some(&iter), sub);
    }
    for file in &dir.files {
        let tip = glib::markup_escape_text(&file.path);
        store.insert_with_values(parent, None, &[(NAME, &file.name), (PATH, &file.path), (IS_DIR, &false), (TIP, &tip.as_str())]);
    }
}

/// "a/b/c" → ["a", "a/b"].
fn parents(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).map(|i| parts[..i].join("/")).collect()
}

#[cfg(test)]
mod tests {
    use super::parents;

    #[test]
    fn lists_parent_folders() {
        assert_eq!(parents("a/b/c"), ["a", "a/b"]);
        assert!(parents("nota").is_empty());
    }
}
