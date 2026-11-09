//! Home ▸ Workspaces: the servers you joined (`workspace.*`), listed in the sidebar under
//! Templates. A workspace is a server's address; opening one shows the files its operator shares
//! with everyone who reaches it (its team list), and a click joins one, as on Home ▸ Online. The
//! "+" beside the heading opens the form that joins another. Nothing is asked of a server until its
//! workspace is shown, and then once, in the background.
//!
//! Leaving a workspace (its row's right-click or long-press menu, or the button beside its name)
//! asks first and only forgets it here: nothing on the server changes.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Ui, pos2, vec2};
use figcraft_engine::online::server_host;
use figcraft_engine::team::{Team, TeamList};
use serde_json::{Value, json};

use crate::FigApp;
use crate::a11y;
use crate::home::Section;
use crate::home::online::{team_detail, team_row};
use crate::i18n::{tr, trf};
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets;

const FILES: &str = "workspace.files";
/// Longest address or name typed into the form (the engine keeps its own limits).
const TYPED_MAX: usize = 500;

/// The state of the Workspaces section.
#[derive(Default)]
pub struct WorkspacesUi {
    /// The workspace shown (its address); none: the form that joins one.
    pub open: Option<String>,
    /// The join form's fields.
    join_url: String,
    join_name: String,
    /// The name being typed for the open workspace (Rename was chosen).
    rename: Option<String>,
    /// Leave was chosen for the open workspace: its confirmation shows.
    leave: bool,
    /// The workspace and frame last drawn: a workspace shown again after a gap asks its server.
    drawn: Option<(String, u64)>,
    /// Where the sidebar drew the "+" and each workspace's row last frame (the tests click there).
    pub(crate) plus: Option<Rect>,
    pub(crate) rows: Vec<(String, Rect)>,
}

/// Show workspace `url` (clearing what belonged to the one before).
fn open(app: &mut FigApp, url: Option<String>) {
    let home = &mut app.ui.home_view;
    home.section = Section::Workspaces;
    home.file_query.clear();
    home.workspaces.rename = None;
    home.workspaces.leave = false;
    home.workspaces.open = url;
}

/// The sidebar's Workspaces heading with its "+" and one row per workspace, from `y`. Right-click or
/// long-press on a row asks to leave it.
pub fn sidebar(app: &mut FigApp, ui: &mut Ui, t: &Tokens, left: Rect, mut y: f32) {
    app.session.prefs.migrate_workspaces();
    let x = left.min.x + 8.0;
    let width = crate::home::SIDEBAR - 16.0;
    ui.painter().text(pos2(x + 10.0, y + 12.0), Align2::LEFT_CENTER, tr("Workspaces"), semibold(11.0), t.text_secondary);
    let plus = Rect::from_center_size(pos2(x + width - 16.0, y + 12.0), vec2(24.0, 24.0));
    app.ui.home_view.workspaces.plus = Some(plus);
    app.ui.home_view.workspaces.rows.clear();
    let resp = ui.interact(plus, ui.scope_id().with("workspace_join"), Sense::click());
    a11y::button(&resp, tr("Join workspace"));
    if resp.hovered() {
        ui.painter().rect_filled(plus, CornerRadius::same(5), t.hover);
    }
    icons::paint(ui.painter(), plus.shrink(5.0), Icon::Plus, t.icon);
    if resp.on_hover_text(tr("Join workspace")).clicked() {
        open(app, None);
    }
    y += 30.0;
    let list = app.session.prefs.workspaces.clone();
    if list.is_empty() {
        ui.painter().text(pos2(x + 10.0, y + 12.0), Align2::LEFT_CENTER, tr("No workspaces yet"), regular(11.0), t.text_tertiary);
        return;
    }
    for w in &list {
        let r = Rect::from_min_size(pos2(x, y), vec2(width, 32.0));
        app.ui.home_view.workspaces.rows.push((w.url.clone(), r));
        let resp = ui.interact(r, ui.scope_id().with(("workspace_row", &w.url)), Sense::click());
        let current = app.ui.home_view.file_query.trim().is_empty()
            && app.ui.home_view.section == Section::Workspaces
            && app.ui.home_view.workspaces.open.as_deref() == Some(w.url.as_str());
        a11y::tab(&resp, w.label(), current);
        if current || resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), if current { t.selected_row } else { t.hover });
        }
        icons::paint(ui.painter(), Rect::from_min_size(pos2(r.min.x + 10.0, r.center().y - 8.0), vec2(16.0, 16.0)), Icon::Workspace, t.icon);
        let text = Rect::from_min_max(pos2(r.min.x + 36.0, r.min.y), pos2(r.max.x - 6.0, r.max.y));
        ui.painter().with_clip_rect(text).text(pos2(text.min.x, r.center().y), Align2::LEFT_CENTER, w.label(), regular(12.0), t.text);
        if resp.clicked() {
            open(app, Some(w.url.clone()));
        }
        if resp.secondary_clicked() || resp.long_touched() {
            open(app, Some(w.url.clone()));
            app.ui.home_view.workspaces.leave = true;
        }
        y += 34.0;
    }
}

