//! Replay of `parity/corpus/clearance/` (recorded by `parity/oracle/clearance.js` from
//! web/prop/clearance.js). Exact: the verdicts, masks and runs are discrete, and the one float
//! function (`tallSpan`, `minSpanFor`, `insertFloorStations`) is arithmetic on its inputs.
//! A captured call records only the arguments it passed; the JS defaults fill the rest.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::inside::Nearest;
use sf_engine::prop::clearance::*;
use sf_testkit::parity::{Resolved, resolve_args};

fn bools(v: &Value) -> Vec<bool> {
    v.arr()
        .iter()
        .map(|b| matches!(b, Value::Bool(true)))
        .collect()
}
fn bools_value(b: &[bool]) -> Value {
    Value::Arr(b.iter().map(|&x| Value::Bool(x)).collect())
}
fn range_value(r: Option<[usize; 2]>) -> Value {
    r.map_or(Value::Null, |r| Value::from(vec![r[0] as f64, r[1] as f64]))
}
/// JS arg `i` before the trailing `$config` (if any), `false` when not passed.
fn flag(a: &[Resolved], i: usize) -> bool {
    a.len() - 1 > i && matches!(a[i].plain(), Value::Bool(true))
}

fn station_replay(file: &str, certified: bool) {
    let cases = load_cases("clearance", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let line = pts3(a[0].plain());
        let k = a[1].plain().num() as usize;
        let (topo, rot, off, cfg) = (
            a[2].topo(),
            rot3(a[3].plain()),
            xyz(a[4].plain()),
            cfg_of(&a),
        );
        let r = if certified {
            station_certified(&line, k, topo, &rot, &off, &cfg)
        } else {
            station_is_clear(&line, k, topo, &rot, &off, &cfg)
        };
        check(file, c, &Value::Bool(r), &Tol::Exact);
    }
}

fn path_replay(file: &str) {
    // (no golden scene calls pathToPlateIsClear: its captured file is empty)
    let cases = load_cases("clearance", file);
    for c in &cases {
        let a = resolve_args(&c.args);
        let r = path_to_plate_is_clear(
            &pts3(a[0].plain()),
            a[1].topo(),
            &rot3(a[2].plain()),
            &xyz(a[3].plain()),
            &cfg_of(&a),
        );
        check(file, c, &Value::Bool(r), &Tol::Exact);
    }
}

fn welds_replay(file: &str) {
    let cases = load_cases("clearance", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let hit = Nearest {
            d: a[0].plain().get("d").num(),
            cos_up: a[0].plain().get("cosUp").num(),
        };
        check(file, c, &Value::Bool(welds(&hit, &cfg_of(&a))), &Tol::Exact);
    }
}

fn pure_replay(file: &str, run: impl Fn(&[Resolved]) -> Value) {
    let cases = load_cases("clearance", file);
    assert!(!cases.is_empty());
    for c in &cases {
        check(file, c, &run(&resolve_args(&c.args)), &Tol::Exact);
    }
}

fn longest(a: &[Resolved]) -> Value {
    range_value(longest_run(&bools(a[0].plain())))
}
fn low_tails(a: &[Resolved]) -> Value {
    bools_value(&with_low_tails(&bools(a[0].plain()), &bools(a[1].plain())))
}
fn body(a: &[Resolved]) -> Value {
    bools_value(&body_mask(&pts3(a[0].plain()), &cfg_of(a)))
}
fn tall_b(a: &[Resolved]) -> Value {
    range_value(tall_body(&bools(a[0].plain())))
}
fn tall_s(a: &[Resolved]) -> Value {
    Value::Num(tall_span(&pts3(a[0].plain()), &bools(a[1].plain())))
}
fn min_span(a: &[Resolved]) -> Value {
    Value::Num(min_span_for(
        &pts3(a[0].plain()),
        &bools(a[1].plain()),
        flag(a, 2),
        flag(a, 3),
        &cfg_of(a),
    ))
}
fn floors(a: &[Resolved]) -> Value {
    let mut line = pts3(a[0].plain());
    insert_floor_stations(&mut line, &cfg_of(a));
    // edited in place: the same array comes back, with its properties
    with_props(pts_value(&line), props_of(a[0].plain()))
}

#[test]
fn station_is_clear_direct() {
    station_replay("stationIsClear", false);
}
#[test]
fn station_is_clear_captured() {
    station_replay("stationIsClear.captured", false);
}
#[test]
fn station_certified_direct() {
    station_replay("stationCertified", true);
}
#[test]
fn station_certified_captured() {
    station_replay("stationCertified.captured", true);
}
#[test]
fn path_to_plate_is_clear_direct() {
    path_replay("pathToPlateIsClear");
}
#[test]
fn path_to_plate_is_clear_captured() {
    path_replay("pathToPlateIsClear.captured");
}
#[test]
fn welds_direct() {
    welds_replay("welds");
}
#[test]
fn welds_captured() {
    welds_replay("welds.captured");
}
#[test]
fn longest_run_direct() {
    pure_replay("longestRun", longest);
}
#[test]
fn longest_run_captured() {
    pure_replay("longestRun.captured", longest);
}
#[test]
fn with_low_tails_direct() {
    pure_replay("withLowTails", low_tails);
}
#[test]
fn with_low_tails_captured() {
    pure_replay("withLowTails.captured", low_tails);
}
#[test]
fn body_mask_direct() {
    pure_replay("bodyMask", body);
}
#[test]
fn body_mask_captured() {
    pure_replay("bodyMask.captured", body);
}
#[test]
fn tall_body_direct() {
    pure_replay("tallBody", tall_b);
}
#[test]
fn tall_body_captured() {
    pure_replay("tallBody.captured", tall_b);
}
#[test]
fn tall_span_direct() {
    pure_replay("tallSpan", tall_s);
}
#[test]
fn tall_span_captured() {
    pure_replay("tallSpan.captured", tall_s);
}
#[test]
fn min_span_for_direct() {
    pure_replay("minSpanFor", min_span);
}
#[test]
fn min_span_for_captured() {
    pure_replay("minSpanFor.captured", min_span);
}
#[test]
fn insert_floor_stations_direct() {
    pure_replay("insertFloorStations", floors);
}
#[test]
fn insert_floor_stations_captured() {
    pure_replay("insertFloorStations.captured", floors);
}
