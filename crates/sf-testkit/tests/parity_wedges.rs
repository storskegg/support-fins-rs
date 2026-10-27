//! Replay of `parity/corpus/wedges/*` (recorded by `parity/oracle/wedges.js` from web/fins/wedges.js):
//! `perp_columns`, `build_perp_fins`, `prop_serves_patch`, `bare_after_wedges` / `unserved_after_wedges`,
//! `grip_patches` and `wedge_veto`, called directly over models and poses and as the 54 golden
//! scenes' builds call them. Exact.
mod common;
use ::parity::{Case, Tol, Value, load_cases};
use common::*;
use indexmap::IndexSet;
use sf_engine::config::Config;
use sf_engine::fins::wedges::*;
use sf_engine::overhangs::{Analysis, Region};
use sf_engine::planes::{Patch, Xy};
use sf_engine::prop::build::PropRecord;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::Xyz;
use sf_testkit::parity::{Resolved, resolve_args};
use std::sync::Arc;

fn xyz_of(v: &Value) -> Xyz {
    Xyz {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
    }
}
fn xy_of(v: &Value) -> Xy {
    Xy {
        x: v.get("x").num(),
        y: v.get("y").num(),
    }
}
fn xyz_val(p: Xyz) -> Value {
    Value::Obj(vec![
        ("x".into(), p.x.into()),
        ("y".into(), p.y.into()),
        ("z".into(), p.z.into()),
    ])
}
fn xy_val(p: Xy) -> Value {
    Value::Obj(vec![("x".into(), p.x.into()), ("y".into(), p.y.into())])
}

/// `0` when the recorded patch (a hand-made one in the synthetic cases) lacks the key.
fn opt_num(v: &Value, k: &str) -> f64 {
    match v {
        Value::Obj(o) if o.iter().any(|(key, _)| key == k) => v.get(k).num(),
        _ => 0.0,
    }
}

/// A `Patch` from a recorded patch object. The hand-made ones hold only `tris` (a `Float32Array`),
/// `u0 u1 t0 t1`, `n`, `d`, `u`, `t`.
fn patch_of(v: &Value) -> Patch {
    let has = |k: &str| matches!(v, Value::Obj(o) if o.iter().any(|(key, _)| key == k));
    Patch {
        faces: if has("faces") {
            nums(v.get("faces")).iter().map(|&f| f as usize).collect()
        } else {
            vec![]
        },
        area: opt_num(v, "area"),
        mid: if has("mid") {
            xy_of(v.get("mid"))
        } else {
            Xy::default()
        },
        n: xyz_of(v.get("n")),
        u: xy_of(v.get("u")),
        t: xyz_of(v.get("t")),
        h: opt_num(v, "h"),
        d: v.get("d").num(),
        u0: opt_num(v, "u0"),
        u1: opt_num(v, "u1"),
        t0: opt_num(v, "t0"),
        t1: opt_num(v, "t1"),
        z0: opt_num(v, "z0"),
        z1: opt_num(v, "z1"),
        lean: opt_num(v, "lean"),
        flatness: opt_num(v, "flatness"),
        tris: match v.get("tris") {
            Value::F32(a) => a.iter().map(|&x| x as f64).collect(),
            other => nums(other),
        },
    }
}

fn patch_val(p: &Patch) -> Value {
    Value::Obj(vec![
        (
            "faces".into(),
            Value::Arr(p.faces.iter().map(|&f| (f as f64).into()).collect()),
        ),
        ("area".into(), p.area.into()),
        ("mid".into(), xy_val(p.mid)),
        ("n".into(), xyz_val(p.n)),
        ("u".into(), xy_val(p.u)),
        ("t".into(), xyz_val(p.t)),
        ("h".into(), p.h.into()),
        ("d".into(), p.d.into()),
        ("u0".into(), p.u0.into()),
        ("u1".into(), p.u1.into()),
        ("t0".into(), p.t0.into()),
        ("t1".into(), p.t1.into()),
        ("z0".into(), p.z0.into()),
        ("z1".into(), p.z1.into()),
        ("lean".into(), p.lean.into()),
        ("flatness".into(), p.flatness.into()),
        ("tris".into(), Value::F64(p.tris.clone())),
    ])
}

