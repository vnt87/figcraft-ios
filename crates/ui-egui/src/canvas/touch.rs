//! Touch on the canvas, as on a drawing app: the Apple Pencil is the precise input and acts with
//! whatever tool is active, while a finger taps and pans.
//!
//! - A **finger** that lands where the Move or Scale tool has something to grab (a layer, a
//!   selection handle, an edit in progress) acts as the pointer does. Anywhere else a short press
//!   is a tap (the tool gets a click) and a drag pans the canvas.
//! - A **second finger** ends whatever the first began (the half-drawn shape or drag is dropped and
//!   the document restored) and the two fingers pan and pinch ([`super::input`]); the finger left
//!   when one lifts does nothing until it lifts too.
//! - The **Pencil** is told apart by the host (the iPad runner, which reads the touch type from
//!   UIKit and calls [`mark_pencil`]); where nothing says so every touch counts as a finger.
use egui::{Context, Id, Pos2, Rect};
use figcraft_engine::tools::{ToolId, selection_box};

use crate::FigApp;

/// How far a finger may move (points) and still be a tap.
pub const TAP_SLOP: f32 = 8.0;
/// How far outside a selection's box its handles are still grabbed by a finger (points).
const HANDLE_REACH: f32 = 24.0;

/// A finger that went down on the canvas and is not the tool's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finger {
    /// Where it landed (screen points).
    pub start: Pos2,
    /// It has moved past [`TAP_SLOP`]: the canvas pans with it.
    pub panning: bool,
}

fn pencil_id() -> Id {
    Id::unique("figcraft.touch.pencil")
}

/// Host call: the touch now down (or just lifted) is, or is not, the Apple Pencil's.
pub fn mark_pencil(ctx: &Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(pencil_id(), on));
}

/// The touch down now is the Pencil's.
pub fn pencil_down(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(pencil_id())).unwrap_or(false)
}

/// Whether a press at `pos` is the tool's to take: the Move or Scale tool over a layer, a handle
/// of the selection, or an edit in progress; the Hand tool, which pans by itself.
pub(super) fn tool_takes(app: &FigApp, pos: Pos2) -> bool {
    let tool = app.session.tool.tool;
    if tool == ToolId::Hand {
        return true;
    }
    let Ok(ds) = app.session.ds() else {
        return false;
    };
    if ds.vector_edit.is_some() || ds.text_edit.is_some() {
        return true;
    }
    if !matches!(tool, ToolId::Move | ToolId::Scale) {
        return false;
    }
    let (doc, page, sel, zoom) = (ds.doc.clone(), ds.page, ds.selection.clone(), ds.view.zoom.max(1e-6));
    let p = app.to_page(pos);
    if app.session.hit(&doc, page, p, 2.0 / zoom).is_some() {
        return true;
    }
    selection_box(&doc, &sel).is_some_and(|corners| {
        let mut r = Rect::NOTHING;
        for c in corners {
            r.extend_with(app.to_screen(c.x, c.y));
        }
        r.expand(HANDLE_REACH).contains(pos)
    })
}

/// A second finger ends what the first began. True while a gesture that began with several fingers
/// is still down: the canvas leaves the pointer alone until the last finger lifts.
pub(super) fn second_finger(app: &mut FigApp, ctx: &Context) -> bool {
    let (multi, down) = ctx.input(|i| (i.multi_touch().is_some(), i.pointer.primary_down()));
    if multi {
        if app.canvas.dragging {
            app.canvas.dragging = false;
            // The document is restored; nothing here may stop the pinch, so an error is dropped.
            if let Err(e) = app.session.drop_gesture() {
                log::warn!("touch: dropping the gesture failed: {e}");
            }
        }
        app.canvas.finger = None;
        app.canvas.middle_pan = None;
        app.canvas.multi_lock = true;
    }
    if app.canvas.multi_lock && !multi && !down {
        app.canvas.multi_lock = false;
    }
    app.canvas.multi_lock
}
