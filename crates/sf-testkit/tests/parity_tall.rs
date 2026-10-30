//! Replay of `parity/corpus/tall/*` (recorded by `parity/oracle/tall.js` from web/fins/tall.js):
//! `PlanGrid` (random triangles, tags, `keep`, the `null`/`undefined` tag semantics) and
//! `brace_tall_walls` on real builds (the stress models with the ribs off, `BRACE.tallH` lowered so
//! walls qualify) and in the golden scenes. Exact.
mod common;

use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt};
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::fins::tall::{PlanGrid, brace_tall_walls};
use sf_engine::overhangs::Analysis;
use sf_engine::types::{V4, Xyz};
use sf_testkit::parity::resolve_args;

fn v4s(flat: &Value) -> Vec<V4> {
    nums(flat)
        .chunks(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect()
}

fn v3s(flat: &Value) -> Vec<V4> {
    nums(flat)
        .chunks(3)
        .map(|c| [c[0], c[1], c[2], 0.0])
        .collect()
}

fn usize_of(v: &Value) -> Option<usize> {
    match v {
        Value::Num(n) => Some(*n as usize),
        _ => None,
    }
}

#[test]
fn plan_grid_replay() {
    let cases = load_cases("tall", "planGrid");
    assert!(cases.len() >= 50);
    let mut hit = 0;
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let mut g = PlanGrid::new();
        for add in a[0].plain().arr() {
            let vs = v3s(add.get("vs"));
            let tag = usize_of(add.get("tag"));
            match usize_of(add.get("keepBelow")) {
                Some(_) => {
                    let below = add.get("keepBelow").num();
                    g.add(&vs, tag, |p, q, r| {
                        sf_engine::jsnum::min(sf_engine::jsnum::min(p[2], q[2]), r[2]) <= below
                    });
                }
                None => g.add(&vs, tag, |_, _, _| true),
            }
        }
        let out: Vec<Value> = a[1]
            .plain()
            .arr()
            .iter()
            .map(|q| {
                let r = g.hits(&v3s(q.get("vs")), usize_of(q.get("own")), &cfg);
                hit += r as usize;
                Value::Bool(r)
            })
            .collect();
        check("planGrid", c, &Value::Arr(out), &Tol::Exact);
    }
    assert!(hit > 20);
}

/// `built` of a recorded call: only the fields `braceTallWalls` reads.
fn built_of(v: &Value) -> FinsBuilt {
    let mut b = FinsBuilt::empty(Seating {
        kind: SeatingKind::Face,
        span: 0.0,
        bed_area: 0.0,
    });
    b.triangles = v4s(v.get("triangles"));
    b.pad_triangles = v4s(v.get("padTriangles"));
    b.fins = v
        .get("fins")
        .arr()
        .iter()
        .enumerate()
        .map(|(i, f)| FinRecord {
            height: f.get("height").num(),
            length: 0.0,
            tines: 0,
            rows: 0,
            stilt: 0.0,
            lean: 0.0,
            bearing: 0.0,
            site: None,
            id: i,
            kind: match f.get("kind").str() {
                "prop" => FinKind::Prop,
                "wedge" => FinKind::Wedge,
                _ => FinKind::Sway,
            },
            tri_ranges: f
                .get("triRanges")
                .arr()
                .iter()
                .map(|r| {
                    let n = nums(r);
                    [n[0] as usize, n[1] as usize]
                })
                .collect(),
            line: pts3(f.get("line")),
            span: 0.0,
            short: false,
            fill: matches!(get(f, "fill"), Some(Value::Bool(true))),
            joined: false,
            braces: get(f, "braces").and_then(usize_of),
            ribs: None,
        })
        .collect();
    b
}

fn replay(file: &str) -> (usize, usize) {
    let cases = load_cases("tall", file);
    assert!(!cases.is_empty());
    let (mut ribbed, mut ran) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let mut built = built_of(a[3].plain());
        let result = Analysis {
            over: vec![],
            kept: vec![],
            on_bed: vec![],
            regions: vec![],
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: 0.0,
            over_face_count: 0,
            offset: xyz(a[1].plain().get("offset")),
            size: Xyz::default(),
        };
        brace_tall_walls(a[0].topo(), &result, &rot3(a[2].plain()), &mut built, &cfg);
        ran += 1;
        if built.tall_ribs.unwrap_or(0) > 0 {
            ribbed += 1;
        }
        let fins: Vec<Value> = built
            .fins
            .iter()
            .map(|f| {
                let mut o = vec![(
                    "triRanges".to_string(),
                    Value::Arr(
                        f.tri_ranges
                            .iter()
                            .map(|r| num_value(&[r[0] as f64, r[1] as f64]))
                            .collect(),
                    ),
                )];
                if let Some(n) = f.ribs {
                    o.push(("ribs".to_string(), Value::Num(n)));
                }
                Value::Obj(o)
            })
            .collect();
        let actual = Value::Obj(vec![
            (
                "tris".into(),
                Value::F64(
                    built
                        .triangles
                        .iter()
                        .flat_map(|v| [v[0], v[1], v[2], if v[3] == 1.0 { 1.0 } else { 0.0 }])
                        .collect(),
                ),
            ),
            (
                "tallRibs".into(),
                Value::Num(built.tall_ribs.unwrap() as f64),
            ),
            ("fins".into(), Value::Arr(fins)),
        ]);
        check(file, c, &actual, &Tol::Exact);
    }
    (ran, ribbed)
}

#[test]
fn brace_tall_walls_on_real_builds() {
    let (ran, ribbed) = replay("braceTallWalls");
    assert!(ran >= 20 && ribbed >= 10, "{ran} cases, {ribbed} ribbed");
}

#[test]
fn brace_tall_walls_in_the_golden_scenes() {
    let (ran, _) = replay("braceTallWalls.captured");
    assert!(ran >= 5);
}
