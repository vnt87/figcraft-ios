//! What the Apple Pencil is doing, as the UIKit bridge ([`crate::pencil`]) reports it: whether it
//! touches the screen, and where it hovers above it. [`crate::app::IosApp`] turns that into egui
//! events each frame: a hover moves the pointer (so the canvas answers it as it answers a mouse:
//! the layer under the tip lights up, the tool shows its preview), and a touching Pencil is marked
//! so the canvas draws with it while a finger pans ([`figcraft_ui_egui::canvas::touch`]).
//!
//! This is free of UIKit types so it is tested on the host.
#[cfg(target_os = "ios")]
use std::sync::Mutex;

use eframe::egui::{Event, Pos2};

#[derive(Debug, Default, PartialEq)]
pub struct Pencil {
    down: bool,
    hover: Option<Pos2>,
    /// How high the hovering tip is: 0 at the glass, 1 at the far end of the hardware's range.
    height: f32,
    /// The hover moved since the last frame.
    moved: bool,
    /// The hover ended since the last frame.
    gone: bool,
}

impl Pencil {
    /// The Pencil touched the screen (`true`) or lifted (`false`).
    pub fn touch(&mut self, down: bool) {
        self.down = down;
        if down {
            // A touching Pencil is no longer hovering: its touch moves the pointer from here on.
            self.hover = None;
            self.height = 0.0;
            self.moved = false;
            self.gone = false;
        }
    }

    /// The Pencil hovers at `at` (screen points), or no longer hovers (`None`).
    #[cfg(test)]
    pub fn hover(&mut self, at: Option<Pos2>) {
        self.hover_at(at, 0.0);
    }

    /// As [`Pencil::hover`], with how high the tip is (0 at the glass, 1 at the far end of the range;
    /// out of range is clamped, not a number is 0).
    pub fn hover_at(&mut self, at: Option<Pos2>, height: f32) {
        match at {
            Some(p) if p.is_finite() && !self.down => {
                self.hover = Some(p);
                self.height = if height.is_finite() { height.clamp(0.0, 1.0) } else { 0.0 };
                self.moved = true;
                self.gone = false;
            }
            Some(_) => {}
            None => {
                if self.hover.take().is_some() {
                    self.gone = true;
                }
                self.moved = false;
            }
        }
    }

    /// How high the Pencil hovers (0..=1), if it hovers: the canvas draws its reticle from this.
    pub fn hover_height(&self) -> Option<f32> {
        self.hover.map(|_| self.height)
    }

    pub fn is_down(&self) -> bool {
        self.down
    }

    /// The pointer events this frame needs, each reported once.
    pub fn events(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        if self.down {
            return events;
        }
        if std::mem::take(&mut self.moved)
            && let Some(p) = self.hover
        {
            events.push(Event::PointerMoved(p));
        }
        if std::mem::take(&mut self.gone) {
            events.push(Event::PointerGone);
        }
        events
    }
}

#[cfg(target_os = "ios")]
static SHARED: Mutex<Pencil> = Mutex::new(Pencil { down: false, hover: None, height: 0.0, moved: false, gone: false });

#[cfg(target_os = "ios")]
/// Run `f` on the Pencil state the UIKit callbacks and the frame loop share.
pub fn with<R>(f: impl FnOnce(&mut Pencil) -> R) -> R {
    match SHARED.lock() {
        Ok(mut p) => f(&mut p),
        Err(poisoned) => f(&mut poisoned.into_inner()),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::pos2;

    use super::*;

    #[test]
    fn a_hover_moves_the_pointer_once_and_its_end_takes_the_pointer_away() {
        let mut p = Pencil::default();
        p.hover(Some(pos2(10.0, 20.0)));
        assert_eq!(p.events(), vec![Event::PointerMoved(pos2(10.0, 20.0))]);
        assert!(p.events().is_empty(), "reported once");
        p.hover(Some(pos2(12.0, 20.0)));
        assert_eq!(p.events(), vec![Event::PointerMoved(pos2(12.0, 20.0))]);
        p.hover(None);
        assert_eq!(p.events(), vec![Event::PointerGone]);
        assert!(p.events().is_empty());
    }

    #[test]
    fn a_hover_that_never_began_ends_quietly() {
        let mut p = Pencil::default();
        p.hover(None);
        assert!(p.events().is_empty());
    }

    #[test]
    fn a_touching_pencil_sends_no_hover_and_drops_the_one_under_way() {
        let mut p = Pencil::default();
        p.hover(Some(pos2(1.0, 1.0)));
        p.touch(true);
        assert!(p.is_down());
        assert!(p.events().is_empty(), "the touch moves the pointer now");
        p.hover(Some(pos2(2.0, 2.0)));
        assert!(p.events().is_empty(), "no hover while touching");
        p.touch(false);
        assert!(!p.is_down());
        assert!(p.events().is_empty(), "lifting reports nothing: the touch's own end did");
    }

    #[test]
    fn the_hover_height_is_clamped_and_gone_when_the_hover_ends_or_the_pencil_touches() {
        let mut p = Pencil::default();
        assert_eq!(p.hover_height(), None);
        p.hover_at(Some(pos2(1.0, 1.0)), 0.4);
        assert_eq!(p.hover_height(), Some(0.4));
        p.hover_at(Some(pos2(1.0, 1.0)), 5.0);
        assert_eq!(p.hover_height(), Some(1.0));
        p.hover_at(Some(pos2(1.0, 1.0)), -1.0);
        assert_eq!(p.hover_height(), Some(0.0));
        p.hover_at(Some(pos2(1.0, 1.0)), f32::NAN);
        assert_eq!(p.hover_height(), Some(0.0));
        p.hover_at(Some(pos2(1.0, 1.0)), 0.7);
        p.touch(true);
        assert_eq!(p.hover_height(), None, "touching ends the hover");
        p.touch(false);
        p.hover_at(Some(pos2(2.0, 2.0)), 0.7);
        p.hover(None);
        assert_eq!(p.hover_height(), None, "so does the hover ending");
    }

    #[test]
    fn a_hover_off_the_screen_is_ignored() {
        let mut p = Pencil::default();
        p.hover(Some(pos2(f32::NAN, 0.0)));
        p.hover(Some(pos2(f32::INFINITY, 0.0)));
        assert!(p.events().is_empty());
    }
}
