//! Replay of `parity/corpus/sway/*` (recorded by `parity/oracle/sway.js` from web/sway.js):
//! `build_sway_rib`, `build_sway_braces`, `sway_at_face`, `sway_clashes`, `sway_clashes_wall` and
//! `face_is_upright`, on synthetic parts (`*.soup`: the replay builds the topology and the analysis
//! from the recorded soup), on the stress models and in the golden scene that turns sway on. Exact.
mod common;

use ::parity::{Case, Tol, Value, load_cases};
use common::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{SwayArm, SwayLevel};
use sf_engine::fins::seating::seated_part_tris;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::planes::find_wall_patches;
use sf_engine::sway::*;
use sf_engine::types::{Rot3, V4, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};

type V3 = [f64; 3];

fn pts_of(v: &Value) -> Vec<V3> {
    v.arr().iter().map(p3).collect()
}

/// The walls of an `avoid`: polylines of `[x, y, z]` (a recorded wall may be empty).
fn walls_of(v: &Value) -> Vec<Vec<V3>> {
    v.arr().iter().map(pts_of).collect()
}

fn pt2s(v: &Value) -> Vec<[f64; 2]> {
    v.arr()
        .iter()
        .map(|q| {
            let n = nums(q);
            [n[0], n[1]]
        })
        .collect()
}

/// A recorded brace already standing (the fields the clash checks read; triangles dropped).
fn rib_of(v: &Value) -> SwayRib {
    SwayRib {
        tris: vec![],
        tines: 0,
        height: get(v, "height").unwrap().num(),
        depth: 0.0,
        th: get(v, "th").unwrap().num(),
        stilt: 0.0,
        foot: {
            let f = pts_of(get(v, "foot").unwrap());
            [f[0], f[1]]
        },
        half_w: get(v, "halfW").unwrap().num(),
        levels: get(v, "levels")
            .unwrap()
            .arr()
            .iter()
            .map(|l| SwayLevel {
                z: l.get("z").num(),
                a: p3(l.get("a")),
                b: p3(l.get("b")),
            })
            .collect(),
        cross: 0,
        arms: get(v, "arms")
            .unwrap()
            .arr()
            .iter()
            .map(|a| SwayArm {
                a: p3(a.get("a")),
                b: p3(a.get("b")),
                th: a.get("th").num(),
                plan: pt2s(a.get("plan")),
            })
            .collect(),
        profiled: get(v, "profiled").is_some(),
        tri_range: None,
    }
}

/// `avoid`: `[]`, `[rib...]` or `{ braces?, walls? }`.
fn avoid_of(v: &Value) -> (Vec<SwayRib>, Vec<Vec<V3>>) {
    match v {
        Value::Arr(a) => (a.iter().map(rib_of).collect(), vec![]),
        Value::Obj(_) => (
            get(v, "braces")
                .map(|b| b.arr().iter().map(rib_of).collect())
                .unwrap_or_default(),
            get(v, "walls").map(walls_of).unwrap_or_default(),
        ),
        other => panic!("avoid: {other:?}"),
    }
}

fn opt_num(v: &Value, k: &str) -> Option<f64> {
    get(v, k).and_then(|x| match x {
        Value::Num(n) => Some(*n),
        _ => None,
    })
}

fn is_true(v: &Value, k: &str) -> bool {
    matches!(get(v, k), Some(Value::Bool(true)))
}

/// The `opts` object (a key that is `undefined` or not a number counts as absent).
fn opts_of<'a>(v: &Value, avoid: Avoid<'a>) -> SwayOpts<'a> {
    SwayOpts {
        tines: get(v, "tines").and_then(|x| match x {
            Value::Bool(b) => Some(*b),
            _ => None,
        }),
        layer_height: opt_num(v, "layerHeight"),
        gap: opt_num(v, "gap"),
        tine_contact_offset: opt_num(v, "tineContactOffset"),
        grip_from: opt_num(v, "gripFrom"),
        tine_spacing: opt_num(v, "tineSpacing"),
        reach: opt_num(v, "reach"),
        uncapped_depth: is_true(v, "uncappedDepth"),
        round_feet: is_true(v, "roundFeet"),
        allow_stilt: get(v, "allowStilt").and_then(|x| match x {
            Value::Bool(b) => Some(*b),
            _ => None,
        }),
        max_height: opt_num(v, "maxHeight"),
        iface_all: matches!(get(v, "iface"), Some(Value::Str(s)) if s == "all"),
        cross_above: opt_num(v, "crossAbove"),
        cross_clear: None,
        cross_rise: opt_num(v, "crossRise"),
        avoid,
        full: is_true(v, "full"),
    }
}

fn tag_flat(tris: &[V4]) -> Value {
    Value::F64(
        tris.iter()
            .flat_map(|v| [v[0], v[1], v[2], if v[3] == 1.0 { 1.0 } else { 0.0 }])
            .collect(),
    )
}

