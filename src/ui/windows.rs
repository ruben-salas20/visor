
//! The model of visor's windows: which are open, in which order, and which one
//! has the focus. It knows no GTK and no webview; a window is only its id.
//!
//! Agents act on the focused window unless they ask for another one.

use crate::trf;

pub type WinId = u64;

/// First view id of a window's tabs. Each window owns a 2^40 range, so view
/// ids never collide across windows.
pub fn first_view_id(w: WinId) -> u64 {
    w << 40
}

/// `Windows::target` for any owner of the windows: `open` tells whether a window exists.
pub fn target_in(focused: Option<WinId>, open: impl Fn(WinId) -> bool, asked: Option<u64>) -> Result<WinId, String> {
    let Some(focused) = focused else {
        return Err(crate::i18n::tr("No hay ninguna ventana abierta", "No window is open").into());
    };
    match asked {
        None => Ok(focused),
        Some(n) if open(n) => Ok(n),
        Some(n) => Err(trf!("No hay ninguna ventana con el número {n}", "There is no window number {n}")),
    }
}

#[derive(Default)]
pub struct Windows {
    /// Open windows, in opening order.
    list: Vec<WinId>,
    /// The last window that had focus. `None` only when no window is open.
    focused: Option<WinId>,
    /// The next id to hand out. Ids are never reused, so an agent's `--window`
    /// number never points at a different window later.
    next: WinId,
}

#[derive(Debug, PartialEq)]
pub enum Closed {
    /// It was the only window; none is open now.
    Last,
    /// Other windows remain. `focus` is the window with the focus now.
    Other { focus: Option<WinId> },
}

impl Windows {
    pub fn new() -> Self {
        Windows { next: 1, ..Default::default() }
    }

    /// Opens a window at the end of the list and gives it the focus.
    pub fn open(&mut self) -> WinId {
        // `Default` leaves `next` at 0; ids must start at 1.
        let id = self.next.max(1);
        self.next = id + 1;
        self.list.push(id);
        self.focused = Some(id);
        id
    }

    /// Gives the focus to `w`. Returns false if `w` is unknown or already focused.
    pub fn focus(&mut self, w: WinId) -> bool {
        if !self.contains(w) || self.focused == Some(w) {
            return false;
        }
        self.focused = Some(w);
        true
    }

    pub fn focused(&self) -> Option<WinId> {
        self.focused
    }

    /// Open windows, in opening order.
    pub fn ids(&self) -> &[WinId] {
        &self.list
    }

    pub fn contains(&self, w: WinId) -> bool {
        self.list.contains(&w)
    }

    /// Removes `w`. An unknown id changes nothing.
    pub fn close(&mut self, w: WinId) -> Closed {
        let Some(at) = self.list.iter().position(|&x| x == w) else {
            return Closed::Other { focus: self.focused };
        };
        self.list.remove(at);
        if self.list.is_empty() {
            self.focused = None;
            return Closed::Last;
        }
        if self.focused == Some(w) {
            // The newest window takes over, as the one opened last is the most
            // likely to be where the person was working.
            self.focused = self.list.last().copied();
        }
        Closed::Other { focus: self.focused }
    }

    /// The window an agent's request goes to: the one asked for, or the focused one.
    #[cfg(test)]
    pub fn target(&self, asked: Option<u64>) -> Result<WinId, String> {
        target_in(self.focused, |w| self.contains(w), asked)
    }


    /// The windows in the order they are saved to config, and the index of the
    /// focused one in that list.
    pub fn to_save(&self) -> (Vec<WinId>, usize) {
        let index = self.focused.and_then(|f| self.list.iter().position(|&w| w == f)).unwrap_or(0);
        (self.list.clone(), index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Windows {
        let mut w = Windows::new();
        for _ in 0..3 {
            w.open();
        }
        w
    }

    #[test]
    fn ids_start_at_one_and_are_never_reused() {
        let mut w = Windows::new();
        assert_eq!(w.open(), 1);
        assert_eq!(w.open(), 2);
        assert_eq!(w.close(1), Closed::Other { focus: Some(2) });
        assert_eq!(w.open(), 3);
        assert_eq!(w.ids(), &[2, 3]);
    }

    #[test]
    fn default_also_starts_at_one() {
        let mut w = Windows::default();
        assert_eq!(w.open(), 1);
    }

    #[test]
    fn open_focuses_the_new_window() {
        let w = three();
        assert_eq!(w.focused(), Some(3));
        assert_eq!(w.ids(), &[1, 2, 3]);
    }

    #[test]
    fn focus_reports_unknown_and_already_focused() {
        let mut w = three();
        assert!(w.focus(1));
        assert_eq!(w.focused(), Some(1));
        assert!(!w.focus(1));
        assert!(!w.focus(9));
        assert_eq!(w.focused(), Some(1));
    }

    #[test]
    fn closing_the_last_window_says_last() {
        let mut w = Windows::new();
        w.open();
        assert_eq!(w.close(1), Closed::Last);
        assert!(w.ids().is_empty());
        assert_eq!(w.focused(), None);
    }

    #[test]
    fn closing_the_focused_window_moves_focus_to_the_most_recent_remaining() {
        let mut w = three();
        w.focus(1);
        assert_eq!(w.close(1), Closed::Other { focus: Some(3) });
        assert_eq!(w.focused(), Some(3));
    }

    #[test]
    fn closing_an_unfocused_window_keeps_focus() {
        let mut w = three();
        assert_eq!(w.close(1), Closed::Other { focus: Some(3) });
        assert_eq!(w.focused(), Some(3));
        assert_eq!(w.close(9), Closed::Other { focus: Some(3) });
    }

    #[test]
    fn target_none_is_the_focused_window() {
        let mut w = three();
        assert_eq!(w.target(None), Ok(3));
        w.focus(2);
        assert_eq!(w.target(None), Ok(2));
    }

    #[test]
    fn target_known_id_wins_over_focus() {
        let w = three();
        assert_eq!(w.target(Some(1)), Ok(1));
    }

    #[test]
    fn target_unknown_id_is_an_error() {
        let w = three();
        assert_eq!(w.target(Some(7)).unwrap_err(), "No hay ninguna ventana con el número 7");
    }

    #[test]
    fn target_with_no_window_is_an_error() {
        let w = Windows::new();
        assert_eq!(w.target(None).unwrap_err(), "No hay ninguna ventana abierta");
        assert_eq!(w.target(Some(1)).unwrap_err(), "No hay ninguna ventana abierta");
    }

    #[test]
    fn to_save_gives_the_order_and_the_focused_index() {
        let mut w = three();
        assert_eq!(w.to_save(), (vec![1, 2, 3], 2));
        w.focus(2);
        assert_eq!(w.to_save(), (vec![1, 2, 3], 1));
        assert_eq!(Windows::new().to_save(), (vec![], 0));
    }

    #[test]
    fn first_view_id_gives_each_window_its_own_range() {
        assert_eq!(first_view_id(1), 1 << 40);
        assert_ne!(first_view_id(1), first_view_id(2));
        // A window with a million views still stays below the next window's range.
        assert!(first_view_id(1) + 1_000_000 < first_view_id(2));
    }
}
