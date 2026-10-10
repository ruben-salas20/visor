//! Line icons drawn with cairo in the text color. Theme icons go through a
//! sandboxed SVG loader (glycin) that costs ~6.5 MB and ~50 ms on most cold
//! starts, so visor draws its own. See docs/decisiones.md.
use gtk::glib::Propagation;
use gtk::prelude::*;
use std::f64::consts::PI;

pub fn drawn_icon(size: i32, draw: fn(&gtk::cairo::Context)) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.set_valign(gtk::Align::Center);
    area.connect_draw(move |area, cr| {
        let style = area.style_context();
        let c = style.color(style.state());
        cr.set_source_rgba(c.red(), c.green(), c.blue(), c.alpha());
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        // Save and restore around each draw so a function may scale the context.
        let _ = cr.save();
        draw(cr);
        let _ = cr.stroke();
        let _ = cr.restore();
        Propagation::Stop
    });
    area
}

pub fn icon_button(tip: &str, draw: fn(&gtk::cairo::Context)) -> gtk::Button {
    let b = gtk::Button::new();
    b.add(&drawn_icon(16, draw));
    b.set_tooltip_text(Some(tip));
    b
}

pub fn draw_plus(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    cr.move_to(8.0, 3.0);
    cr.line_to(8.0, 13.0);
    cr.move_to(3.0, 8.0);
    cr.line_to(13.0, 8.0);
}

pub fn draw_new_folder(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    cr.move_to(1.75, 3.25);
    for (x, y) in [(1.75, 12.75), (14.25, 12.75), (14.25, 5.25), (7.75, 5.25), (6.25, 3.25)] {
        cr.line_to(x, y);
    }
    cr.close_path();
    cr.move_to(8.0, 7.0);
    cr.line_to(8.0, 11.0);
    cr.move_to(6.0, 9.0);
    cr.line_to(10.0, 9.0);
}

pub fn draw_chevron(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    cr.move_to(2.0, 3.5);
    cr.line_to(5.0, 6.5);
    cr.line_to(8.0, 3.5);
}

/// Same chevron, heavier stroke, for use next to semibold (600) text.
pub fn draw_chevron_bold(cr: &gtk::cairo::Context) {
    cr.set_line_width(2.0);
    cr.move_to(2.0, 3.5);
    cr.line_to(5.0, 6.5);
    cr.line_to(8.0, 3.5);
}

pub fn draw_back(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    cr.move_to(13.0, 8.0);
    cr.line_to(3.0, 8.0);
    cr.move_to(7.5, 3.5);
    cr.line_to(3.0, 8.0);
    cr.line_to(7.5, 12.5);
}

pub fn draw_forward(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    cr.move_to(3.0, 8.0);
    cr.line_to(13.0, 8.0);
    cr.move_to(8.5, 3.5);
    cr.line_to(13.0, 8.0);
    cr.line_to(8.5, 12.5);
}

/// A 300° arc open at the upper right, with the arrowhead at its end.
pub fn draw_reload(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    let (cx, cy, r) = (8.0, 8.5, 5.0);
    let end = 3.0 * PI / 2.0;
    cr.arc(cx, cy, r, -PI / 6.0, end);
    let (tx, ty) = (cx, cy - r);
    cr.move_to(tx - 3.0, ty - 3.0);
    cr.line_to(tx, ty);
    cr.line_to(tx - 3.0, ty + 3.0);
}

/// A crosshair: a ring with four ticks. The centre dot is a small stroked
/// circle, because the icon is stroked once and never filled.
pub fn draw_target(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    cr.arc(8.0, 8.0, 4.5, 0.0, 2.0 * PI);
    cr.new_sub_path();
    cr.move_to(8.0, 1.25);
    cr.line_to(8.0, 3.0);
    cr.move_to(8.0, 13.0);
    cr.line_to(8.0, 14.75);
    cr.move_to(1.25, 8.0);
    cr.line_to(3.0, 8.0);
    cr.move_to(13.0, 8.0);
    cr.line_to(14.75, 8.0);
    cr.new_sub_path();
    cr.arc(8.0, 8.0, 0.6, 0.0, 2.0 * PI);
}

/// Three lines with a dot before each one.
pub fn draw_list(cr: &gtk::cairo::Context) {
    cr.set_line_width(1.5);
    for y in [4.0, 8.0, 12.0] {
        cr.move_to(6.5, y);
        cr.line_to(13.5, y);
        cr.new_sub_path();
        cr.arc(3.5, y, 0.6, 0.0, 2.0 * PI);
        cr.new_sub_path();
    }
}

pub fn draw_close(cr: &gtk::cairo::Context) {
    // Drawn on a 16-unit grid and scaled to the 14 px tab icon; the line
    // width is in grid units, so it comes out at ~1.4 px.
    cr.scale(14.0 / 16.0, 14.0 / 16.0);
    cr.set_line_width(1.6);
    cr.move_to(3.5, 3.5);
    cr.line_to(12.5, 12.5);
    cr.move_to(12.5, 3.5);
    cr.line_to(3.5, 12.5);
}
