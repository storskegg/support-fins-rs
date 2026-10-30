//! Own tests of `sf_cli::{options, entry}` (plugins/shared/engine): the options file is the pinned
//! upstream one, the dialog is validated as `optionsFromDialog` does, `optionVisible` follows
//! `showIf`, and `report_line` words the stats.

use sf_cli::entry::*;
use sf_cli::options::{flag_of, option, options};
use sf_io::json::Json;
use sf_testkit::upstream_root;

#[test]
fn options_json_is_the_pinned_upstream_one() {
    let theirs =
        std::fs::read_to_string(upstream_root().join("plugins/shared/engine/options.json"))
            .unwrap();
    let ours =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/options.json")).unwrap();
    assert_eq!(
        ours, theirs,
        "options.json drifted from the checkout at UPSTREAM"
    );
    assert_eq!(options().len(), 19);
}

#[test]
fn flags_come_from_the_keys() {
    assert_eq!(flag_of("sway.on"), "sway");
    assert_eq!(flag_of("sway.gripFrom"), "sway-grip-from");
    assert_eq!(flag_of("tineDensity"), "tine-density");
    assert_eq!(flag_of("plateOnly"), "plate-only");
    assert!(option("nope").is_err());
}

fn d(kv: &[(&str, Json)]) -> Vec<(String, Json)> {
    kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

#[test]
fn the_dialog_is_validated_strictly() {
    let n = Json::Num;
    let s = |t: &str| Json::Str(t.to_string());
    // left out, null and '' take the default
    let o = options_from_dialog(&d(&[
        ("gap", Json::Null),
        ("mode", s("")),
        ("tines", s("TRUE")),
    ]))
    .unwrap();
    assert_eq!((o.gap, o.mode.clone(), o.tines), (None, None, Some(true)));
    assert_eq!(
        options_from_dialog(&d(&[("coverage", n(0.5))]))
            .unwrap()
            .coverage,
        Some(0.5)
    );
    // a percent control is the entry's 0..1
    let e = options_from_dialog(&d(&[("coverage", n(50.0))])).unwrap_err();
    assert_eq!(
        e,
        "coverage must be 0..1 (a percent control's value / 100), got 50"
    );
    assert_eq!(
        options_from_dialog(&d(&[("gap", n(3.0))])).unwrap_err(),
        "gap must be 0.1..0.4, got 3"
    );
    assert_eq!(
        options_from_dialog(&d(&[("tines", s("maybe"))])).unwrap_err(),
        "tines must be true or false, got \"maybe\""
    );
    assert_eq!(
        options_from_dialog(&d(&[("mode", s("weird"))])).unwrap_err(),
        "mode must be one of auto, full, got \"weird\""
    );
    assert_eq!(
        options_from_dialog(&d(&[("gap", s("x"))])).unwrap_err(),
        "gap must be a number, got \"x\""
    );
    assert_eq!(
        options_from_dialog(&d(&[("layerHeight", n(0.0))])).unwrap_err(),
        "layerHeight must be positive, got 0"
    );
    assert_eq!(
        options_from_dialog(&d(&[("layerHeight", n(0.3))]))
            .unwrap()
            .layer_height,
        Some(0.3)
    );
    assert_eq!(
        options_from_dialog(&d(&[("nozzle", s("0.8"))]))
            .unwrap()
            .nozzle,
        Some(0.8)
    );
    assert_eq!(
        options_from_dialog(&d(&[("nozzle", n(3.0))])).unwrap_err(),
        "nozzle must be 0.1..2 mm, got 3"
    );
    assert!(options_from_dialog(&d(&[("bogus", n(1.0))])).is_err());
    // sway is kept only when it is on
    assert!(
        options_from_dialog(&d(&[("sway.reach", n(0.2))]))
            .unwrap()
            .sway
            .is_none()
    );
    let sw = options_from_dialog(&d(&[("sway.on", Json::Bool(true)), ("sway.reach", n(0.2))]))
        .unwrap()
        .sway
        .unwrap();
    assert_eq!(sw.reach, Some(0.2));
}

#[test]
fn show_if_follows_options_json() {
    let b = Json::Bool;
    let s = |t: &str| Json::Str(t.to_string());
    // tineDensity shows with tines (default on), pinDepth only for vertical tines
    assert!(option_visible("tineDensity", &[]).unwrap());
    assert!(!option_visible("tineDensity", &d(&[("tines", b(false))])).unwrap());
    assert!(!option_visible("pinDepth", &[]).unwrap());
    assert!(option_visible("pinDepth", &d(&[("tineDir", s("vertical"))])).unwrap());
    // coverage is Auto's, separate walls Full's
    assert!(option_visible("coverage", &[]).unwrap());
    assert!(!option_visible("coverage", &d(&[("mode", s("full"))])).unwrap());
    assert!(option_visible("separateWalls", &d(&[("mode", s("full"))])).unwrap());
    // layerHeight: tines, or a pad style light / auto (an `any`)
    assert!(option_visible("layerHeight", &d(&[("tines", b(false))])).unwrap());
    assert!(
        !option_visible(
            "layerHeight",
            &d(&[("tines", b(false)), ("padStyle", s("off"))])
        )
        .unwrap()
    );
    // sway settings need sway.on
    assert!(!option_visible("sway.reach", &[]).unwrap());
    assert!(option_visible("sway.reach", &d(&[("sway.on", b(true))])).unwrap());
}

#[test]
fn seat_soup_centres_on_the_box_and_snaps_to_a_nanometre() {
    let (pos, shift) = seat_soup(&[10.0, 20.0, 30.0, 14.0, 20.0, 30.0, 10.0, 26.0, 33.0]).unwrap();
    assert_eq!((shift.x, shift.y, shift.z), (12.0, 23.0, 30.0));
    assert_eq!(pos, vec![-2.0, -3.0, 0.0, 2.0, -3.0, 0.0, -2.0, 3.0, 3.0]);
    assert_eq!(snap(0.1234564), 0.123456);
    assert!(
        seat_soup(&[])
            .unwrap_err()
            .contains("non-empty triangle soup")
    );
    assert!(seat_soup(&[0.0; 8]).unwrap_err().contains("got 8"));
}

#[test]
fn nozzle_sizes() {
    let s = |t: &str| Json::Str(t.to_string());
    assert_eq!(nozzle_choice(&Json::Num(1.0)).unwrap(), s("1.0"));
    assert_eq!(nozzle_choice(&s(" 0.6 ")).unwrap(), s("0.6"));
    assert!(nozzle_choice(&s("")).is_err());
    assert_eq!(nozzle_size(&s("0.3")).unwrap(), 0.3);
    assert!(nozzle_size(&Json::Num(0.05)).is_err() && nozzle_size(&Json::Num(2.5)).is_err());
}
