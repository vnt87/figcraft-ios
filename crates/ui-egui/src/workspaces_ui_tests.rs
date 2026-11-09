//! Home ▸ Workspaces: the sidebar lists the servers you joined, and the section shows the open
//! one or the form that joins another ([`crate::home::workspaces`]).

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::home::Section;
use crate::{FigApp, NoServices};

struct Home {
    ctx: egui::Context,
    app: FigApp,
    time: f64,
}

impl Home {
    fn new() -> Self {
        let mut h = Self { ctx: egui::Context::default(), app: FigApp::new(Session::new(), Box::new(NoServices)), time: 0.0 };
        h.app.session.prefs.publish_server = String::new();
        h.app.ui.home = true;
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))), time: Some(self.time), events, ..Default::default() };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        out.textures_delta.clear();
    }

    fn state(&self) -> Value {
        crate::home::workspaces::inspect(&self.app)
    }

    fn click(&mut self, at: Pos2) {
        let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![press(true)]);
        self.frame(vec![press(false)]);
        self.frame(vec![]);
    }
}

#[test]
fn the_section_is_asked_for_by_name() {
    assert_eq!(Section::parse("workspaces"), Some(Section::Workspaces));
    assert_eq!(Section::Workspaces.id(), "workspaces");
    assert_eq!(Section::parse("templates"), Some(Section::Templates));
}

#[test]
fn a_joined_workspace_is_listed_and_its_page_shows() {
    let mut h = Home::new();
    h.app.run("workspace.join", json!({"url": "wss://team.example/", "name": "Design"}));
    h.app.ui.home_view.section = Section::Workspaces;
    h.app.ui.home_view.workspaces.open = Some("wss://team.example/".into());
    h.frame(vec![]);
    let s = h.state();
    assert_eq!(s["open"], true);
    assert_eq!(s["workspace"], "wss://team.example/");
    assert_eq!(s["workspaces"][0]["label"], "Design");
    assert_eq!(s["joining"], false);
}

#[test]
fn a_workspace_that_was_left_elsewhere_closes_its_page() {
    let mut h = Home::new();
    h.app.run("workspace.join", json!({"url": "wss://team.example/"}));
    h.app.ui.home_view.section = Section::Workspaces;
    h.app.ui.home_view.workspaces.open = Some("wss://team.example/".into());
    h.frame(vec![]);
    h.app.run("workspace.leave", json!({"url": "wss://team.example/"}));
    h.frame(vec![]);
    let s = h.state();
    assert_eq!(s["workspace"], Value::Null);
    assert_eq!(s["joining"], true, "the form shows");
}

#[test]
fn the_plus_beside_the_heading_opens_the_join_form_and_a_row_opens_its_workspace() {
    let mut h = Home::new();
    h.app.run("workspace.join", json!({"url": "wss://one.example/"}));
    h.app.run("workspace.join", json!({"url": "wss://two.example/"}));
    h.frame(vec![]);
    let row = |h: &Home, i: usize| h.app.ui.home_view.workspaces.rows[i].1.center();
    let at = row(&h, 0);
    h.click(at);
    let s = h.state();
    assert_eq!(s["open"], true);
    assert_eq!(s["workspace"], "wss://one.example/", "the first row");
    let at = row(&h, 1);
    h.click(at);
    assert_eq!(h.state()["workspace"], "wss://two.example/", "the second row");
    // The "+" is at the right end of the heading.
    let at = h.app.ui.home_view.workspaces.plus.unwrap().center();
    h.click(at);
    let s = h.state();
    assert_eq!(s["joining"], true, "the join form: {s}");
    assert_eq!(s["workspace"], Value::Null);
}

#[test]
fn right_click_on_a_row_asks_before_leaving() {
    let mut h = Home::new();
    h.app.run("workspace.join", json!({"url": "wss://one.example/"}));
    h.frame(vec![]);
    let at = h.app.ui.home_view.workspaces.rows[0].1.center();
    let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE };
    h.frame(vec![Event::PointerMoved(at)]);
    h.frame(vec![press(true)]);
    h.frame(vec![press(false)]);
    h.frame(vec![]);
    let s = h.state();
    assert_eq!(s["confirmLeave"], true);
    assert_eq!(h.app.session.prefs.workspaces.len(), 1, "nothing is left until it is confirmed");
}
