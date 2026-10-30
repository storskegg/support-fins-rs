//! Replay of `parity/corpus/shortwalls/*` (recorded by `parity/oracle/shortwalls.js` from
//! web/fins/shortwalls.js): `last_resort_walls` on the stress models (nothing served / what
//! `buildProps` served and built) and as the 54 golden scenes' Auto builds call it. Exact.
mod common;

use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::fins::coverage::automatic_targets;
use sf_engine::fins::shortwalls::last_resort_walls;
use sf_engine::prop::build::BuildOpts;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::V4;
use sf_testkit::parity::{Resolved, resolve_args};
use std::sync::Arc;

fn num_opt(v: &Value, k: &str) -> Option<f64> {
    match get(v, k) {
        Some(Value::Num(n)) => Some(*n),
        _ => None,
    }
}

fn bool_opt(v: &Value, k: &str) -> Option<bool> {
    match get(v, k) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// The crest state `{part, layerHeight}`: a `$seated` part, or a plain value (`null`).
fn crest_of(state: &Resolved, cfg: &sf_engine::config::Config) -> Crest {
    let (part, layer): (Option<Arc<Tris>>, f64) = match state {
        Resolved::Plain(v) => (
            match v.get("part") {
                Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
                Value::Null => None,
                other => panic!("crest part {other:?}"),
            },
            match v.get("layerHeight") {
                Value::Num(n) => *n,
                _ => 0.0,
            },
        ),
        r => (
            match r.get("part") {
                Resolved::Seated(t) => Some(t.clone()),
                other => panic!("crest part {other:?}"),
            },
            match r.get("layerHeight") {
                Resolved::Plain(Value::Num(n)) => *n,
                _ => 0.0,
            },
        ),
    };
    Crest::new(part, layer, cfg)
}

fn xyz4(v: &Value) -> Vec<V4> {
    nums(v).chunks(3).map(|c| [c[0], c[1], c[2], 0.0]).collect()
}

fn replay(file: &str) -> (usize, usize) {
    let cases = load_cases("shortwalls", file);
    assert!(!cases.is_empty());
    let mut with_walls = 0;
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let rot = rot3(a[2].plain());
        let topo = a[0].topo();
        let targets = automatic_targets(topo, a[1].analysis(), &rot);
        let o = a[3].plain();
        let opts = BuildOpts {
            step: num_opt(o, "step"),
            coverage: num_opt(o, "coverage"),
            tines: bool_opt(o, "tines") == Some(true),
            tine_density: num_opt(o, "tineDensity"),
            layer_height: num_opt(o, "layerHeight"),
            raster: bool_opt(o, "raster"),
            sway_on: get(o, "sway").is_some_and(|s| bool_opt(s, "on") == Some(true)),
            rim_walls: bool_opt(o, "rimWalls"),
            downhill: bool_opt(o, "downhill"),
            curved_walls: bool_opt(o, "curvedWalls"),
            plate_only: bool_opt(o, "plateOnly") == Some(true),
            excluded_support_faces: Some(Arc::new(targets.excluded_support_faces.clone())),
            ..Default::default()
        };
        let served: Vec<usize> = nums(a[4].plain()).iter().map(|&x| x as usize).collect();
        let ctx = BuildCtx::new(&cfg, crest_of(&a[7], &cfg));
        let r = last_resort_walls(
            topo,
            &targets.analysis,
            &rot,
            &opts,
            &served,
            &xyz4(a[5].plain()),
            &xyz4(a[6].plain()),
            &ctx,
        );
        if !r.props.is_empty() {
            with_walls += 1;
        }
        let actual = Value::Obj(vec![
            (
                "tris".into(),
                Value::F64(
                    r.triangles
                        .iter()
                        .flat_map(|v| [v[0], v[1], v[2], if v[3] == 1.0 { 1.0 } else { 0.0 }])
                        .collect(),
                ),
            ),
            (
                "props".into(),
                Value::Arr(
                    r.props
                        .iter()
                        .map(|q| {
                            Value::Obj(vec![
                                (
                                    "region".into(),
                                    q.region.map_or(Value::Null, |r| Value::Num(r as f64)),
                                ),
                                ("id".into(), Value::Num(q.id as f64)),
                                ("tines".into(), Value::Num(q.tines as f64)),
                                ("span".into(), Value::Num(q.span)),
                                ("height".into(), Value::Num(q.height)),
                                ("line".into(), pts_value(&q.line)),
                                (
                                    "triRanges".into(),
                                    Value::Arr(
                                        q.tri_ranges
                                            .iter()
                                            .map(|t| num_value(&[t[0] as f64, t[1] as f64]))
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "served".into(),
                Value::Arr(r.served.iter().map(|&s| Value::Num(s as f64)).collect()),
            ),
            ("tines".into(), Value::Num(r.tines as f64)),
        ]);
        check(file, c, &actual, &Tol::Exact);
    }
    (cases.len(), with_walls)
}

#[test]
fn last_resort_walls_on_the_stress_models() {
    let (n, with_walls) = replay("lastResortWalls");
    assert!(
        n >= 200 && with_walls >= 40,
        "{n} cases, {with_walls} with walls"
    );
}

#[test]
fn last_resort_walls_in_the_golden_scenes() {
    let (n, _) = replay("lastResortWalls.captured");
    assert!(n >= 20);
}
