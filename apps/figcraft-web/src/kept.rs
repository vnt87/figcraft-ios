//! What the page keeps across reloads, in the browser's local storage: your name (Preferences ▸
//! Your name), so a reload or a later visit joins live sessions and comments as you, and the
//! workspaces you joined (their addresses and names; no key is in either).

use figcraft_engine::{Prefs, Session};
use serde_json::{Value, json};

/// Put back the name the page kept, checked as one typed in Preferences: the page's
/// storage is anyone's to change.
pub fn restore_name(session: &mut Session, kept: Option<String>) {
    if let Some(name) = kept
        && let Err(e) = session.execute("pref.userName", &json!({"name": name}))
    {
        log::warn!("the kept name wasn't used: {e}");
    }
}

/// The workspaces as the page keeps them: a JSON list of `{url, name}`, or none when there are none.
pub fn workspaces_to_keep(prefs: &Prefs) -> Option<String> {
    if prefs.workspaces.is_empty() {
        return None;
    }
    let list: Vec<Value> = prefs.workspaces.iter().map(|w| json!({"url": w.url, "name": w.name})).collect();
    serde_json::to_string(&list).ok()
}

/// Put back the workspaces the page kept, joined one by one as if typed: the page's storage is
/// anyone's to change, so whatever isn't a workspace address is skipped.
pub fn restore_workspaces(session: &mut Session, kept: Option<String>) {
    let Some(Ok(Value::Array(list))) = kept.map(|k| serde_json::from_str::<Value>(&k)) else { return };
    for w in list.iter().take(figcraft_engine::workspace::MAX_WORKSPACES) {
        if let Err(e) = session.execute("workspace.join", w) {
            log::warn!("a kept workspace wasn't used: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use figcraft_engine::Session;
    use serde_json::json;

    use super::{restore_name, restore_workspaces, workspaces_to_keep};

    /// What one page kept, the next one restores; junk in the storage is cleaned like typing.
    #[test]
    fn the_name_survives_a_reload() {
        let mut before = Session::new();
        before.execute("pref.userName", &json!({"name": "  Alice "})).unwrap();
        let kept = before.prefs.user_name.clone();
        let mut after = Session::new();
        restore_name(&mut after, Some(kept));
        assert_eq!(after.prefs.user_name, "Alice");
        let mut fresh = Session::new();
        restore_name(&mut fresh, None);
        assert_eq!(fresh.prefs.user_name, Session::new().prefs.user_name, "nothing kept: the default");
        restore_name(&mut fresh, Some(format!("Bob\u{7}{}", "x".repeat(500))));
        assert!(fresh.prefs.user_name.starts_with("Bobx") && fresh.prefs.user_name.chars().count() == 64);
    }

    /// The workspaces a page kept come back in the next one, in order; junk is skipped.
    #[test]
    fn the_workspaces_survive_a_reload() {
        let mut before = Session::new();
        before.prefs.publish_server = String::new();
        assert_eq!(workspaces_to_keep(&before.prefs), None, "none joined: nothing kept");
        before.execute("workspace.join", &json!({"url": "wss://a.example/", "name": "A"})).unwrap();
        before.execute("workspace.join", &json!({"url": "ws://b.example:81/"})).unwrap();
        let kept = workspaces_to_keep(&before.prefs);
        let mut after = Session::new();
        after.prefs.publish_server = String::new();
        restore_workspaces(&mut after, kept);
        assert_eq!(after.prefs.workspaces, before.prefs.workspaces);
        let mut junk = Session::new();
        junk.prefs.publish_server = String::new();
        restore_workspaces(&mut junk, Some(r#"[{"url":"javascript:1"},7,{"url":"wss://k.example/?key=x"},{"url":"wss://ok.example/"}]"#.into()));
        restore_workspaces(&mut junk, Some("not json".into()));
        restore_workspaces(&mut junk, None);
        assert_eq!(junk.prefs.workspaces.len(), 1);
        assert_eq!(junk.prefs.workspaces[0].url, "wss://ok.example/");
    }
}
