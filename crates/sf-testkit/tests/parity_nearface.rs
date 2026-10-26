//! Replay of `parity/corpus/nearface/*` (recorded by `parity/oracle/nearface.js` from web/prop/nearface.js).
//! Exact: face indices, and distances that are arithmetic on the seated coordinates.
use ::parity::{Tol, Value, compare, load_cases};
use sf_engine::prop::nearface::{nearest_faces, pt_tri_dist2};
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::resolve_args;

fn xs(v: &Value) -> Vec<f64> {
    match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        o => panic!("numbers: {o:?}"),
    }
}
fn p3(v: &Value) -> [f64; 3] {
    xs(v).try_into().expect("3 numbers")
}

fn pt_tri_replay(file: &str) {
    let cases = load_cases("nearface", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let d = pt_tri_dist2(
            p3(&c.args[0]),
            p3(&c.args[1]),
            p3(&c.args[2]),
            p3(&c.args[3]),
        );
        compare(&Value::Num(d), c.out.as_ref().unwrap(), &Tol::Exact)
            .unwrap_or_else(|m| panic!("ptTriDist2 {}: {m:?}", c.id));
    }
}
fn nearest_replay(file: &str) {
    let cases = load_cases("nearface", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let rot: Rot3 = xs(a[1].plain()).try_into().expect("9 numbers");
        let o = a[2].plain();
        let off = Xyz {
            x: o.get("x").num(),
            y: o.get("y").num(),
            z: o.get("z").num(),
        };
        let n = |i: usize| a[i].plain().num();
        let faces = nearest_faces(a[0].topo(), &rot, &off, n(3), n(4), n(5));
        let actual = Value::from(faces.iter().map(|&f| f as f64).collect::<Vec<_>>());
        compare(&actual, c.out.as_ref().unwrap(), &Tol::Exact)
            .unwrap_or_else(|m| panic!("nearestFaces {}: {m:?}", c.id));
    }
}

#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn pt_tri_dist2_direct() {
    pt_tri_replay("ptTriDist2");
}
#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn pt_tri_dist2_captured() {
    pt_tri_replay("ptTriDist2.captured");
}
#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn nearest_faces_direct() {
    nearest_replay("nearestFaces");
}
#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn nearest_faces_captured() {
    nearest_replay("nearestFaces.captured");
}
