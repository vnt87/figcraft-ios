//! The Apple Pencil hover reticle preference: on by default, a toggle command, read as on from
//! settings files that predate it.

use serde_json::json;

use crate::Session;

#[test]
fn the_pencil_reticle_is_on_by_default_toggles_and_old_settings_read_as_on() {
    let mut s = Session::new();
    assert!(s.prefs.pencil_reticle);
    assert_eq!(s.execute("pref.pencilReticle", &json!({})).unwrap()["on"], false);
    assert!(!s.prefs.pencil_reticle);
    assert_eq!(s.execute("pref.pencilReticle", &json!({"on": true})).unwrap()["on"], true);
    let old: crate::Prefs = serde_json::from_value(json!({"theme": "dark"})).unwrap();
    assert!(old.pencil_reticle);
}