fn v3(p: V3) -> Value {
    Value::from(p.to_vec())
}

fn rib_value(r: &SwayRib, with_tris: bool) -> Value {
    let mut o: Vec<(String, Value)> = vec![("ok".into(), Value::Bool(true))];
    if with_tris {
        o.push(("tris".into(), tag_flat(&r.tris)));
    }
    o.push(("tines".into(), Value::Num(r.tines as f64)));
    o.push(("height".into(), Value::Num(r.height)));
    o.push(("depth".into(), Value::Num(r.depth)));
    o.push(("th".into(), Value::Num(r.th)));
    o.push(("stilt".into(), Value::Num(r.stilt)));
    o.push((
        "foot".into(),
        Value::Arr(vec![v3(r.foot[0]), v3(r.foot[1])]),
    ));
    o.push(("halfW".into(), Value::Num(r.half_w)));
    o.push((
        "levels".into(),
        Value::Arr(
            r.levels
                .iter()
                .map(|l| {
                    Value::Obj(vec![
                        ("z".into(), Value::Num(l.z)),
                        ("a".into(), v3(l.a)),
                        ("b".into(), v3(l.b)),
                    ])
                })
                .collect(),
        ),
    ));
    o.push(("cross".into(), Value::Num(r.cross as f64)));
    o.push((
        "arms".into(),
        Value::Arr(
            r.arms
                .iter()
                .map(|a| {
                    Value::Obj(vec![
                        ("a".into(), v3(a.a)),
                        ("b".into(), v3(a.b)),
                        ("th".into(), Value::Num(a.th)),
                        (
                            "plan".into(),
                            Value::Arr(a.plan.iter().map(|q| Value::from(q.to_vec())).collect()),
                        ),
                    ])
                })
                .collect(),
        ),
    ));
    if r.profiled {
        o.push(("profiled".into(), Value::Bool(true)));
    }
    if let Some(t) = r.tri_range {
        o.push((
            "triRange".into(),
            Value::Arr(vec![Value::Num(t[0] as f64), Value::Num(t[1] as f64)]),
        ));
    }
    Value::Obj(o)
}

fn result_value(r: &Result<SwayRib, String>) -> Value {
    match r {
        Ok(rib) => rib_value(rib, true),
        Err(reason) => Value::Obj(vec![
            ("ok".into(), Value::Bool(false)),
            ("reason".into(), Value::Str(reason.clone())),
        ]),
    }
}

fn braces_value(s: &SwayBraces) -> Value {
    Value::Obj(vec![
        ("triangles".into(), tag_flat(&s.triangles)),
        ("count".into(), Value::Num(s.count as f64)),
        ("tines".into(), Value::Num(s.tines as f64)),
        ("skipped".into(), Value::Num(s.skipped as f64)),
        (
            "ribs".into(),
            Value::Arr(s.ribs.iter().map(|r| rib_value(r, false)).collect()),
        ),
        (
            "reason".into(),
            s.reason.clone().map_or(Value::Null, Value::Str),
        ),
    ])
}

/// The synthetic part of a `*.soup` case: its topology and analysis at `thr` in `rot`.
fn soup(a: &[Resolved]) -> (Topology, Rot3, Analysis) {
    let pos: Vec<f64> = match a[0].plain() {
        Value::F32(p) => p.iter().map(|&x| x as f64).collect(),
        other => panic!("soup: {other:?}"),
    };
    let topo = build_topology(pos);
    let rot = rot3(a[2].plain());
    let res = analyze(&topo, a[1].plain().num(), &rot);
    (topo, rot, res)
}

/// `{ offset }`: all that `buildSwayBraces` reads of the analyze result.
fn offset_only(v: &Value) -> Analysis {
    Analysis {
        over: vec![],
        kept: vec![],
        on_bed: vec![],
        regions: vec![],
        raw_region_count: 0,
        over_area: 0.0,
        bed_area: 0.0,
        over_face_count: 0,
        offset: xyz(v.get("offset")),
        size: Xyz::default(),
    }
}

fn replay(file: &str, mut f: impl FnMut(&Case, &[Resolved], &Config) -> Value) -> usize {
    let cases = load_cases("sway", file);
    assert!(!cases.is_empty(), "{file}");
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        check(file, c, &f(c, &a, &cfg), &Tol::Exact);
    }
    cases.len()
}

#[test]
fn build_sway_braces_soup() {
    let mut placed = 0;
    let n = replay("buildSwayBraces.soup", |_, a, cfg| {
        let (topo, rot, res) = soup(a);
        let walls = get(a[3].plain(), "avoid")
            .and_then(|v| get(v, "walls"))
            .map(walls_of)
            .unwrap_or_default();
        let opts = opts_of(
            a[3].plain(),
            Avoid {
                braces: &[],
                walls: &walls,
            },
        );
        let s = build_sway_braces(&topo, &res, &rot, &opts, cfg);
        placed += s.count;
        braces_value(&s)
    });
    assert!(n > 150 && placed > 100, "{n} cases, {placed} braces");
}

