//! Workspaces: the servers you have joined, as the home screen's Workspaces section lists them.
//!
//! A workspace is a sync server's address with a name you can change on this computer; opening one
//! shows the files its operator shares with everyone who reaches it (the team list, [`crate::team`]).
//! The server has no accounts and the address carries no key: nothing here is a secret, and leaving
//! a workspace only forgets it here (nothing on the server changes).
//!
//! The list is kept with the preferences ([`Prefs::workspaces`](crate::Prefs)). The single server of
//! earlier versions (`pref.publishServer`) becomes the first workspace the first time the list is
//! read, unless it is the server FigCraft ships with, and publishing keeps going to the server in
//! `pref.publishServer`: choosing a workspace as the publish target sets it.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::live::DEFAULT_SERVER;
use crate::online::{same_server, server_host};
use crate::params::{req_string, string};
use crate::{Prefs, Result, Session, bad, spec};

/// Most workspaces kept.
pub const MAX_WORKSPACES: usize = 50;
/// Longest name kept (characters).
const MAX_NAME: usize = 80;

/// A server you joined.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// Its address, `wss://host/path/` or `ws://host:port/` (see [`normalize_url`]).
    pub url: String,
    /// The name you gave it here; empty: the server's host shows.
    pub name: String,
}

impl Workspace {
    /// What the list shows: its name, or the host when it has none.
    pub fn label(&self) -> &str {
        if self.name.is_empty() { server_host(&self.url) } else { &self.name }
    }
}

/// `raw` as a server address to record: `https://` and `http://` mean `wss://` and `ws://` (the
/// address of a server's web page is what people paste), a path keeps its case and ends in `/`.
/// `None` when it isn't a plain address: no key, query or fragment, no spaces, at most 500 bytes.
pub fn normalize_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let (scheme, rest) = raw.split_once("://")?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "wss" | "https" => "wss",
        "ws" | "http" => "ws",
        _ => return None,
    };
    let host_end = rest.find('/').unwrap_or(rest.len());
    let host = rest.get(..host_end)?;
    if host.is_empty() || host.starts_with(['.', ':', '@']) || host.contains('@') {
        return None;
    }
    if raw.len() > 500 || raw.contains(|c: char| c.is_whitespace() || c.is_control() || c == '?' || c == '#' || c == '\\') {
        return None;
    }
    let mut url = format!("{scheme}://{}{}", host.to_ascii_lowercase(), rest.get(host_end..)?);
    if !url.ends_with('/') {
        url.push('/');
    }
    Some(url)
}

fn clean_name(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME).collect()
}

/// The usable workspaces of `list`: one per server, at most [`MAX_WORKSPACES`].
fn clean(list: impl IntoIterator<Item = Workspace>) -> Vec<Workspace> {
    let mut out: Vec<Workspace> = Vec::new();
    for w in list {
        let Some(url) = normalize_url(&w.url) else { continue };
        if out.len() < MAX_WORKSPACES && !out.iter().any(|o| same_server(&o.url, &url)) {
            out.push(Workspace { url, name: clean_name(&w.name) });
        }
    }
    out
}

/// Reads the list from the preferences: whatever isn't a usable record is skipped, so junk there
/// never costs the other preferences.
pub(crate) fn read<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<Workspace>, D::Error> {
    let v = Value::deserialize(d).unwrap_or(Value::Null);
    let entries = v.as_array().map(Vec::as_slice).unwrap_or_default();
    Ok(clean(entries.iter().take(4 * MAX_WORKSPACES).filter_map(|e| serde_json::from_value(e.clone()).ok())))
}

impl Prefs {
    /// The first time: the server in `publish_server` becomes the first workspace (not the built-in
    /// one, which isn't a team's), so nobody loses the server they set up. Later never again, so
    /// leaving every workspace stays left.
    pub fn migrate_workspaces(&mut self) {
        if std::mem::replace(&mut self.workspaces_migrated, true) {
            return;
        }
        let own = !self.publish_server.is_empty() && !same_server(&self.publish_server, DEFAULT_SERVER);
        if let Some(url) = own.then(|| normalize_url(&self.publish_server)).flatten()
            && self.workspaces.is_empty()
        {
            self.workspaces.push(Workspace { url, name: String::new() });
        }
    }

    /// Where `url` is in the list.
    pub fn workspace_index(&self, url: &str) -> Option<usize> {
        self.workspaces.iter().position(|w| same_server(&w.url, url))
    }
}

fn listing(prefs: &Prefs) -> Vec<Value> {
    prefs
        .workspaces
        .iter()
        .map(|w| {
            json!({
                "url": w.url,
                "name": w.name,
                "label": w.label(),
                "host": server_host(&w.url),
                "publishTarget": same_server(&w.url, &prefs.publish_server),
            })
        })
        .collect()
}

