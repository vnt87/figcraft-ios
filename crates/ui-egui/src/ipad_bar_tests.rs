//! The bottom bar on an iPad: lifted off the home indicator and carrying Undo and Redo. Desktop,
//! web and phone keep the bar as it was ([`crate::toolbar::dock`]).

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use figcraft_engine::Session;
use serde_json::json;

use crate::toolbar::{IPAD_LIFT, bar_width, dock, history_width, pill_rect};
use crate::{FigApp, NoServices};

struct Bar {
    ctx: egui::Context,
    app: FigApp,
    time: f64,
}

impl Bar {
    fn new(ipad: bool) -> Self {
        let mut b = Self { ctx: egui::Context::default(), app: FigApp::new(Session::new(), Box::new(NoServices)), time: 0.0 };
        b.app.ui.ipad = ipad;
        for _ in 0..3 {
            b.frame(vec![]);
        }
        b
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))), time: Some(self.time), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
    }

    fn click(&mut self, at: Pos2) {
        let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![press(true)]);
        self.frame(vec![press(false)]);
        self.frame(vec![]);
    }

    fn pill(&self) -> Rect {
        pill_rect(dock(&self.app), bar_width(&self.app))
    }

    /// The middle of the `i`th cell (0 Undo, 1 Redo) at the left end of the bar.
    fn cell(&self, i: usize) -> Pos2 {
        let p = self.pill();
        pos2(p.min.x + 8.0 + (i as f32 + 0.5) * 32.0, p.center().y)
    }
}

#[test]
fn only_the_ipad_bar_is_lifted_and_wider() {
    let flat = Bar::new(false);
    let ipad = Bar::new(true);
    assert_eq!(flat.app.canvas.rect, ipad.app.canvas.rect);
    let (f, i) = (flat.pill(), ipad.pill());
    assert!((f.max.y - flat.app.canvas.rect.max.y + 12.0).abs() < 0.01, "elsewhere 12 pt from the bottom: {f:?}");
    assert!((i.max.y - ipad.app.canvas.rect.max.y + 12.0 + IPAD_LIFT).abs() < 0.01, "on an iPad 56 pt: {i:?}");
    assert!((i.width() - f.width() - history_width(&ipad.app)).abs() < 0.01, "room for Undo and Redo: {f:?} vs {i:?}");
    assert_eq!(history_width(&flat.app), 0.0);
}

#[test]
fn undo_and_redo_buttons_run_the_commands_when_there_is_something_to_undo() {
    let mut b = Bar::new(true);
    b.app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 40}));
    b.frame(vec![]);
    assert_eq!(b.app.session.undo_depth().0, 1);
    b.click(b.cell(1));
    assert_eq!(b.app.session.undo_depth(), (1, 0), "Redo does nothing with nothing to redo");
    b.click(b.cell(0));
    assert_eq!(b.app.session.undo_depth(), (0, 1), "Undo took the shape back");
    b.click(b.cell(1));
    assert_eq!(b.app.session.undo_depth(), (1, 0), "and Redo brought it again");
}

#[test]
fn off_an_ipad_the_same_spot_is_a_tool_not_undo() {
    let mut b = Bar::new(false);
    b.app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 40}));
    b.frame(vec![]);
    b.click(b.cell(0));
    assert_eq!(b.app.session.undo_depth().0, 1, "the first cell is a tool: nothing was undone");
}