#[test]
fn build_sway_rib_soup() {
    let (mut ok, mut cross, mut profiled) = (0, 0, 0);
    let n = replay("buildSwayRib.soup", |_, a, cfg| {
        let (topo, rot, res) = soup(a);
        let patches = find_wall_patches(&topo, &rot, &res.offset, None, None);
        let p = &patches[a[3].plain().num() as usize];
        let part = seated_part_tris(&topo, &rot, &res.offset);
        let opts = opts_of(a[5].plain(), Avoid::default());
        let r = build_sway_rib(
            p,
            a[4].plain().num(),
            &part,
            &topo,
            &rot,
            &res.offset,
            &opts,
            cfg,
        );
        if let Ok(rib) = &r {
            ok += 1;
            cross += rib.cross;
            profiled += rib.profiled as usize;
        }
        result_value(&r)
    });
    assert!(n > 1000 && ok > 500 && cross > 50 && profiled > 20);
}

#[test]
fn sway_at_face_soup() {
    let (mut ok, mut refused) = (0, 0);
    let n = replay("swayAtFace.soup", |_, a, cfg| {
        let (topo, rot, res) = soup(a);
        let (braces, walls) = avoid_of(a[6].plain());
        let opts = opts_of(a[5].plain(), Avoid::default());
        let r = sway_at_face(
            &topo,
            &res,
            &rot,
            a[3].plain().num() as usize,
            p3(a[4].plain()),
            &opts,
            &Avoid {
                braces: &braces,
                walls: &walls,
            },
            cfg,
        );
        if r.is_ok() {
            ok += 1;
        } else {
            refused += 1;
        }
        result_value(&r)
    });
    assert!(n > 200 && ok > 50 && refused > 50);
}

#[test]
fn sway_clashes_soup() {
    let mut clashed = 0;
    replay("swayClashes.soup", |_, a, cfg| {
        let ribs: Vec<SwayRib> = a[1].plain().arr().iter().map(rib_of).collect();
        let r = sway_clashes(&rib_of(a[0].plain()), &ribs, cfg);
        clashed += r as usize;
        Value::Bool(r)
    });
    assert!(clashed > 0);
}

#[test]
fn sway_clashes_wall_soup() {
    let mut clashed = 0;
    replay("swayClashesWall.soup", |_, a, cfg| {
        let r = sway_clashes_wall(&rib_of(a[0].plain()), &walls_of(a[1].plain()), cfg);
        clashed += r as usize;
        Value::Bool(r)
    });
    assert!(clashed > 0);
}

fn models_replay(file: &str, topo_at: usize, rot_at: usize, off_at: usize, opts_at: usize) {
    let mut placed = 0;
    replay(file, |_, a, cfg| {
        let walls = get(a[opts_at].plain(), "avoid")
            .and_then(|v| get(v, "walls"))
            .map(walls_of)
            .unwrap_or_default();
        let opts = opts_of(
            a[opts_at].plain(),
            Avoid {
                braces: &[],
                walls: &walls,
            },
        );
        let s = build_sway_braces(
            a[topo_at].topo(),
            &offset_only(a[off_at].plain()),
            &rot3(a[rot_at].plain()),
            &opts,
            cfg,
        );
        placed += s.count;
        braces_value(&s)
    });
    let _ = placed;
}

#[test]
fn build_sway_braces_on_models() {
    models_replay("buildSwayBraces", 0, 1, 2, 3);
}

#[test]
fn build_sway_braces_captured() {
    let mut placed = 0;
    replay("buildSwayBraces.captured", |_, a, cfg| {
        let opts = opts_of(a[3].plain(), Avoid::default());
        let walls = get(a[3].plain(), "avoid")
            .and_then(|v| get(v, "walls"))
            .map(walls_of)
            .unwrap_or_default();
        let opts = SwayOpts {
            avoid: Avoid {
                braces: &[],
                walls: &walls,
            },
            ..opts
        };
        let s = build_sway_braces(
            a[0].topo(),
            &offset_only(a[2].plain()),
            &rot3(a[1].plain()),
            &opts,
            cfg,
        );
        placed += s.count;
        braces_value(&s)
    });
    assert!(placed > 0);
}

#[test]
fn face_is_upright_replay() {
    let mut up = 0;
    let n = replay("faceIsUpright", |_, a, cfg| {
        let r = face_is_upright(
            a[0].topo(),
            &rot3(a[1].plain()),
            a[2].plain().num() as usize,
            cfg,
        );
        up += r as usize;
        Value::Bool(r)
    });
    assert!(n > 1000 && up > 100);
}
