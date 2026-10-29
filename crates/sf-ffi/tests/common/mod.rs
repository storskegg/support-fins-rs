//! The app's real request text for the tests that are about app behaviour (S-07), from
//! `parity/fixtures/app-settings/default.json` (what `FinSettings().optionsJSON()` / `tunablesJSON()` /
//! `drawSettingsJSON()` produce). A test of empty input, or one that compares against the engine's own defaults,
//! keeps `"{}"` and says why.
#![allow(dead_code)]
use sf_io::json::{Json, parse};

fn field(k: &str) -> String {
    let j = parse(include_str!(
        "../../../../parity/fixtures/app-settings/default.json"
    ))
    .unwrap();
    match j.get(k) {
        Some(Json::Str(s)) => s.clone(),
        o => panic!("{k}: {o:?}"),
    }
}
/// `FinSettings().optionsJSON()`
pub fn options() -> String {
    field("optionsJSON")
}
/// `FinSettings().tunablesJSON()`
pub fn tunables() -> String {
    field("tunablesJSON")
}
/// `FinSettings().drawSettingsJSON()`
pub fn draw_settings() -> String {
    field("drawSettingsJSON")
}
