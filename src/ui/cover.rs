//! Where the frame takes the pointer. The frame lies over the pages, transparent
//! in their holes; its input zone is the window minus the holes, plus what the
//! frame draws over them (menus, dialogs, floating bars). While the frame drags
//! something, the zone is the whole window, so the drop never reaches a page.
//!
//! It knows no GTK: `ui/host.rs` turns a [`Zone`] into an input shape.

/// A rectangle in whole pixels of the window: x, y, width, height.
pub type Px = (i32, i32, i32, i32);

/// How far around a floating box the frame keeps the pointer and, where the
/// zone also clips drawing (Windows), the pixels: room for its shadow.
pub const SHADOW: i32 = 24;

/// Larger than any window, so the zone needs no window size.
const ALL: Px = (0, 0, 1 << 20, 1 << 20);

/// The frame's input: [`Zone::base`] minus `holes`, plus `boxes`.
#[derive(Debug, PartialEq, Clone)]
pub struct Zone {
    pub base: Px,
    pub holes: Vec<Px>,
    pub boxes: Vec<Px>,
}

impl Zone {
    /// `boxes` are the frame's floating boxes; each takes [`SHADOW`] more around it.
    pub fn new(holes: &[Px], boxes: &[Px], dragging: bool) -> Zone {
        let grow = |&(x, y, w, h): &Px| (x - SHADOW, y - SHADOW, w + 2 * SHADOW, h + 2 * SHADOW);
        Zone {
            base: ALL,
            holes: if dragging { Vec::new() } else { holes.to_vec() },
            boxes: boxes.iter().filter(|b| b.2 > 0 && b.3 > 0).map(grow).collect(),
        }
    }

    /// Whether a point of the window goes to the frame.
    #[cfg(test)]
    pub fn takes(&self, x: i32, y: i32) -> bool {
        let inside = |&(rx, ry, w, h): &Px| x >= rx && x < rx + w && y >= ry && y < ry + h;
        inside(&self.base) && !self.holes.iter().any(inside) || self.boxes.iter().any(inside)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOLE: Px = (200, 40, 800, 660);

    #[test]
    fn pages_take_their_holes_and_the_frame_the_rest() {
        let zone = Zone::new(&[HOLE], &[], false);
        assert!(zone.takes(10, 10), "panel");
        assert!(zone.takes(500, 20), "tab row");
        assert!(!zone.takes(500, 300), "page");
        assert!(!zone.takes(200, 40), "the hole's corner is the page's");
        assert!(zone.takes(199, 40));
        assert!(zone.takes(5000, 5000), "beyond any window size");
    }

    #[test]
    fn a_menu_over_a_page_takes_the_pointer_and_its_shadow() {
        let zone = Zone::new(&[HOLE], &[(220, 44, 240, 200)], false);
        assert!(zone.takes(300, 100), "on the menu");
        assert!(zone.takes(220 + 240 + SHADOW - 1, 100), "on its shadow");
        assert!(!zone.takes(220 + 240 + SHADOW, 100), "past the shadow");
        assert!(!zone.takes(800, 600), "elsewhere on the page");
    }

    #[test]
    fn two_sides_two_holes() {
        let zone = Zone::new(&[(200, 40, 400, 660), (605, 40, 395, 660)], &[], false);
        assert!(!zone.takes(300, 300));
        assert!(zone.takes(602, 300), "the divider");
        assert!(!zone.takes(700, 300));
    }

    #[test]
    fn while_dragging_the_frame_takes_the_whole_window() {
        let zone = Zone::new(&[HOLE], &[], true);
        assert!(zone.takes(500, 300));
        assert_eq!(Zone::new(&[HOLE], &[], false), Zone::new(&[HOLE], &[], false));
        assert_ne!(zone, Zone::new(&[HOLE], &[], false), "a change to apply");
    }

    #[test]
    fn empty_boxes_take_nothing() {
        let zone = Zone::new(&[HOLE], &[(300, 300, 0, 10)], false);
        assert!(!zone.takes(300, 300));
        assert!(zone.boxes.is_empty());
    }
}
