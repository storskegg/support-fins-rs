//! Replay of `parity/corpus/report/` (recorded by `parity/oracle/report.js` from web/report.js),
//! all `Tol::Exact`: `report_part`, `report_settings`, `report_coverage` and `report_url`.
//! `replayReport` needs `build_fins` and is replayed with it.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::fins::build::{Bare, FillStats, FinKind, FinRecord, FinsBuilt, PadInfo, SwayInfo};
use sf_engine::fins::pad::PadStyle;
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::overhangs::{Analysis, Region};
use sf_engine::pieces::FloatingPiece;
use sf_engine::types::Xyz;
use sf_io::json::Json;
use sf_io::report::*;

fn check(c: &Case, actual: Value) {
    let expected = c
        .out
        .as_ref()
        .unwrap_or_else(|| panic!("{}: recorded a throw", c.id));
    if let Err(m) = compare(&actual, expected, &Tol::Exact) {
        panic!(
            "{}: out{}: actual {} vs expected {}",
            c.id, m.path, m.actual, m.expected
        );
    }
}

/// A recorded value as `Json`, the way `JSON.stringify` sees it: `undefined` keys are dropped
/// (and `undefined` array items are `null`); non-finite numbers stay (`Json::stringify` writes
/// them as `null`).
fn json_of(v: &Value) -> Json {
    match v {
        Value::Num(n) => Json::Num(*n),
        Value::Str(s) => Json::Str(s.clone()),
        Value::Bool(b) => Json::Bool(*b),
        Value::Null | Value::Undefined => Json::Null,
        Value::Arr(a) => Json::Arr(a.iter().map(json_of).collect()),
        Value::Obj(o) => Json::Obj(
            o.iter()
                .filter(|(_, v)| !matches!(v, Value::Undefined))
                .map(|(k, v)| (k.clone(), json_of(v)))
                .collect(),
        ),
        other => panic!("not JSON: {other:?}"),
    }
}

/// The settings JSON as a recorded value (`Json::Null` is `null`).
fn value_of(j: &Json) -> Value {
    match j {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Num(n) => Value::Num(*n),
        Json::Str(s) => Value::Str(s.clone()),
        Json::Arr(a) => Value::Arr(a.iter().map(value_of).collect()),
        Json::Obj(o) => Value::Obj(o.iter().map(|(k, v)| (k.clone(), value_of(v))).collect()),
    }
}

fn floats(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        Value::F32(a) => a.iter().map(|&x| x as f64).collect(),
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        o => panic!("numbers: {o:?}"),
    }
}

#[test]
fn report_part_replays() {
    let cases = load_cases("report", "reportPart");
    assert!(cases.len() > 20);
    for c in &cases {
        let p = report_part(&floats(&c.args[0]), c.args[1].num());
        check(c, Value::F32(p.iter().map(|&x| x as f32).collect()));
    }
}

#[test]
fn report_settings_replays() {
    let cases = load_cases("report", "reportSettings");
    assert!(cases.len() >= 15);
    for c in &cases {
        let a = &c.args[0];
        let rot: [f64; 9] = floats(a.get("rot")).try_into().expect("9 numbers");
        let size: [f64; 3] = floats(a.get("size")).try_into().expect("3 numbers");
        let j = report_settings(
            a.get("threshold").num(),
            &rot,
            &json_of(a.get("opts")),
            a.get("scale").num(),
            size,
            a.get("faces").num() as usize,
        );
        check(c, value_of(&j));
    }
}

fn opt_num(v: &Value) -> Option<f64> {
    match v {
        Value::Undefined => None,
        o => Some(o.num()),
    }
}

fn truthy_len(v: &Value) -> usize {
    v.arr().len()
}

