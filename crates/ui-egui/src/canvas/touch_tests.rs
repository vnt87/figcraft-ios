//! Touch on the canvas: a finger taps and pans, the Pencil acts with the tool, a second finger
//! drops what the first began ([`super::touch`]).

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, TouchDeviceId, TouchId, TouchPhase, vec2};
use figcraft_engine::Session;
use serde_json::json;

use super::touch::mark_pencil;
use crate::{FigApp, NoServices};

struct Touch {
    ctx: egui::Context,
    app: FigApp,
    time: f64,
}

impl Touch {
    fn new() -> Self {
        let mut t = Self { ctx: egui::Context::default(), app: FigApp::new(Session::new(), Box::new(NoServices)), time: 0.0 };
        for _ in 0..3 {
            t.frame(vec![]);
        }
        t
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), time: Some(self.time), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
    }

    fn centre(&self) -> Pos2 {
        self.app.canvas.rect.center()
    }

    fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos, force: None }
    }

    /// One finger (or the Pencil) goes down at `pos`.
    fn down(&mut self, id: u64, pos: Pos2) {
        self.frame(vec![
            Event::PointerMoved(pos),
            Self::touch(id, TouchPhase::Start, pos),
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ]);
    }

    fn drag(&mut self, id: u64, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos), Self::touch(id, TouchPhase::Move, pos)]);
    }

    fn up(&mut self, id: u64, pos: Pos2) {
        self.frame(vec![
            Self::touch(id, TouchPhase::End, pos),
            Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        ]);
        self.frame(vec![Event::PointerGone]);
    }

    fn view(&self) -> (f64, f64) {
        self.app.session.ds().map(|d| (d.view.cx, d.view.cy)).unwrap()
    }

    fn nodes(&self) -> usize {
        self.app.session.ds().map(|d| d.doc.node_count()).unwrap()
    }

    fn busy(&self) -> bool {
        self.app.session.tool.busy()
    }
}

#[test]
fn a_finger_dragging_empty_canvas_pans_and_selects_nothing() {
    let mut t = Touch::new();
    let c = t.centre();
    let before = t.view();
    t.down(1, c);
    t.drag(1, c + vec2(30.0, 0.0));
    t.drag(1, c + vec2(120.0, 40.0));
    assert!(!t.busy(), "no marquee under the finger");
    t.up(1, c + vec2(120.0, 40.0));
    let after = t.view();
    assert!(after.0 < before.0 - 1.0 && after.1 < before.1 - 1.0, "the view followed the finger: {before:?} → {after:?}");
}

#[test]
fn a_finger_drawing_tool_pans_instead_of_drawing() {
    let mut t = Touch::new();
    t.app.run("tool.set", json!({"tool": "rectangle"}));
    let (c, nodes, before) = (t.centre(), t.nodes(), t.view());
    t.down(1, c);
    t.drag(1, c + vec2(150.0, 90.0));
    t.drag(1, c + vec2(170.0, 110.0));
    t.up(1, c + vec2(150.0, 90.0));
    assert_eq!(t.nodes(), nodes, "no shape was drawn");
    assert_ne!(t.view(), before, "the canvas panned");
}

#[test]
fn the_pencil_draws_with_the_tool() {
    let mut t = Touch::new();
    t.app.run("tool.set", json!({"tool": "rectangle"}));
    mark_pencil(&t.ctx, true);
    let (c, nodes, before) = (t.centre(), t.nodes(), t.view());
    t.down(1, c);
    t.drag(1, c + vec2(150.0, 90.0));
    assert!(t.busy(), "the shape is being drawn");
    t.up(1, c + vec2(150.0, 90.0));
    assert!(t.nodes() > nodes, "the shape was drawn");
    assert_eq!(t.view(), before, "the canvas stayed put");
}

#[test]
fn a_second_finger_drops_the_gesture_the_first_began() {
    let mut t = Touch::new();
    t.app.run("tool.set", json!({"tool": "rectangle"}));
    mark_pencil(&t.ctx, true);
    let (c, nodes) = (t.centre(), t.nodes());
    t.down(1, c);
    t.drag(1, c + vec2(150.0, 90.0));
    assert!(t.busy());
    let second = c + vec2(300.0, 0.0);
    t.frame(vec![Touch::touch(2, TouchPhase::Start, second), Touch::touch(1, TouchPhase::Move, c + vec2(150.0, 90.0))]);
    t.frame(vec![Touch::touch(2, TouchPhase::Move, second + vec2(5.0, 0.0)), Touch::touch(1, TouchPhase::Move, c + vec2(155.0, 90.0))]);
    assert!(!t.busy(), "the half-drawn shape was dropped");
    assert_eq!(t.nodes(), nodes, "and the document is as it was");
    // Both lift: the finger left over never draws.
    t.frame(vec![Touch::touch(2, TouchPhase::End, second)]);
    t.frame(vec![Touch::touch(1, TouchPhase::Move, c + vec2(200.0, 90.0)), Event::PointerMoved(c + vec2(200.0, 90.0))]);
    assert!(!t.busy(), "the remaining finger does nothing");
    t.up(1, c + vec2(200.0, 90.0));
    assert_eq!(t.nodes(), nodes);
}

/// Two fingers a little apart, at `at` and `at + (gap, 0)`.
fn two(t: &mut Touch, a: Pos2, b: Pos2, phase_a: TouchPhase, phase_b: TouchPhase) {
    t.frame(vec![Event::PointerMoved(a), Touch::touch(1, phase_a, a), Touch::touch(2, phase_b, b)]);
}

#[test]
fn two_fingers_pan_even_while_their_spacing_wobbles() {
    let mut t = Touch::new();
    let c = t.centre();
    let before = t.view();
    let (mut a, mut b) = (c - vec2(60.0, 0.0), c + vec2(60.0, 0.0));
    two(&mut t, a, b, TouchPhase::Start, TouchPhase::Start);
    // Both fingers move 12 pt right and down each frame, the second a hair further: a pinch of a
    // fraction of a percent every frame, as real fingers make.
    for i in 0..8 {
        a += vec2(12.0, 8.0);
        b += vec2(12.2 + i as f32 * 0.01, 8.0);
        two(&mut t, a, b, TouchPhase::Move, TouchPhase::Move);
    }
    let after = t.view();
    assert!(after.0 < before.0 - 5.0 && after.1 < before.1 - 3.0, "the view followed the fingers: {before:?} → {after:?}");
}

#[test]
fn two_fingers_pan_with_the_pointer_nowhere_over_the_canvas() {
    let mut t = Touch::new();
    let c = t.centre();
    let before = t.view();
    let (mut a, mut b) = (c - vec2(60.0, 0.0), c + vec2(60.0, 0.0));
    two(&mut t, a, b, TouchPhase::Start, TouchPhase::Start);
    for _ in 0..6 {
        a += vec2(15.0, 0.0);
        b += vec2(15.0, 0.0);
        // No pointer position in these frames: only the touches say where the fingers are.
        t.frame(vec![Touch::touch(1, TouchPhase::Move, a), Touch::touch(2, TouchPhase::Move, b), Event::PointerGone]);
    }
    assert!(t.view().0 < before.0 - 5.0, "panned: {before:?} → {:?}", t.view());
}