/// The section in `main`: the open workspace's files, or the form that joins one.
pub fn section(app: &mut FigApp, ui: &mut Ui, main: Rect, t: &Tokens) {
    app.session.prefs.migrate_workspaces();
    // A workspace that is gone (left from another window, or the list was cleaned) closes.
    let shown = app.ui.home_view.workspaces.open.clone().filter(|url| app.session.prefs.workspace_index(url).is_some());
    app.ui.home_view.workspaces.open = shown.clone();
    let body = Rect::from_min_max(main.min, pos2(main.min.x + main.width().min(720.0), main.max.y));
    ui.scope_builder(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.spacing_mut().item_spacing.y = 8.0;
        match &shown {
            Some(url) => workspace(app, ui, t, url),
            None => join_form(app, ui, t),
        }
    });
}

/// The form that joins a workspace.
fn join_form(app: &mut FigApp, ui: &mut Ui, t: &Tokens) {
    ui.label(egui::RichText::new(tr("Join a workspace")).font(semibold(18.0)).color(t.text));
    widgets::label(
        ui,
        tr("A workspace is a sync server. Paste its address to see the files its team shares. You can join as many as you like."),
        t.text_secondary,
    );
    ui.add_space(6.0);
    let ws = &mut app.ui.home_view.workspaces;
    let url = widgets::text_input(ui, Id::unique("workspace_join_url"), &mut ws.join_url, 420.0, "wss://sync.example.com/");
    a11y::text(&url, tr("Server address"), &ws.join_url);
    let name = widgets::text_input(ui, Id::unique("workspace_join_name"), &mut ws.join_name, 420.0, tr("Name (optional)"));
    a11y::text(&name, tr("Name (optional)"), &ws.join_name);
    ws.join_url.truncate(ws.join_url.char_indices().nth(TYPED_MAX).map_or(ws.join_url.len(), |(i, _)| i));
    let typed = !ws.join_url.trim().is_empty();
    let enter = (url.lost_focus() || name.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let has_any = !app.session.prefs.workspaces.is_empty();
    let (mut join, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        join = widgets::button(ui, tr("Join"), true, typed).clicked();
        if has_any {
            cancel = widgets::button(ui, tr("Cancel"), false, true).clicked();
        }
    });
    if (join || (enter && typed))
        && let Some(r) =
            app.run("workspace.join", json!({"url": app.ui.home_view.workspaces.join_url.trim(), "name": app.ui.home_view.workspaces.join_name}))
    {
        let url = r["url"].as_str().map(str::to_string);
        let ws = &mut app.ui.home_view.workspaces;
        ws.join_url.clear();
        ws.join_name.clear();
        open(app, url);
    } else if cancel {
        let first = app.session.prefs.workspaces.first().map(|w| w.url.clone());
        open(app, first);
    }
}

/// One workspace: its name and server, Refresh · Rename · Leave, and its team's files.
fn workspace(app: &mut FigApp, ui: &mut Ui, t: &Tokens, url: &str) {
    let ctx = ui.ctx().clone();
    let Some(w) = app.session.prefs.workspace_index(url).and_then(|i| app.session.prefs.workspaces.get(i).cloned()) else { return };
    // Shown again after something else was: ask the server.
    let frame = ctx.cumulative_frame_nr();
    let again = app.ui.home_view.workspaces.drawn.as_ref().is_none_or(|(u, f)| u != url || f.saturating_add(1) < frame);
    app.ui.home_view.workspaces.drawn = Some((url.to_string(), frame));
    if again {
        ask(app, &ctx, url);
    }
    ui.label(egui::RichText::new(w.label()).font(semibold(18.0)).color(t.text));
    widgets::label(ui, server_host(&w.url), t.text_secondary);
    let (mut refresh, mut rename, mut leave) = (false, false, false);
    ui.horizontal(|ui| {
        refresh = widgets::button(ui, tr("Refresh"), false, !app.session.waiting_for(FILES)).clicked();
        rename = widgets::button(ui, tr("Rename"), false, true).clicked();
        leave = widgets::button(ui, tr("Leave"), false, true).clicked();
    });
    if refresh {
        ask(app, &ctx, url);
    }
    if rename {
        app.ui.home_view.workspaces.rename = Some(w.name.clone());
        app.ui.home_view.workspaces.leave = false;
    }
    if leave {
        app.ui.home_view.workspaces.leave = true;
        app.ui.home_view.workspaces.rename = None;
    }
    rename_bar(app, ui, url);
    leave_bar(app, ui, t, url, w.label());
    if app.ui.home_view.workspaces.open.is_none() {
        return;
    }
    ui.add_space(6.0);
    let team = app.session.workspace_team(url).cloned();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        files(app, ui, t, url, team.as_ref());
    });
}

