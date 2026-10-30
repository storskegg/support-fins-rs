//! Replays `parity/corpus/planes/*` (recorded from web/planes.js by `parity/oracle/planes.js`):
//! `findWallPatches` (plain, with `stats`, with `excludedFaces`), `faceAdjacency` and the patch
//! helpers, direct and captured. Pure arithmetic on the inputs: `Tol::Exact`. Topologies come
//! from the `{"$topo": id}` reference, so the replay also tests `build_topology`.

mod common;
use ::parity::{Case, Tol, Value, compare, load_cases};
use common::nums;
use common::xyz as xyz_of;
use sf_engine::planes::*;
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};

fn rot(r: &Resolved) -> Rot3 {
    <[f64; 9]>::try_from(nums(r.plain()).as_slice()).expect("rot has 9 numbers")
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

/// A `Patch` from a recorded patch object (or just its `{d, n, u, t}` frame).
fn patch_of(v: &Value) -> Patch {
    let full = matches!(v, Value::Obj(o) if o.iter().any(|(k, _)| k == "faces"));
    Patch {
        faces: if full {
            nums(v.get("faces")).iter().map(|&f| f as usize).collect()
        } else {
            vec![]
        },
        area: if full { v.get("area").num() } else { 0.0 },
        mid: if full {
            xy_of(v.get("mid"))
        } else {
            Xy::default()
        },
        n: xyz_of(v.get("n")),
        u: xy_of(v.get("u")),
        t: xyz_of(v.get("t")),
        h: if full { v.get("h").num() } else { 0.0 },
        d: v.get("d").num(),
        u0: if full { v.get("u0").num() } else { 0.0 },
        u1: if full { v.get("u1").num() } else { 0.0 },
        t0: if full { v.get("t0").num() } else { 0.0 },
        t1: if full { v.get("t1").num() } else { 0.0 },
        z0: if full { v.get("z0").num() } else { 0.0 },
        z1: if full { v.get("z1").num() } else { 0.0 },
        lean: if full { v.get("lean").num() } else { 0.0 },
        flatness: if full { v.get("flatness").num() } else { 0.0 },
        tris: if full { nums(v.get("tris")) } else { vec![] },
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

fn check(file: &str, c: &Case, got: &Value) {
    if let Err(m) = compare(got, c.out.as_ref().unwrap(), &Tol::Exact) {
        panic!(
            "{file}/{}: out{} differs: got {}, expected {}",
            c.id, m.path, m.actual, m.expected
        );
    }
}

fn replay(file: &str, min_cases: usize, f: impl Fn(&Case, &[Resolved]) -> Value) {
    let cases = load_cases("planes", file);
    assert!(cases.len() >= min_cases, "{file}: {} cases", cases.len());
    for c in &cases {
        let args = resolve_args(&c.args);
        check(file, c, &f(c, &args));
    }
}

fn find_patches(a: &[Resolved], stats: Option<&mut PatchStats>) -> Vec<Patch> {
    let excluded = a.get(4).and_then(|r| match r.plain() {
        Value::U8(b) => Some(b.as_slice()),
        _ => None,
    });
    find_wall_patches(
        a[0].topo(),
        &rot(&a[1]),
        &xyz_of(a[2].plain()),
        stats,
        excluded,
    )
}

fn patches_val(ps: &[Patch]) -> Value {
    Value::Arr(ps.iter().map(patch_val).collect())
}

#[test]
fn find_wall_patches_direct() {
    replay("findWallPatches", 100, |_, a| {
        patches_val(&find_patches(a, None))
    });
}

#[test]
fn find_wall_patches_captured() {
    // a captured call may carry a `stats` object (filled by the call) as the 4th argument
    replay("findWallPatches.captured", 50, |_, a| {
        let mut stats = PatchStats::default();
        let has_stats = matches!(a.get(3).map(|r| r.plain()), Some(Value::Obj(_)));
        patches_val(&find_patches(a, has_stats.then_some(&mut stats)))
    });
}

#[test]
fn find_wall_patches_with_excluded_faces() {
    replay("findWallPatches.excluded", 100, |_, a| {
        patches_val(&find_patches(a, None))
    });
}

#[test]
fn find_wall_patches_stats() {
    // the JS object only has the keys that were ever incremented, in first-increment order
    let cases = load_cases("planes", "findWallPatches.stats");
    assert!(cases.len() >= 100);
    for c in &cases {
        let a = resolve_args(&c.args);
        let mut stats = PatchStats::default();
        find_patches(&a, Some(&mut stats));
        let want = c.out.as_ref().unwrap();
        let ours = [
            ("grown", stats.grown),
            ("tooSmall", stats.too_small),
            ("notUpright", stats.not_upright),
            ("notFlat", stats.not_flat),
            ("tooShort", stats.too_short),
            ("tooNarrow", stats.too_narrow),
        ];
        let Value::Obj(fields) = want else {
            panic!("{}: stats is not an object", c.id)
        };
        for (k, v) in &ours {
            let recorded = fields.iter().find(|(fk, _)| fk == k).map(|(_, x)| x.num());
            assert_eq!(recorded.unwrap_or(0.0), *v as f64, "{}: stats.{k}", c.id);
        }
        for (k, _) in fields {
            assert!(
                ours.iter().any(|(ok, _)| ok == k),
                "{}: unknown key {k}",
                c.id
            );
        }
    }
}

fn adjacency(a: &[Resolved]) -> Value {
    let adj = face_adjacency(a[0].topo());
    Value::Obj(vec![
        ("start".into(), Value::I32(adj.start.clone())),
        ("nbr".into(), Value::I32(adj.nbr.clone())),
    ])
}

#[test]
fn face_adjacency_direct() {
    replay("faceAdjacency", 20, |_, a| adjacency(a));
}

#[test]
fn face_adjacency_captured() {
    replay("faceAdjacency.captured", 20, |_, a| adjacency(a));
}

fn probe_value(r: Option<f64>) -> Value {
    r.map_or(Value::Null, Value::from)
}

fn grid_pairs(v: &Value) -> Vec<Vec<f64>> {
    v.arr().iter().map(nums).collect()
}

#[test]
fn patch_probe_grid() {
    replay("patchProbe.grid", 100, |_, a| {
        let p = patch_of(a[0].plain());
        Value::Arr(
            grid_pairs(a[1].plain())
                .iter()
                .map(|q| probe_value(patch_probe(&p, q[0], q[1])))
                .collect(),
        )
    });
}

#[test]
fn patch_covers_grid() {
    replay("patchCovers.grid", 100, |_, a| {
        let p = patch_of(a[0].plain());
        Value::Arr(
            grid_pairs(a[1].plain())
                .iter()
                .map(|q| Value::Bool(patch_covers(&p, q[0], q[1])))
                .collect(),
        )
    });
}

#[test]
fn patch_probe_captured() {
    replay("patchProbe.captured", 20, |_, a| {
        let p = patch_of(a[0].plain());
        probe_value(patch_probe(&p, a[1].plain().num(), a[2].plain().num()))
    });
}

fn point_val(p: [f64; 3]) -> Value {
    Value::Arr(p.iter().map(|&x| x.into()).collect())
}

#[test]
fn patch_point_grid() {
    replay("patchPoint.grid", 100, |_, a| {
        let p = patch_of(a[0].plain());
        Value::Arr(
            grid_pairs(a[1].plain())
                .iter()
                .map(|q| point_val(patch_point(&p, q[0], q[1], q[2])))
                .collect(),
        )
    });
}

#[test]
fn patch_point_captured() {
    replay("patchPoint.captured", 20, |_, a| {
        let p = patch_of(a[0].plain());
        point_val(patch_point(
            &p,
            a[1].plain().num(),
            a[2].plain().num(),
            a[3].plain().num(),
        ))
    });
}

#[test]
fn t_at_z_grid() {
    replay("tAtZ.grid", 100, |_, a| {
        let p = patch_of(a[0].plain());
        Value::Arr(
            grid_pairs(a[1].plain())
                .iter()
                .map(|q| t_at_z(&p, q[0], q[1]).into())
                .collect(),
        )
    });
}

#[test]
fn z_at_grid() {
    replay("zAt.grid", 100, |_, a| {
        let p = patch_of(a[0].plain());
        Value::Arr(
            grid_pairs(a[1].plain())
                .iter()
                .map(|q| z_at(&p, q[0], q[1]).into())
                .collect(),
        )
    });
}
