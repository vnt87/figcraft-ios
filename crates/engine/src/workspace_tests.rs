//! Workspaces: the servers you joined, kept with the preferences ([`crate::workspace`]).

use serde_json::{Value, json};

use crate::workspace::{MAX_WORKSPACES, Workspace, normalize_url};
use crate::{Prefs, Session};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn urls(v: &Value) -> Vec<String> {
    v["workspaces"].as_array().unwrap().iter().map(|w| w["url"].as_str().unwrap().to_string()).collect()
}

#[test]
fn an_address_is_made_plain_or_refused() {
    assert_eq!(normalize_url("wss://Team.Example/").as_deref(), Some("wss://team.example/"));
    assert_eq!(normalize_url("  https://team.example  ").as_deref(), Some("wss://team.example/"), "a web page's address names its server");
    assert_eq!(normalize_url("http://10.0.0.5:7000").as_deref(), Some("ws://10.0.0.5:7000/"));
    assert_eq!(normalize_url("wss://host/Team/").as_deref(), Some("wss://host/Team/"), "a path keeps its case");
    for bad in [
        "",
        "team.example",
        "ftp://team.example/",
        "wss://",
        "wss:///path",
        "wss://user:pw@host/",
        "wss://host/?key=abc",
        "wss://host/#frag",
        "wss://ho st/",
        "wss://host/\u{7}",
        "wss://host\\evil/",
    ] {
        assert_eq!(normalize_url(bad), None, "{bad:?}");
    }
    assert_eq!(normalize_url(&format!("wss://host/{}", "a".repeat(600))), None, "too long");
}

#[test]
fn joining_renaming_and_leaving_a_workspace() {
    let mut s = Session::new();
    s.prefs.publish_server = String::new();
    assert!(urls(&run(&mut s, "workspace.list", json!({}))).is_empty());
    let r = run(&mut s, "workspace.join", json!({"url": "https://team.example", "name": " Design team "}));
    assert_eq!(r["added"], true);
    assert_eq!(r["url"], "wss://team.example/");
    assert_eq!(s.prefs.workspaces, vec![Workspace { url: "wss://team.example/".into(), name: "Design team".into() }]);
    assert!(std::mem::take(&mut s.prefs_unsaved), "the app is told to keep the list at once");
    // The same server again, written another way, changes nothing.
    let r = run(&mut s, "workspace.join", json!({"url": "WSS://TEAM.example"}));
    assert_eq!(r["added"], false);
    assert_eq!(s.prefs.workspaces.len(), 1);
    assert_eq!(s.prefs.workspaces[0].name, "Design team", "no name given: the name stays");
    run(&mut s, "workspace.join", json!({"url": "ws://localhost:9000/"}));
    assert_eq!(urls(&run(&mut s, "workspace.list", json!({}))), ["wss://team.example/", "ws://localhost:9000/"]);
    run(&mut s, "workspace.rename", json!({"url": "ws://localhost:9000", "name": "Mine"}));
    assert_eq!(s.prefs.workspaces[1].label(), "Mine");
    run(&mut s, "workspace.rename", json!({"url": "ws://localhost:9000", "name": "  "}));
    assert_eq!(s.prefs.workspaces[1].label(), "localhost:9000", "no name: the host shows");
    s.prefs.publish_server = "wss://elsewhere/".into();
    let r = run(&mut s, "workspace.leave", json!({"url": "wss://team.example/"}));
    assert_eq!(urls(&r), ["ws://localhost:9000/"]);
    assert_eq!(s.prefs.publish_server, "wss://elsewhere/", "leaving only forgets: nothing else changes");
    assert!(s.execute("workspace.leave", &json!({"url": "wss://team.example/"})).is_err(), "not in it any more");
    assert!(s.execute("workspace.rename", &json!({"url": "wss://nowhere/", "name": "x"})).is_err());
}