#[test]
fn report_coverage_replays() {
    let cases = load_cases("report", "reportCoverage");
    for c in &cases {
        let (b, res) = (&c.args[0], &c.args[1]);
        let mut built = FinsBuilt::empty(Seating {
            kind: SeatingKind::Face,
            span: 0.0,
            bed_area: 0.0,
        });
        built.fins = (0..truthy_len(b.get("fins")))
            .map(|i| FinRecord {
                height: 0.0,
                length: 0.0,
                tines: 0,
                rows: 0,
                stilt: 0.0,
                lean: 0.0,
                bearing: 0.0,
                site: None,
                id: i,
                kind: FinKind::Prop,
                tri_ranges: vec![],
                line: vec![],
                span: 0.0,
                short: false,
                fill: false,
                joined: false,
                braces: None,
                ribs: None,
            })
            .collect();
        built.brace_count = opt_num(b.get("braceCount")).map(|n| n as usize);
        built.prop_count = opt_num(b.get("propCount")).map(|n| n as usize);
        built.tines = b.get("tines").num() as usize;
        built.sway = match b.get("sway") {
            Value::Undefined => None,
            s => Some(SwayInfo {
                count: s.get("count").num() as usize,
                tines: 0,
                skipped: 0,
                reason: None,
                cross: 0,
                braces: vec![],
            }),
        };
        built.bare = match b.get("bare") {
            Value::Undefined => None,
            s => Some(Bare {
                area: s.get("area").num(),
                total: s.get("total").num(),
                coverage: 0.5,
            }),
        };
        built.fill = match b.get("fill") {
            Value::Undefined => None,
            f => Some(FillStats {
                walls: f.get("walls").num() as usize,
                rows: 0,
                rows_lost: 0.0,
                row_builds: 0,
                joined: 0,
                ribs: Some(0),
                ties: Some(0),
                tries: 0,
                checks: 0,
                capped: matches!(f.get("capped"), Value::Bool(true)),
                refused: Default::default(),
                bare_before: 0.0,
                unserved_area: f.get("unservedArea").num(),
                unserved_pts: vec![],
                islands: 0,
                islands_bare: f.get("islandsBare").num() as usize,
                island_pts: vec![],
                islands_capped: false,
            }),
        };
        built.unserved = b.get("unserved").num();
        built.small_hole_regions = b.get("smallHoleRegions").num() as usize;
        built.floating = (0..truthy_len(b.get("floating")))
            .map(|_| FloatingPiece {
                faces: 0,
                lowest: [0.0; 3],
                drop: 0.0,
            })
            .collect();
        built.pad = match b.get("pad") {
            Value::Null => None,
            p => Some(PadInfo {
                r1: 0.0,
                r2: 0.0,
                cells: 0,
                height: 0.0,
                points: 0,
                oval: true,
                style: match p.get("style").str() {
                    "auto" => PadStyle::Auto,
                    "light" => PadStyle::Light,
                    "sure" => PadStyle::Sure,
                    _ => PadStyle::Custom,
                },
                auto_sure: None,
                small_foot: None,
                outline: None,
            }),
        };
        let a = Analysis {
            over: vec![],
            kept: vec![],
            on_bed: vec![],
            regions: (0..truthy_len(res.get("regions")))
                .map(|_| Region {
                    faces: vec![],
                    area: 0.0,
                })
                .collect(),
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: 0.0,
            over_face_count: 0,
            offset: Xyz {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            size: Xyz {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        };
        check(c, coverage_value(&report_coverage(&built, &a)));
    }
}

/// The JS object `reportCoverage` returns: every key present, `undefined` where it is `None`.
fn coverage_value(c: &Coverage) -> Value {
    let n = |v: Option<f64>| v.map_or(Value::Undefined, Value::Num);
    let u = |v: Option<usize>| n(v.map(|x| x as f64));
    Value::Obj(vec![
        ("overhangs".into(), Value::Num(c.overhangs as f64)),
        ("supports".into(), Value::Num(c.supports as f64)),
        ("braces".into(), u(c.braces)),
        ("props".into(), u(c.props)),
        ("tines".into(), Value::Num(c.tines as f64)),
        ("swayBraces".into(), u(c.sway_braces)),
        ("bareArea".into(), n(c.bare_area)),
        ("redArea".into(), n(c.red_area)),
        ("barePct".into(), n(c.bare_pct)),
        ("fillWalls".into(), u(c.fill_walls)),
        ("fillBare".into(), n(c.fill_bare)),
        (
            "fillCapped".into(),
            c.fill_capped.map_or(Value::Undefined, Value::Bool),
        ),
        ("islandsBare".into(), u(c.islands_bare)),
        ("unserved".into(), n(c.unserved)),
        ("smallHoles".into(), u(c.small_holes)),
        ("floating".into(), u(c.floating)),
        ("pad".into(), Value::Str(c.pad.clone())),
    ])
}

/// A recorded coverage object back into a `Coverage` (absent keys are `None`).
fn coverage_of(v: &Value) -> Coverage {
    let get = |k: &str| match v {
        Value::Obj(o) => o
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, x)| x)
            .filter(|x| !matches!(x, Value::Undefined)),
        _ => None,
    };
    let n = |k: &str| get(k).map(Value::num);
    let u = |k: &str| n(k).map(|x| x as usize);
    Coverage {
        overhangs: u("overhangs").unwrap(),
        supports: u("supports").unwrap(),
        braces: u("braces"),
        props: u("props"),
        tines: u("tines").unwrap(),
        sway_braces: u("swayBraces"),
        bare_area: n("bareArea"),
        red_area: n("redArea"),
        bare_pct: n("barePct"),
        fill_walls: u("fillWalls"),
        fill_bare: n("fillBare"),
        fill_capped: get("fillCapped").map(|_| true),
        islands_bare: u("islandsBare"),
        unserved: n("unserved"),
        small_holes: u("smallHoles"),
        floating: u("floating"),
        pad: get("pad").unwrap().str().to_string(),
    }
}

#[test]
fn report_url_replays() {
    let cases = load_cases("report", "reportUrl");
    assert!(cases.len() > 50);
    let mut cut = 0;
    for c in &cases {
        let o = &c.args[2];
        let num_or = |k: &str| match o {
            Value::Obj(kv) => kv
                .iter()
                .find(|(n, _)| n == k)
                .map_or(0.0, |(_, v)| v.num()),
            _ => 0.0,
        };
        let url = report_url(
            &json_of(&c.args[0]),
            &coverage_of(&c.args[1]),
            &UrlOpts {
                where_: o.get("where").str().to_string(),
                when: o.get("when").str().to_string(),
                drawn: num_or("drawn"),
                removed: num_or("removed"),
            },
        );
        if url.contains("Copy+settings") {
            cut += 1;
        }
        check(c, Value::Str(url));
    }
    assert!(cut >= 1, "the corpus must reach the too-long link");
}
