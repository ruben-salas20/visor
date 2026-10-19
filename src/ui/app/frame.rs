//! Keeps the frame up to date: after each event, every area's snapshot is
//! built and sent if it changed. Rust decides; the frame only draws and asks.

use super::App;
use crate::ui::protocol::{Colors, FrameMsg, Own, SLOTS, ToFrame};

/// What the frame was last sent, so an unchanged snapshot is not sent again.
#[derive(Default)]
pub(super) struct Sent {
    /// The frame said it is ready; until then nothing is sent.
    ready: bool,
    last: [Option<String>; SLOTS],
    /// The tree must be read and sent again. It is the one snapshot built only
    /// when asked, since building it walks the vault.
    pub(super) tree_stale: bool,
}

impl App {
    pub(super) fn on_frame(&mut self, msg: FrameMsg) {
        match msg {
            FrameMsg::Frame(Own::Ready) => {
                // A frame that starts (or starts again) has nothing.
                self.frame = Sent { ready: true, tree_stale: true, ..Default::default() };
            }
            FrameMsg::Frame(Own::Holes { holes }) => self.host.set_holes(&holes),
            FrameMsg::Frame(Own::Cover { boxes, dragging }) => self.host.set_cover(&boxes, dragging),
            FrameMsg::Files(msg) => self.on_sidebar(msg),
            FrameMsg::Sides(msg) => self.on_sides(msg),
            FrameMsg::Bar(msg) => self.on_bar(msg),
            FrameMsg::Find(msg) => match msg {},
            FrameMsg::Settings(msg) => self.on_settings(msg),
        }
    }

    /// Sends the frame what changed since the last time. Called once per event.
    pub(in crate::ui) fn sync_frame(&mut self) {
        if !self.frame.ready {
            return;
        }
        let mut updates = vec![ToFrame::Files(self.files_state()), ToFrame::Sides(self.sides_state()), ToFrame::Bar(self.bar_state()), ToFrame::Status(self.status())];
        if let Some(colors) = self.colors() {
            updates.push(ToFrame::Theme(colors));
        }
        if std::mem::take(&mut self.frame.tree_stale) {
            updates.push(ToFrame::Tree(self.tree()));
        }
        for update in updates {
            let Ok(json) = serde_json::to_string(&update) else { continue };
            let last = &mut self.frame.last[update.slot()];
            if last.as_deref() != Some(json.as_str()) {
                self.host.send(&json);
                *last = Some(json);
            }
        }
    }

    /// The theme's colors. Without a theme the frame keeps its light defaults.
    fn colors(&self) -> Option<Colors> {
        let theme = self.theme()?;
        Some(Colors {
            dark: theme.dark,
            bg: theme.bg.clone(),
            fg: theme.fg.clone(),
            accent: theme.accent.clone(),
            muted: theme.muted.clone(),
            selection: theme.selection.clone(),
        })
    }
}