#[test]
fn only_so_many_workspaces() {
    let mut s = Session::new();
    s.prefs.publish_server = String::new();
    for i in 0..MAX_WORKSPACES {
        run(&mut s, "workspace.join", json!({"url": format!("wss://s{i}.example/")}));
    }
    let e = s.execute("workspace.join", &json!({"url": "wss://one-more.example/"})).unwrap_err().to_string();
    assert!(e.contains("at most"), "{e}");
    assert_eq!(s.prefs.workspaces.len(), MAX_WORKSPACES);
}

#[test]
fn the_server_set_up_before_becomes_the_first_workspace_once() {
    let mut s = Session::new();
    s.prefs.publish_server = "wss://ours.example/".into();
    assert_eq!(urls(&run(&mut s, "workspace.list", json!({}))), ["wss://ours.example/"]);
    run(&mut s, "workspace.leave", json!({"url": "wss://ours.example/"}));
    assert!(urls(&run(&mut s, "workspace.list", json!({}))).is_empty(), "left stays left");
    // The built-in server is nobody's team, and an empty one is none.
    for server in [crate::live::DEFAULT_SERVER, ""] {
        let mut s = Session::new();
        s.prefs.publish_server = server.into();
        assert!(urls(&run(&mut s, "workspace.list", json!({}))).is_empty(), "{server:?}");
    }
}

#[test]
fn the_list_survives_the_preferences_file_and_junk_in_it_costs_nothing() {
    let p = Prefs { workspaces: vec![Workspace { url: "wss://a.example/".into(), name: "A".into() }], workspaces_migrated: true, ..Prefs::default() };
    let json = serde_json::to_string(&p).unwrap();
    let back: Prefs = serde_json::from_str(&json).unwrap();
    assert_eq!(back.workspaces, p.workspaces);
    assert!(back.workspaces_migrated);
    // Nothing joined: nothing is written.
    let none = serde_json::to_value(Prefs::default()).unwrap();
    assert!(none.get("workspaces").is_none() && none.get("workspacesMigrated").is_none() && none.get("workspaces_migrated").is_none());
    let junk = json!({
        "nudge_small": 3.0,
        "workspaces": [
            {"url": "wss://ok.example/", "name": "Ok"},
            {"url": "wss://OK.example", "name": "same server"},
            {"url": "javascript:alert(1)"},
            {"url": 7},
            "wss://bare.example/",
            null,
            {"url": "wss://key.example/?key=secret"},
            {"url": "ws://second.example:81"}
        ]
    });
    let p: Prefs = serde_json::from_value(junk).unwrap();
    assert_eq!(p.nudge_small, 3.0, "the other preferences are kept");
    assert_eq!(p.workspaces.iter().map(|w| w.url.as_str()).collect::<Vec<_>>(), ["wss://ok.example/", "ws://second.example:81/"]);
    let p: Prefs = serde_json::from_value(json!({"workspaces": "nonsense", "nudge_small": 2.0})).unwrap();
    assert!(p.workspaces.is_empty() && p.nudge_small == 2.0);
}

#[test]
fn files_of_a_workspace_are_not_asked_for_unless_told_to() {
    let mut s = Session::new();
    s.prefs.publish_server = String::new();
    run(&mut s, "workspace.join", json!({"url": "wss://team.example/"}));
    let r = run(&mut s, "workspace.files", json!({"url": "wss://team.example/"}));
    assert_eq!(r["team"], Value::Null, "no network was used");
    assert!(s.execute("workspace.files", &json!({"url": "wss://team.example/", "refresh": "yes"})).is_err());
    assert!(s.execute("workspace.files", &json!({"url": "wss://other.example/"})).is_err(), "you have not joined it");
}

#[test]
fn the_built_in_server_answers_no_team_list_without_being_asked() {
    if option_env!("FIGCRAFT_DEFAULT_SERVER").is_some() || crate::live::DEFAULT_SERVER.is_empty() {
        return;
    }
    let mut s = Session::new();
    s.prefs.publish_server = String::new();
    run(&mut s, "workspace.join", json!({"url": crate::live::DEFAULT_SERVER}));
    let r = run(&mut s, "workspace.files", json!({"url": crate::live::DEFAULT_SERVER, "refresh": true}));
    assert_eq!(r["team"]["state"], "none");
}
