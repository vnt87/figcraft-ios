//! The app eframe runs on iPadOS: FigCraft's UI (kept in `figcraft_ui_egui::windows`, as on the
//! desktop) plus what the host owns: the safe area, taking saved documents to where the user keeps
//! them, and saving the preferences when the app leaves the foreground.
use std::path::PathBuf;

use eframe::egui;
use figcraft_ui_egui::windows::with_app;

pub struct IosApp {
    /// The app's support folder (none: nowhere to keep documents and settings).
    root: Option<PathBuf>,
    was_focused: bool,
    last_safe_area: Option<String>,
    /// Takes the documents the app saves to where the user keeps them.
    sync: Option<crate::documents::DocumentSync>,
}

impl IosApp {
    pub fn new(root: Option<PathBuf>) -> Self {
        let sync = root.clone().map(crate::documents::DocumentSync::new);
        Self { root, was_focused: true, last_safe_area: None, sync }
    }

    fn save_prefs(&self) {
        if let Some(root) = &self.root {
            with_app(|fig| crate::services::save_state(root, &fig.session.prefs));
        }
    }
}

impl eframe::App for IosApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::safe_area::report(ctx, &mut self.last_safe_area);
        // iPadOS may end a backgrounded app without a word: keep the preferences current.
        let focused = ctx.input(|i| i.focused);
        if self.was_focused && !focused {
            self.save_prefs();
        }
        self.was_focused = focused;
        if let Some(sync) = &mut self.sync {
            sync.poll(|message| {
                with_app(|fig| fig.toast(message));
            });
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        with_app(|fig| figcraft_ui_egui::windows::guard(fig, &ctx, |fig| fig.ui(ui)));
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        crate::safe_area::apply(raw_input);
    }

    fn on_exit(&mut self) {
        self.save_prefs();
    }
}