/// Ask the workspace's server for its team list, in the background.
fn ask(app: &mut FigApp, ctx: &egui::Context, url: &str) {
    if !app.session.waiting_for(FILES) {
        app.run_background(ctx, FILES, json!({"url": url, "refresh": true}), Box::new(|_, _, _| {}));
    }
}

/// Rename: the field with Save and Cancel, while it is open.
fn rename_bar(app: &mut FigApp, ui: &mut Ui, url: &str) {
    let Some(mut name) = app.ui.home_view.workspaces.rename.take() else { return };
    let (mut save, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        let r = widgets::text_input(ui, Id::unique(("workspace_rename", url)), &mut name, 260.0, tr("Name"));
        a11y::text(&r, tr("Name"), &name);
        save = widgets::button(ui, tr("Save"), true, true).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        cancel = widgets::button(ui, tr("Cancel"), false, true).clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)));
    });
    if save {
        app.run("workspace.rename", json!({"url": url, "name": name}));
    } else if !cancel {
        app.ui.home_view.workspaces.rename = Some(name);
    }
}

/// Leave: says what it does, then asks.
fn leave_bar(app: &mut FigApp, ui: &mut Ui, t: &Tokens, url: &str, label: &str) {
    if !app.ui.home_view.workspaces.leave {
        return;
    }
    widgets::label(ui, &trf("Leave {name}? It only leaves your list here: nothing on the server changes.", &[("name", &label)]), t.text_secondary);
    let (mut confirm, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        confirm = widgets::button(ui, tr("Leave workspace"), true, true).clicked();
        cancel = widgets::button(ui, tr("Cancel"), false, true).clicked();
    });
    if confirm {
        if app.run("workspace.leave", json!({"url": url})).is_some() {
            let next = app.session.prefs.workspaces.first().map(|w| w.url.clone());
            open(app, next);
        }
    } else if cancel {
        app.ui.home_view.workspaces.leave = false;
    }
}

/// The workspace's files, or what stands in their place.
fn files(app: &mut FigApp, ui: &mut Ui, t: &Tokens, url: &str, team: Option<&Team>) {
    let host = server_host(url).to_string();
    if app.session.waiting_for(FILES) {
        return widgets::label(ui, tr("Checking the server…"), t.text_secondary);
    }
    let Some(team) = team else {
        return widgets::label(ui, tr("Not loaded yet. Press Refresh to ask the server."), t.text_secondary);
    };
    match &team.list {
        TeamList::Offline(_) => widgets::label(ui, &trf("Couldn't reach {server}. Press Refresh to try again.", &[("server", &host)]), t.danger),
        TeamList::None => widgets::label(ui, &trf("{server} doesn't list shared files.", &[("server", &host)]), t.text_secondary),
        TeamList::Files { files, .. } if files.is_empty() => widgets::label(ui, tr("Nobody shares a file there now."), t.text_secondary),
        TeamList::Files { web, files } => {
            widgets::label(ui, tr("Everyone who reaches this server can open these files while they are shared."), t.text_secondary);
            for f in files {
                team_row(app, ui, t, &team.server, web, f);
            }
        }
    }
}

/// The section's state for `ui.inspect`: no address beyond what the list shows.
pub fn inspect(app: &FigApp) -> Value {
    let w = &app.ui.home_view.workspaces;
    let list: Vec<Value> = app.session.prefs.workspaces.iter().map(|w| json!({"url": w.url, "label": w.label()})).collect();
    let team = w.open.as_deref().and_then(|u| app.session.workspace_team(u)).map(|team| match &team.list {
        TeamList::Files { files, .. } => json!({
            "state": "listed",
            "files": files.iter().map(|f| json!({"file": f.file, "name": f.name, "detail": team_detail(f)})).collect::<Vec<_>>(),
        }),
        TeamList::None => json!({"state": "none", "files": []}),
        TeamList::Offline(_) => json!({"state": "offline", "files": []}),
    });
    json!({
        "open": app.ui.home_view.section == Section::Workspaces,
        "workspace": w.open,
        "workspaces": list,
        "joining": w.open.is_none(),
        "confirmLeave": w.leave,
        "renaming": w.rename.is_some(),
        "checking": app.session.waiting_for(FILES),
        "team": team,
    })
}
