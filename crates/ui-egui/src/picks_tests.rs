//! Pickers that answer on a later frame ([`Services::begin_pick`]), and the blocking fallback.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use figcraft_engine::Session;

use crate::{FigApp, PickKind, PickRequest, Services};

/// A platform whose picker answers later: records what was asked and answers when told to.
#[derive(Clone, Default)]
struct Deferred {
    asked: Rc<RefCell<Vec<PickRequest>>>,
    answer: Rc<RefCell<Option<Option<PathBuf>>>>,
    polls: Rc<Cell<usize>>,
}

impl Services for Deferred {
    fn begin_pick(&mut self, request: &PickRequest) -> bool {
        self.asked.borrow_mut().push(request.clone());
        true
    }
    fn poll_pick(&mut self) -> Option<Option<PathBuf>> {
        self.polls.set(self.polls.get() + 1);
        self.answer.borrow_mut().take()
    }
}

/// A desktop-style platform: blocks and answers at once through the `pick_*` methods.
struct Blocking(Option<PathBuf>);

impl Services for Blocking {
    fn pick_open(&mut self, _: &str, _: &[&str]) -> Option<PathBuf> {
        self.0.clone()
    }
    fn pick_save(&mut self, _: &str, _: &str, _: &[&str]) -> Option<PathBuf> {
        self.0.clone()
    }
    fn pick_folder(&mut self, _: &str) -> Option<PathBuf> {
        self.0.clone()
    }
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figcraft-picks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn a_deferred_save_waits_for_the_answer_then_writes_the_file() {
    let dir = temp("save");
    let platform = Deferred::default();
    let mut app = FigApp::new(Session::new(), Box::new(platform.clone()));
    app.save_as_dialog();
    assert!(app.picking());
    let asked = platform.asked.borrow().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].kind, PickKind::Save);
    assert_eq!(asked[0].exts, ["figcraft"]);
    assert!(asked[0].name.ends_with(".figcraft"), "{:?}", asked[0].name);
    // Still choosing: nothing is written, and the pick stays open.
    app.poll_pick();
    assert!(app.picking());
    assert!(std::fs::read_dir(&dir).expect("dir").next().is_none());
    let target = dir.join("design.figcraft");
    *platform.answer.borrow_mut() = Some(Some(target.clone()));
    app.poll_pick();
    assert!(!app.picking());
    assert!(target.is_file(), "the saved file exists");
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn a_cancelled_deferred_pick_does_nothing_and_frees_the_slot() {
    let platform = Deferred::default();
    let mut app = FigApp::new(Session::new(), Box::new(platform.clone()));
    app.open_dialog();
    assert!(app.picking());
    *platform.answer.borrow_mut() = Some(None);
    app.poll_pick();
    assert!(!app.picking());
    // A new pick can start again.
    app.open_dialog();
    assert!(app.picking());
    assert_eq!(platform.asked.borrow().len(), 2);
}

#[test]
fn a_second_request_while_one_is_open_is_dropped() {
    let platform = Deferred::default();
    let mut app = FigApp::new(Session::new(), Box::new(platform.clone()));
    app.open_dialog();
    app.import_dialog(true);
    app.save_as_dialog();
    assert_eq!(platform.asked.borrow().len(), 1, "only the first reached the platform");
}

#[test]
fn polling_without_a_pick_never_asks_the_platform() {
    let platform = Deferred::default();
    let mut app = FigApp::new(Session::new(), Box::new(platform.clone()));
    app.poll_pick();
    assert_eq!(platform.polls.get(), 0);
    assert!(!app.picking());
}

#[test]
fn a_platform_that_declines_begin_pick_uses_the_blocking_picker() {
    let dir = temp("blocking");
    let target = dir.join("blocking.figcraft");
    let mut app = FigApp::new(Session::new(), Box::new(Blocking(Some(target.clone()))));
    app.save_as_dialog();
    assert!(!app.picking(), "answered at once");
    assert!(target.is_file());
    // Cancelled: no file, nothing pending.
    let mut app = FigApp::new(Session::new(), Box::new(Blocking(None)));
    app.save_as_dialog();
    assert!(!app.picking());
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn folder_and_export_requests_carry_their_kind_and_suggested_name() {
    let platform = Deferred::default();
    let mut app = FigApp::new(Session::new(), Box::new(platform.clone()));
    app.pick(PickRequest::folder("Export"), |_, _| {});
    assert_eq!(platform.asked.borrow()[0].kind, PickKind::Folder);
    assert_eq!(PickRequest::open("Open", &["svg", "png"]).exts, ["svg", "png"]);
    assert_eq!(PickRequest::save("Export", "a.png", &["png"]).name, "a.png");
}
