//! Replay of `parity/corpus/judge/` (recorded by `parity/oracle/judge.js` from web/prop/judge.js).
//! `scorer` and `judgeRuler` return functions in JS: their cases apply the function to recorded
//! points / faces. Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::judge::*;
use sf_testkit::parity::{Resolved, resolve_args};
use sf_testkit::parity_config::config;

fn pos_of(v: &Value) -> Vec<f64> {
    match v {
        Value::F32(a) => a.iter().map(|&x| x as f64).collect(),
        o => panic!("pos: {o:?}"),
    }
}

fn pair(v: &Value) -> [usize; 2] {
    let n = nums(v);
    [n[0] as usize, n[1] as usize]
}

fn setup(
    a: &[Resolved],
) -> (
    &sf_engine::overhangs::Topology,
    sf_engine::overhangs::Analysis,
    sf_engine::types::Rot3,
) {
    (a[0].topo(), analysis_full(&a[1]).0, rot3(a[2].plain()))
}

fn part_anchors_replay(file: &str) {
    let cases = load_cases("judge", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (topo, res, rot) = setup(&a);
        let cache = JudgeCache::default();
        check(
            file,
            c,
            &pts_value(part_anchors(topo, &res, &rot, &cache)),
            &Tol::Exact,
        );
    }
}

fn face_samples_replay(file: &str) {
    let cases = load_cases("judge", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (topo, res, rot) = setup(&a);
        let cache = JudgeCache::default();
        let s = face_samples(
            topo,
            &res,
            &rot,
            a[3].plain().num() as usize,
            &cache,
            &cfg_of(&a),
        );
        let actual = Value::Arr(s.iter().map(|p| Value::from(p.to_vec())).collect());
        check(file, c, &actual, &Tol::Exact);
    }
}

fn attached_replay(file: &str) {
    let cases = load_cases("judge", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (topo, res, rot) = setup(&a);
        let r = attached(
            topo,
            &res,
            &rot,
            a[3].plain().num() as usize,
            a[4].plain().num() as usize,
            &pair(a[5].plain()),
            &p3(a[6].plain()),
            &p3(a[7].plain()),
        );
        check(file, c, &Value::Bool(r), &Tol::Exact);
    }
}

#[test]
fn part_anchors_direct() {
    part_anchors_replay("partAnchors");
}
#[test]
fn part_anchors_captured() {
    part_anchors_replay("partAnchors.captured");
}
#[test]
fn face_samples_direct() {
    face_samples_replay("faceSamples");
}
#[test]
fn face_samples_captured() {
    face_samples_replay("faceSamples.captured");
}
#[test]
fn attached_direct() {
    attached_replay("attached");
}
#[test]
fn attached_captured() {
    attached_replay("attached.captured");
}

#[test]
fn scorer_direct() {
    let cases = load_cases("judge", "scorer");
    assert!(!cases.is_empty());
    for c in &cases {
        let tops: Vec<Vec<[f64; 3]>> = c.args[0].arr().iter().map(pts3).collect();
        let part = pts3(&c.args[1]);
        let pts = pts3(&c.args[2]);
        let cfg = config(c.args[3].get("$config"));
        let s = scorer(&tops, &part, &cfg);
        let actual = Value::Arr(
            pts.iter()
                .map(|q| {
                    Value::from(match s.score(q[0], q[1], q[2]) {
                        Hold::Near => "near",
                        Hold::Bridged => "bridged",
                        Hold::Cant => "cant",
                        Hold::Bare => "bare",
                    })
                })
                .collect(),
        );
        check("scorer", c, &actual, &Tol::Exact);
    }
}

#[test]
fn judge_ruler_direct() {
    let cases = load_cases("judge", "judgeRuler");
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (topo, res, rot) = setup(&a);
        let cache = JudgeCache::default();
        let cfg = cfg_of(&a);
        let ruler = judge_ruler(topo, &res, &rot, &cache, &cfg);
        let faces: Vec<[f64; 6]> = a[3]
            .plain()
            .arr()
            .iter()
            .map(|f| nums(f).try_into().expect("6 numbers"))
            .collect();
        let tops = pts3(a[4].plain());
        check(
            "judgeRuler",
            c,
            &Value::Num(ruler(&faces, &tops)),
            &Tol::Exact,
        );
    }
}

#[test]
fn shared_edge_direct() {
    let cases = load_cases("judge", "sharedEdge");
    assert!(!cases.is_empty());
    for c in &cases {
        let r = shared_edge(
            &pos_of(&c.args[0]),
            c.args[1].num() as usize,
            c.args[2].num() as usize,
        );
        let actual = r.map_or(Value::Null, |e| Value::from(vec![e[0] as f64, e[1] as f64]));
        check("sharedEdge", c, &actual, &Tol::Exact);
    }
}

#[test]
fn other_vertex_direct() {
    let cases = load_cases("judge", "otherVertex");
    assert!(!cases.is_empty());
    for c in &cases {
        let v = other_vertex(
            &pos_of(&c.args[0]),
            c.args[1].num() as usize,
            &pair(&c.args[2]),
        );
        check("otherVertex", c, &Value::Num(v as f64), &Tol::Exact);
    }
}
