//! THE MILESTONE'S EXIT TEST: `build_props` / `build_short_walls` against `parity/corpus/build/*`
//! (recorded by `parity/oracle/build.js` from web/prop.js). Every stress model x poses x option set x
//! {interface crest off, flat} (direct), and the calls the 54 golden scenes' builds make
//! (captured). `Tol::Exact`; the triangles are compared by a hash of every vertex's four float64s
//! (and in full for the cases that carry them), the walls' records field by field.
mod common;
use ::parity::{Case, Tol, Value, compare, load_cases};
use common::*;
use indexmap::IndexSet;
use sf_engine::prop::build::*;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::ctx::TineSeed;
use sf_engine::tris::Tris;
use sf_testkit::parity::{Resolved, resolve_args};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

fn opt<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(kv) => kv.iter().find(|(kk, _)| kk == k).map(|(_, v)| v),
        _ => None,
    }
}

fn num_opt(v: &Value, k: &str) -> Option<f64> {
    match opt(v, k) {
        Some(Value::Num(n)) => Some(*n),
        _ => None,
    }
}

fn bool_opt(v: &Value, k: &str) -> Option<bool> {
    match opt(v, k) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn crest_of(state: &Resolved, cfg: &sf_engine::config::Config) -> Crest {
    let plain_part = |v: &Value| match v {
        Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
        Value::Null => None,
        o => panic!("crest part {o:?}"),
    };
    let (part, lh) = match state {
        Resolved::Obj(_) => (
            match state.get("part") {
                Resolved::Seated(t) => Some(t.clone()),
                Resolved::Plain(v) => plain_part(v),
                o => panic!("crest part {o:?}"),
            },
            state.get("layerHeight").plain().clone(),
        ),
        Resolved::Plain(v) => (plain_part(v.get("part")), v.get("layerHeight").clone()),
        o => panic!("crest {o:?}"),
    };
    let lh = match lh {
        Value::Num(n) => n,
        _ => 0.0,
    };
    Crest::new(part, lh, cfg)
}

/// The vertex hash of `oracle/build.js`.
fn hash(tris: &[[f64; 4]]) -> (u32, u32) {
    let (mut h1, mut h2) = (0x811c9dc5u32, 0x9747b28cu32);
    for v in tris {
        for x in v {
            let b = x.to_bits();
            for w in [b as u32, (b >> 32) as u32] {
                h1 = (h1 ^ w).wrapping_mul(16777619);
                h2 = (h2 ^ w).wrapping_mul(0x01000193) ^ (h2 >> 13);
            }
        }
    }
    (h1, h2)
}

fn plain_of(r: &Resolved) -> Option<&Value> {
    match r {
        Resolved::Plain(v) => Some(v),
        _ => None,
    }
}

fn n(v: f64) -> Value {
    Value::from(v)
}

fn pair(p: [usize; 2]) -> Value {
    num_value(&[p[0] as f64, p[1] as f64])
}

fn cap_summary(ctx: &BuildCtx) -> Value {
    match ctx.tine_cap.borrow().as_ref() {
        None => Value::Null,
        Some(c) => {
            let flat: Vec<u64> = c
                .iter()
                .flat_map(|s| [s.x, s.y, s.z, s.bite_x, s.bite_y])
                .map(f64::to_bits)
                .collect();
            let (mut h1, mut h2) = (0x811c9dc5u32, 0x9747b28cu32);
            for b in flat {
                for w in [b as u32, (b >> 32) as u32] {
                    h1 = (h1 ^ w).wrapping_mul(16777619);
                    h2 = (h2 ^ w).wrapping_mul(0x01000193) ^ (h2 >> 13);
                }
            }
            Value::Obj(vec![
                ("n".into(), Value::from(c.len() as f64)),
                (
                    "hash".into(),
                    Value::Arr(vec![Value::from(h1 as f64), Value::from(h2 as f64)]),
                ),
            ])
        }
    }
}

fn summary(r: &PropsPass, full: bool, cap: Value) -> Value {
    let (h1, h2) = hash(&r.triangles);
    let tagged = r.triangles.iter().filter(|v| v[3] == 1.0).count();
    let props: Vec<Value> = r
        .props
        .iter()
        .map(|q| {
            let tag = |t: Option<bool>| t.map_or(Value::Null, Value::from);
            Value::Obj(vec![
                (
                    "region".into(),
                    q.region.map_or(Value::Null, |r| n(r as f64)),
                ),
                ("id".into(), n(q.id as f64)),
                ("tines".into(), n(q.tines as f64)),
                ("caps".into(), q.caps.map_or(Value::Null, pair)),
                ("grip".into(), q.grip.map_or(Value::Null, n)),
                ("span".into(), n(q.span)),
                ("height".into(), n(q.height)),
                ("area".into(), n(q.area)),
                ("stations".into(), n(q.stations as f64)),
                (
                    "trimmed".into(),
                    q.trimmed.map_or(Value::Null, |t| n(t as f64)),
                ),
                ("volume".into(), n(q.volume)),
                ("line".into(), pts_value(&q.line)),
                (
                    "triRanges".into(),
                    Value::Arr(q.tri_ranges.iter().map(|r| pair(*r)).collect()),
                ),
                ("triRange".into(), q.tri_range.map_or(Value::Null, pair)),
                ("wallRange".into(), q.wall_range.map_or(Value::Null, pair)),
                (
                    "members".into(),
                    q.members.as_ref().map_or(Value::Null, |m| {
                        Value::Arr(m.iter().map(|&x| n(x as f64)).collect())
                    }),
                ),
                ("rimWall".into(), Value::from(q.rim_wall)),
                ("partAttached".into(), Value::from(q.part_attached)),
                ("inBore".into(), tag(q.in_bore)),
                ("squat".into(), Value::from(q.squat)),
                (
                    "tags".into(),
                    Value::Arr(vec![
                        tag(q.raster),
                        tag(q.circular),
                        tag(q.rim),
                        tag(q.downhill),
                        tag(q.curved),
                    ]),
                ),
            ])
        })
        .collect();
    let mut kv = vec![
        ("nTriangles".to_string(), n(r.triangles.len() as f64)),
        ("hash".into(), Value::Arr(vec![n(h1 as f64), n(h2 as f64)])),
        ("tagged".into(), n(tagged as f64)),
        ("props".into(), Value::Arr(props)),
        (
            "skipped".into(),
            Value::Obj(
                Skipped::KEYS
                    .iter()
                    .map(|k| (k.to_string(), n(r.skipped.get(k) as f64)))
                    .collect(),
            ),
        ),
        ("served".into(), n(r.served as f64)),
        (
            "servedRegions".into(),
            Value::Arr(r.served_regions.iter().map(|&x| n(x as f64)).collect()),
        ),
        ("tines".into(), n(r.tines as f64)),
        ("sagRisk".into(), Value::from(r.sag_risk)),
        (
            "sagRegions".into(),
            r.sag_regions.as_ref().map_or(Value::Null, |s| {
                Value::Arr(s.iter().map(|&x| n(x as f64)).collect())
            }),
        ),
        (
            "leaderOf".into(),
            r.leader_of.as_ref().map_or(Value::Null, |m| {
                Value::Arr(
                    m.iter()
                        .map(|(&k, &l)| num_value(&[k as f64, l as f64]))
                        .collect(),
                )
            }),
        ),
        ("volume".into(), n(r.volume)),
    ];
    if full {
        kv.push((
            "triangles".into(),
            Value::F64(r.triangles.iter().flatten().copied().collect()),
        ));
    }
    kv.push(("cap".into(), cap));
    Value::Obj(kv)
}

fn opts_of(
    plain: &Value,
    excluded: Option<Vec<u8>>,
) -> (BuildOpts, Option<Rc<RefCell<VecDeque<bool>>>>) {
    let mut o = BuildOpts {
        step: num_opt(plain, "step"),
        coverage: num_opt(plain, "coverage"),
        tines: bool_opt(plain, "tines") == Some(true),
        tine_density: num_opt(plain, "tineDensity"),
        layer_height: num_opt(plain, "layerHeight"),
        raster: bool_opt(plain, "raster"),
        sway_on: opt(plain, "sway").is_some_and(|s| bool_opt(s, "on") == Some(true)),
        rim_walls: bool_opt(plain, "rimWalls"),
        downhill: bool_opt(plain, "downhill"),
        curved_walls: bool_opt(plain, "curvedWalls"),
        plate_only: bool_opt(plain, "plateOnly") == Some(true),
        short_walls: bool_opt(plain, "shortWalls") == Some(true),
        excluded_support_faces: excluded.map(Arc::new),
        ..Default::default()
    };
    let log = opt(plain, "rasterVetoLog").map(|l| {
        Rc::new(RefCell::new(
            l.arr()
                .iter()
                .map(|b| matches!(b, Value::Bool(true)))
                .collect::<VecDeque<bool>>(),
        ))
    });
    if let Some(log) = &log {
        let log = log.clone();
        o.raster_veto = Some(Rc::new(move |_q: &PropRecord, _p: &[PropRecord]| {
            log.borrow_mut()
                .pop_front()
                .expect("the veto was asked more often than JS asked it")
        }));
    }
    (o, log)
}

fn run(c: &Case, short: bool) -> Value {
    let a = resolve_args(&c.args);
    let cfg = cfg_of(&a);
    let rot = rot3(a[2].plain());
    // a synthetic part (`buildProps.soup`): the soup itself, analysed at 45 degrees
    let (topo, res, excluded) = match plain_of(&a[0]) {
        Some(Value::F32(p)) => {
            let topo = Arc::new(sf_engine::overhangs::build_topology(
                p.iter().map(|&x| x as f64).collect(),
            ));
            let res = sf_engine::overhangs::analyze(&topo, a[1].plain().num(), &rot);
            (topo, res, None)
        }
        _ => {
            let (res, excluded) = analysis_full(&a[1]);
            (a[0].topo().clone(), res, excluded)
        }
    };
    let (opts, log) = opts_of(a[3].plain(), excluded);
    let crest_at = if short { 5 } else { 4 };
    let ctx = BuildCtx::new(&cfg, crest_of(&a[crest_at], &cfg));
    if let Value::Arr(seeds) = a[crest_at + 1].plain() {
        *ctx.tine_cap.borrow_mut() = Some(
            seeds
                .iter()
                .map(|s| {
                    let v = nums(s);
                    TineSeed {
                        x: v[0],
                        y: v[1],
                        z: v[2],
                        bite_x: v[3],
                        bite_y: v[4],
                    }
                })
                .collect(),
        );
    }
    let pass = if short {
        let regions: IndexSet<usize> = nums(a[4].plain()).iter().map(|&x| x as usize).collect();
        build_short_walls(&topo, &res, &rot, &opts, &regions, &ctx)
    } else {
        build_props(&topo, &res, &rot, &opts, &ctx)
    };
    if let Some(log) = log {
        assert!(
            log.borrow().is_empty(),
            "{}: JS asked the veto more often than Rust",
            c.id
        );
    }
    let full = matches!(
        opt(c.out.as_ref().unwrap(), "triangles"),
        Some(Value::F64(_))
    );
    summary(&pass, full, cap_summary(&ctx))
}

fn replay(file: &str, short: bool, min: usize) {
    let cases = load_cases("build", file);
    assert!(cases.len() >= min, "{file}: {} cases", cases.len());
    let mut bad = Vec::new();
    for c in &cases {
        let got = run(c, short);
        if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            bad.push(format!(
                "{}: out{}: actual {} vs expected {}",
                c.id, m.path, m.actual, m.expected
            ));
            if bad.len() >= 5 {
                break;
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{file}: {} mismatches (first shown):\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn build_props_direct() {
    replay("buildProps", false, 500);
}

#[test]
fn build_props_captured() {
    replay("buildProps.captured", false, 50);
}

#[test]
fn build_short_walls_direct() {
    replay("buildShortWalls", true, 40);
}

#[test]
fn build_short_walls_captured() {
    replay("buildShortWalls.captured", true, 1);
}

#[test]
fn build_props_soup() {
    replay("buildProps.soup", false, 100);
}
