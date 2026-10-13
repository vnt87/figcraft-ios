//! File pickers that may answer on a later frame.
//!
//! `Services::pick_*` block until the user chooses, which is fine where the dialog runs on its own
//! thread and not where the UI thread is the one that must keep running: on iPadOS egui draws on
//! UIKit's main thread, and a document picker needs that thread's run loop. A platform with such a
//! picker implements [`Services::begin_pick`] and [`Services::poll_pick`]; [`FigApp::pick`] starts the
//! pick and runs what should follow it once the answer arrives. Elsewhere `begin_pick` declines and
//! `pick` falls back to the blocking call, so behaviour there is unchanged.

use std::path::PathBuf;

use crate::FigApp;

/// What a picker asks the user for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    /// An existing file to read.
    Open,
    /// Where to write a file.
    Save,
    /// A folder to write several files into.
    Folder,
}

/// One picker, as a platform's [`Services::begin_pick`] sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickRequest {
    pub kind: PickKind,
    pub title: String,
    /// The suggested file name (`Save` only).
    pub name: String,
    /// The file extensions on offer, without dots.
    pub exts: Vec<String>,
}

impl PickRequest {
    pub fn open(title: &str, exts: &[&str]) -> Self {
        Self { kind: PickKind::Open, title: title.into(), name: String::new(), exts: exts.iter().map(|e| (*e).into()).collect() }
    }

    pub fn save(title: &str, name: &str, exts: &[&str]) -> Self {
        Self { kind: PickKind::Save, title: title.into(), name: name.into(), exts: exts.iter().map(|e| (*e).into()).collect() }
    }

    pub fn folder(title: &str) -> Self {
        Self { kind: PickKind::Folder, title: title.into(), name: String::new(), exts: Vec::new() }
    }
}

/// What to do with the chosen path.
pub(crate) type PickDone = Box<dyn FnOnce(&mut FigApp, PathBuf)>;

impl FigApp {
    /// Ask the user for a file or folder and run `then` with the choice. A cancelled picker runs
    /// nothing. Only one pick is under way at a time; a second request while one is open is dropped.
    pub(crate) fn pick(&mut self, request: PickRequest, then: impl FnOnce(&mut FigApp, PathBuf) + 'static) {
        if self.pending_pick.is_some() {
            return;
        }
        if self.services.begin_pick(&request) {
            self.pending_pick = Some(Box::new(then));
            return;
        }
        let exts: Vec<&str> = request.exts.iter().map(String::as_str).collect();
        let chosen = match request.kind {
            PickKind::Open => self.services.pick_open(&request.title, &exts),
            PickKind::Save => self.services.pick_save(&request.title, &request.name, &exts),
            PickKind::Folder => self.services.pick_folder(&request.title),
        };
        if let Some(path) = chosen {
            then(self, path);
        }
    }

    /// Finish the pick under way when the platform has its answer (called every frame).
    pub fn poll_pick(&mut self) {
        if self.pending_pick.is_none() {
            return;
        }
        let Some(answer) = self.services.poll_pick() else { return };
        if let Some(then) = self.pending_pick.take()
            && let Some(path) = answer
        {
            then(self, path);
        }
    }

    /// A pick is waiting for the user's answer.
    pub fn picking(&self) -> bool {
        self.pending_pick.is_some()
    }
}
