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
    /// Hardware-keyboard state: the keys winit never delivers (Enter, arrows, ⌘ shortcuts).
    keys: crate::hw_keys::HwKeys,
    /// Events produced outside the input hook (releasing keys on focus loss), sent with the next frame.
    pending: Vec<egui::Event>,
}

impl IosApp {
    pub fn new(root: Option<PathBuf>) -> Self {
        let sync = root.clone().map(crate::documents::DocumentSync::new);
        Self { root, was_focused: true, last_safe_area: None, sync, keys: Default::default(), pending: Vec::new() }
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
        if !focused {
            // Key-ups may never arrive once the app is in the background.
            self.pending.extend(self.keys.release_all());
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

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // The keys winit's UIKit view never forwards: read the HID transitions and inject them.
        let now = raw_input.time.unwrap_or_default();
        for (code, pressed) in crate::gc_keyboard::drain() {
            raw_input.events.extend(self.keys.handle(code, pressed, now));
        }
        raw_input.events.append(&mut self.pending);
        raw_input.events.extend(self.keys.tick(now));
        if let Some(wait) = self.keys.next_wake(now) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.001)));
        }
        // The Pencil: its hover moves the pointer, and a touching Pencil is told from a finger.
        crate::pencil_input::with(|pencil| {
            raw_input.events.extend(pencil.events());
            figcraft_ui_egui::canvas::touch::mark_pencil(ctx, pencil.is_down());
            figcraft_ui_egui::canvas::touch::mark_hover(ctx, pencil.hover_height());
        });
        crate::safe_area::apply(raw_input);
    }

    fn on_exit(&mut self) {
        self.save_prefs();
    }
}
