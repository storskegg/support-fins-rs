//! The app's real request text for the tests that are about app behaviour (S-07), from
//! `parity/fixtures/app-settings/default.json` (what `FinSettings().optionsJSON()` / `tunablesJSON()` /
//! `drawSettingsJSON()` produce). A test of empty input, or one that compares against the engine's own defaults,
//! keeps `"{}"` and says why.
#![allow(dead_code)]
use std::sync::Arc;

use sf_ffi::{Part, SuggestContext, load_parts};
use sf_io::json::{Json, parse};
use sf_testkit::models_dir;

fn fixture(k: &str) -> String {
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
    fixture("optionsJSON")
}
/// `FinSettings().tunablesJSON()`
pub fn tunables() -> String {
    fixture("tunablesJSON")
}
/// `FinSettings().drawSettingsJSON()`
pub fn draw_settings() -> String {
    fixture("drawSettingsJSON")
}

/// The JSON helpers the FFI tests read results with: each panics, naming what it found, on the wrong shape.
pub fn arr(v: &Json) -> &[Json] {
    match v {
        Json::Arr(a) => a,
        o => panic!("not an array: {o:?}"),
    }
}

pub fn field<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no {k} in {}", v.stringify()))
}

pub fn num(v: &Json) -> f64 {
    match v {
        Json::Num(n) => *n,
        o => panic!("not a number: {o:?}"),
    }
}

pub fn text(v: &Json) -> &str {
    match v {
        Json::Str(s) => s,
        o => panic!("not a string: {o:?}"),
    }
}

pub fn nums(v: &Json) -> Vec<f64> {
    arr(v).iter().map(num).collect()
}

/// `tests/models/<name>.stl`, loaded as the app loads it (its first part).
pub fn load(name: &str) -> Arc<Part> {
    load_parts(
        models_dir()
            .join(format!("{name}.stl"))
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap()
    .remove(0)
}

/// The Suggest context the app sends for a part with no bore chosen.
pub fn ctx() -> SuggestContext {
    SuggestContext {
        current_bore: 0,
        density: 1.24,
    }
}