/// The crest state `{part, layerHeight}`: with a `$seated` part it resolves as an object, otherwise
/// (`null`, or a soup recorded in full) it is a plain value.
fn crest_of(state: &Resolved, cfg: &Config) -> Crest {
    let (part, layer): (Option<Arc<Tris>>, f64) = match state {
        Resolved::Plain(v) => (
            match v.get("part") {
                Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
                Value::Null => None,
                other => panic!("crest part {other:?}"),
            },
            match v.get("layerHeight") {
                Value::Num(n) => *n,
                _ => 0.0, // undefined
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

fn opts_of(v: &Value) -> WedgeOpts {
    let get = |k: &str| match v {
        Value::Obj(o) => o.iter().find(|(key, _)| key == k).map(|(_, x)| x),
        _ => None,
    };
    WedgeOpts {
        pitch: get("pitch").map(|x| x.num()),
        tines: get("tines").map(|x| matches!(x, Value::Bool(true))),
        tine_density: get("tineDensity").map(|x| x.num()),
        layer_height: get("layerHeight").map(|x| x.num()),
    }
}

fn props_of(v: &Value) -> Vec<PropRecord> {
    v.arr()
        .iter()
        .map(|q| PropRecord {
            line: pts3(q.get("line")),
            squat: matches!(
                q,
                Value::Obj(o) if o.iter().any(|(k, x)| k == "squat" && matches!(x, Value::Bool(true)))
            ),
            ..Default::default()
        })
        .collect()
}

fn replay(file: &str, mut f: impl FnMut(&Case, &[Resolved]) -> Value) -> usize {
    let cases = load_cases("wedges", file);
    assert!(!cases.is_empty(), "{file}");
    for c in &cases {
        let a = resolve_args(&c.args);
        check(file, c, &f(c, &a), &Tol::Exact);
    }
    cases.len()
}

fn ints(v: &[usize]) -> Value {
    Value::Arr(v.iter().map(|&i| Value::Num(i as f64)).collect())
}

#[test]
fn perp_columns_replay() {
    let n = replay("perpColumns", |_, a| {
        let cfg = cfg_of(a);
        let cols = perp_columns(
            &patch_of(a[0].plain()),
            a[1].plain().num(),
            a[2].plain().num(),
            a[3].plain().num(),
            &cfg,
        );
        num_value(&cols)
    });
    assert!(n > 500);
}

#[test]
fn build_perp_fins_replay() {
    let mut built = 0;
    let n = replay("buildPerpFins", |_, a| {
        let cfg = cfg_of(a);
        let ctx = BuildCtx::new(&cfg, crest_of(&a[5], &cfg));
        let r = build_perp_fins(
            &patch_of(a[0].plain()),
            a[1].topo(),
            &rot3(a[2].plain()),
            &xyz(a[3].plain()),
            &opts_of(a[4].plain()),
            &ctx,
        );
        if r.count > 0 {
            built += 1;
        }
        Value::Obj(vec![
            (
                "tris".into(),
                Value::F64(
                    r.triangles
                        .iter()
                        .flat_map(|v| [v[0], v[1], v[2], if v[3] == 1.0 { 1.0 } else { 0.0 }])
                        .collect(),
                ),
            ),
            ("tines".into(), Value::Num(r.tines as f64)),
            ("count".into(), Value::Num(r.count as f64)),
            (
                "wedges".into(),
                Value::Arr(
                    r.wedges
                        .iter()
                        .map(|w| {
                            Value::Obj(vec![
                                ("triRange".into(), ints(&[w.tri_range[0], w.tri_range[1]])),
                                ("line".into(), pts_value(&w.line)),
                                ("height".into(), w.height.into()),
                                ("span".into(), w.span.into()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    });
    assert!(n > 500 && built > 300, "{n} cases, {built} built");
}

#[test]
fn prop_serves_patch_replay() {
    let mut yes = 0;
    let n = replay("propServesPatch", |_, a| {
        let cfg = cfg_of(a);
        let r = prop_serves_patch(&patch_of(a[0].plain()), &props_of(a[1].plain()), &cfg);
        if r {
            yes += 1;
        }
        Value::Bool(r)
    });
    assert!(n > 500 && yes > 50);
}

/// The `{ offset, regions: [{ faces }] }` the two read of the analyze result.
fn slim_analysis(v: &Value) -> Analysis {
    Analysis {
        over: vec![],
        kept: vec![],
        on_bed: vec![],
        regions: v
            .get("regions")
            .arr()
            .iter()
            .map(|g| Region {
                faces: nums(g.get("faces")).iter().map(|&f| f as usize).collect(),
                area: 0.0,
            })
            .collect(),
        raw_region_count: 0,
        over_area: 0.0,
        bed_area: 0.0,
        over_face_count: 0,
        offset: xyz(v.get("offset")),
        size: Xyz::default(),
    }
}

fn after_args(a: &[Resolved]) -> (Vec<[f64; 3]>, IndexSet<usize>, Config) {
    let tris: Vec<[f64; 3]> = nums(a[4].plain())
        .chunks(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let served: IndexSet<usize> = nums(a[3].plain()).iter().map(|&i| i as usize).collect();
    (tris, served, cfg_of(a))
}

#[test]
fn bare_after_wedges_replay() {
    let mut nonempty = 0;
    replay("bareAfterWedges", |_, a| {
        let (tris, served, cfg) = after_args(a);
        let v4: Vec<[f64; 4]> = tris.iter().map(|v| [v[0], v[1], v[2], 0.0]).collect();
        let r = bare_after_wedges(
            a[0].topo(),
            &rot3(a[1].plain()),
            &slim_analysis(a[2].plain()),
            &served,
            &v4,
            &cfg,
        );
        if !r.is_empty() {
            nonempty += 1;
        }
        ints(&r)
    });
    assert!(nonempty > 10);
}

#[test]
fn unserved_after_wedges_replay() {
    replay("unservedAfterWedges", |_, a| {
        let (tris, served, cfg) = after_args(a);
        let v4: Vec<[f64; 4]> = tris.iter().map(|v| [v[0], v[1], v[2], 0.0]).collect();
        let r = unserved_after_wedges(
            a[0].topo(),
            &rot3(a[1].plain()),
            &slim_analysis(a[2].plain()),
            &served,
            &v4,
            &cfg,
        );
        Value::Num(r as f64)
    });
}

#[test]
fn grip_patches_replay() {
    let n = replay("gripPatches", |_, a| {
        let g = grip_patches(a[0].topo(), a[1].analysis(), &rot3(a[2].plain()));
        Value::Obj(vec![
            (
                "patches".into(),
                Value::Arr(g.patches.iter().map(patch_val).collect()),
            ),
            (
                "faceMap".into(),
                Value::Arr(
                    g.face_map
                        .iter()
                        .map(|(&f, &i)| {
                            Value::Arr(vec![Value::Num(f as f64), Value::Num(i as f64)])
                        })
                        .collect(),
                ),
            ),
        ])
    });
    assert!(n > 50);
}

#[test]
fn wedge_veto_replay() {
    let mut vetoed = 0;
    let n = replay("wedgeVeto", |_, a| {
        let cfg = cfg_of(a);
        let patches: Vec<Patch> = a[0].plain().arr().iter().map(patch_of).collect();
        let sets: Vec<Vec<PropRecord>> = a[5].plain().arr().iter().map(props_of).collect();
        let veto = wedge_veto(
            patches,
            a[1].topo().clone(),
            rot3(a[2].plain()),
            xyz(a[3].plain()),
            opts_of(a[4].plain()),
            cfg.clone(),
            Crest::new(None, 0.0, &cfg),
        );
        let out: Vec<Value> = a[6]
            .plain()
            .arr()
            .iter()
            .map(|q| {
                let set = &sets[q.arr()[0].num() as usize];
                let rec = props_of(&Value::Arr(vec![q.arr()[1].clone()])).remove(0);
                let r = veto(&rec, set);
                if r {
                    vetoed += 1;
                }
                Value::Bool(r)
            })
            .collect();
        Value::Arr(out)
    });
    assert!(n > 10 && vetoed > 0, "{n} cases, {vetoed} vetoes");
}
