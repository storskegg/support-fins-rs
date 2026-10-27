//! Replay of `parity/corpus/rim/` (recorded by `parity/oracle/rim.js` from web/prop/rim.js).
//! Exact: stations are arithmetic on the seated topology and the rebuilt region soup. A captured
//! call records only the arguments it passed; the JS default `step = PROP.stationStep` fills the rest.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::rim::{clip_to_rim, rim_tracks};
use sf_testkit::parity::resolve_args;

fn tracks_value(t: &[Vec<[f64; 3]>]) -> Value {
    Value::Arr(t.iter().map(|l| pts_value(l)).collect())
}
fn tracks(v: &Value) -> Vec<Vec<[f64; 3]>> {
    v.arr().iter().map(pts3).collect()
}

fn rim_replay(file: &str) {
    let cases = load_cases("rim", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (res, _) = analysis_full(&a[1]);
        let faces: Vec<usize> = nums(a[3].plain()).iter().map(|&f| f as usize).collect();
        let cfg = cfg_of(&a);
        let js = a.len() - 1; // the trailing $config is not a JS argument
        let step = if js > 5 && !matches!(a[5].plain(), Value::Undefined) {
            a[5].plain().num()
        } else {
            cfg.prop.station_step
        };
        let out = rim_tracks(
            a[0].topo(),
            &res,
            &rot3(a[2].plain()),
            &faces,
            &tris_of(&a[4]),
            step,
            &cfg,
        );
        check(file, c, &tracks_value(&out), &Tol::Exact);
    }
}

fn clip_replay(file: &str) {
    let cases = load_cases("rim", file);
    for c in &cases {
        let a = resolve_args(&c.args);
        let out = clip_to_rim(&tracks(a[0].plain()), &tracks(a[1].plain()), &cfg_of(&a));
        check(file, c, &tracks_value(&out), &Tol::Exact);
    }
}

#[test]
fn rim_tracks_direct() {
    rim_replay("rimTracks");
}
#[test]
fn rim_tracks_captured() {
    rim_replay("rimTracks.captured");
}
#[test]
fn clip_to_rim_direct() {
    assert!(!load_cases("rim", "clipToRim").is_empty());
    clip_replay("clipToRim");
}
#[test]
fn clip_to_rim_captured() {
    clip_replay("clipToRim.captured");
}
