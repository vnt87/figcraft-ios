//! The reticle that floats over the canvas where the Apple Pencil hovers: a ring and a centre dot at
//! the tip. The ring closes in as the tip nears the glass and fades out towards the far end of the
//! hover range. It is drawn in screen points (zoom does not scale it) above the canvas and is no part
//! of the document or of an export. Drawn from circles: no images.
use egui::{Color32, Context, Id, LayerId, Order, Pos2, Stroke};

use crate::theme::Tokens;

/// The ring's radius (points) with the tip at the glass, and at the far end of the hover range.
pub const RADIUS_NEAR: f32 = 6.0;
pub const RADIUS_FAR: f32 = 22.0;
/// The ring's opacity at the glass and at the far end (it never quite vanishes, so a tip hovering
/// at the edge of the range still shows).
const ALPHA_NEAR: f32 = 1.0;
const ALPHA_FAR: f32 = 0.35;

/// The ring's radius and opacity for a hover `height` (0 at the glass, 1 at the far end). Heights
/// outside 0..=1 are clamped; one that is not a number gives `None` (nothing is drawn).
pub fn ring(height: f32) -> Option<(f32, f32)> {
    if !height.is_finite() {
        return None;
    }
    let h = height.clamp(0.0, 1.0);
    Some((RADIUS_NEAR * (1.0 - h) + RADIUS_FAR * h, ALPHA_NEAR * (1.0 - h) + ALPHA_FAR * h))
}

/// Where and how high the reticle shows, if it does: the preference is on, the Pencil hovers, the
/// pointer is over the canvas (not a panel or the bottom bar) and has a position.
pub fn wanted(on: bool, height: Option<f32>, pointer: Option<Pos2>, over_canvas: bool) -> Option<(Pos2, f32)> {
    match (on && over_canvas, height, pointer) {
        (true, Some(h), Some(at)) if at.is_finite() && h.is_finite() => Some((at, h)),
        _ => None,
    }
}

/// Draw the reticle at `at`, over everything else except the tool's own cursor glyph.
pub fn show(ctx: &Context, t: &Tokens, at: Pos2, height: f32) {
    if !at.is_finite() {
        return;
    }
    let Some((radius, alpha)) = ring(height) else {
        return;
    };
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("figcraft.pencil_reticle")));
    // A light halo under the accent keeps it readable on dark and light artwork alike.
    let halo = Color32::WHITE.gamma_multiply(alpha);
    let ink = t.accent.gamma_multiply(alpha);
    painter.circle_stroke(at, radius, Stroke::new(3.0, halo));
    painter.circle_stroke(at, radius, Stroke::new(1.5, ink));
    painter.circle_filled(at, 2.5, halo);
    painter.circle_filled(at, 1.5, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_closes_in_and_fades_out_with_height() {
        assert_eq!(ring(0.0), Some((RADIUS_NEAR, ALPHA_NEAR)));
        assert_eq!(ring(1.0), Some((RADIUS_FAR, ALPHA_FAR)));
        let (r, a) = ring(0.5).unwrap_or_default();
        assert!(r > RADIUS_NEAR && r < RADIUS_FAR && a < ALPHA_NEAR && a > ALPHA_FAR);
    }

    #[test]
    fn it_shows_only_for_a_hovering_pencil_over_the_canvas_with_the_preference_on() {
        let at = Some(egui::pos2(5.0, 6.0));
        assert_eq!(wanted(true, Some(0.3), at, true), Some((egui::pos2(5.0, 6.0), 0.3)));
        assert_eq!(wanted(false, Some(0.3), at, true), None, "preference off");
        assert_eq!(wanted(true, None, at, true), None, "no Pencil hover");
        assert_eq!(wanted(true, Some(0.3), at, false), None, "over a panel");
        assert_eq!(wanted(true, Some(0.3), None, true), None, "no pointer");
        assert_eq!(wanted(true, Some(0.3), Some(egui::pos2(f32::NAN, 0.0)), true), None);
    }

    #[test]
    fn heights_out_of_range_clamp_and_junk_draws_nothing() {
        assert_eq!(ring(-3.0), ring(0.0));
        assert_eq!(ring(7.0), ring(1.0));
        assert_eq!(ring(f32::NAN), None);
        assert_eq!(ring(f32::INFINITY), None);
        assert_eq!(ring(f32::NEG_INFINITY), None);
    }
}