/// The address `cmd`'s `url` names, as recorded.
fn url_of(cmd: &str, p: &Value) -> Result<String> {
    let raw = req_string(cmd, p, "url")?;
    normalize_url(&raw).ok_or_else(|| {
        bad(cmd, "a workspace is a server's address: wss://host/ or ws://host:port/ (https:// and http:// work too), without a key, query or spaces")
    })
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    s.prefs.migrate_workspaces();
    Ok(json!({"workspaces": listing(&s.prefs), "max": MAX_WORKSPACES}))
}

fn join(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "workspace.join";
    let url = url_of(CMD, p)?;
    s.prefs.migrate_workspaces();
    let name = string(p, "name").map(|n| clean_name(&n)).unwrap_or_default();
    if let Some(i) = s.prefs.workspace_index(&url) {
        // Joining one you are in changes nothing, except a name you gave now.
        if !name.is_empty()
            && let Some(w) = s.prefs.workspaces.get_mut(i)
            && w.name != name
        {
            w.name = name;
            s.prefs_unsaved = true;
        }
        return Ok(json!({"url": s.prefs.workspaces.get(i).map(|w| w.url.as_str()), "added": false, "workspaces": listing(&s.prefs)}));
    }
    if s.prefs.workspaces.len() >= MAX_WORKSPACES {
        return Err(bad(CMD, format!("at most {MAX_WORKSPACES} workspaces: leave one first")));
    }
    s.prefs.workspaces.push(Workspace { url: url.clone(), name });
    s.prefs_unsaved = true;
    Ok(json!({"url": url, "added": true, "workspaces": listing(&s.prefs)}))
}

fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "workspace.rename";
    let url = url_of(CMD, p)?;
    let name = clean_name(&req_string(CMD, p, "name")?);
    s.prefs.migrate_workspaces();
    let i = s.prefs.workspace_index(&url).ok_or_else(|| bad(CMD, "you have not joined that workspace"))?;
    if let Some(w) = s.prefs.workspaces.get_mut(i) {
        w.name = name;
    }
    s.prefs_unsaved = true;
    Ok(json!({"workspaces": listing(&s.prefs)}))
}

fn leave(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "workspace.leave";
    let url = url_of(CMD, p)?;
    s.prefs.migrate_workspaces();
    let i = s.prefs.workspace_index(&url).ok_or_else(|| bad(CMD, "you have not joined that workspace"))?;
    let gone = s.prefs.workspaces.remove(i);
    s.live.workspace_teams.retain(|t| !same_server(&t.server, &gone.url));
    s.prefs_unsaved = true;
    Ok(json!({"left": gone.url, "workspaces": listing(&s.prefs)}))
}

fn files(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "workspace.files";
    let url = url_of(CMD, p)?;
    s.prefs.migrate_workspaces();
    let i = s.prefs.workspace_index(&url).ok_or_else(|| bad(CMD, "you have not joined that workspace"))?;
    let Some(w) = s.prefs.workspaces.get(i).cloned() else { return Err(bad(CMD, "no such workspace")) };
    match p.get("refresh") {
        None => {}
        Some(v) if v.as_bool() == Some(false) => {}
        Some(v) if v.as_bool() == Some(true) => s.workspace_refresh(&w.url)?,
        Some(_) => return Err(bad(CMD, "`refresh` must be true or false")),
    }
    let team = s.workspace_team(&w.url).map(crate::team::team_value);
    Ok(json!({"url": w.url, "label": w.label(), "host": server_host(&w.url), "team": team}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "workspace.list",
            "Workspaces",
            "{} → {workspaces: [{url, name, label (the name, or the host), host, publishTarget (it is the server in `pref.publishServer`)}], max}: the servers you joined, in the order you joined them. The server in `pref.publishServer` is the first one the first time this is asked, unless it is the built-in server",
            false,
            list
        ),
        spec!(
            "workspace.join",
            "Join workspace",
            "{url: wss://host/ or ws://host:port/ (https:// and http:// too; no key), name?} → {url, added (false: you were in it), workspaces}: adds a server to your workspaces. Nothing is asked of the server (`workspace.files {refresh: true}` does)",
            false,
            join
        ),
        spec!(
            "workspace.rename",
            "Rename workspace",
            "{url, name (blank: the host shows)} → {workspaces}: the name you give a workspace on this computer",
            false,
            rename
        ),
        spec!(
            "workspace.leave",
            "Leave workspace",
            "{url} → {left, workspaces}: forgets the workspace on this computer. Nothing on the server changes",
            false,
            leave
        ),
        spec!(
            "workspace.files",
            "Workspace files",
            "{url, refresh? (true: ask the server for its team list; default false: no network)} → {url, label, host, team: {server, host, state: listed | none (the server keeps no team list) | offline, reason, web, files: [{file, name, updated, people, access, link, webLink}]} or null (not asked yet)}: the files the workspace's server shares with everyone who reaches it (`collab.join {link}` opens one)",
            false,
            files
        ),
    ]
}
