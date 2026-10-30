//! Replays `parity/corpus/inside/*` (recorded from web/inside.js by `parity/oracle/inside.js`):
//! `insidePart`, `nearestPart` and `solidClearance`, direct and captured from the 54 golden
//! scenes. The topology comes from the `{"$topo": id}` reference (`sf_testkit::parity::topo`),
//! so the replay also tests `build_topology`. Pure arithmetic on the inputs: `Tol::Exact`.

mod common;
use ::parity::{Case, Tol, Value, compare, load_cases};
use common::nums;
use sf_engine::inside::{inside_part, nearest_part, solid_clearance};
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};

fn rot(r: &Resolved) -> Rot3 {
    <[f64; 9]>::try_from(nums(r.plain()).as_slice()).expect("rot has 9 numbers")
}

fn xyz(r: &Resolved) -> Xyz {
    let v = r.plain();
    Xyz {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
    }
}

fn num(r: &Resolved) -> f64 {
    r.plain().num()
}

fn obj(fields: &[(&str, f64)]) -> Value {
    Value::Obj(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), (*v).into()))
            .collect(),
    )
}

fn replay(file: &str, min_cases: usize, f: impl Fn(&[Resolved]) -> Value) {
    let cases: Vec<Case> = load_cases("inside", file);
    assert!(cases.len() >= min_cases, "{file}: {} cases", cases.len());
    for c in &cases {
        let args = resolve_args(&c.args);
        let got = f(&args);
        let want = c.out.as_ref().unwrap();
        if let Err(m) = compare(&got, want, &Tol::Exact) {
            panic!(
                "{file}/{}: out{} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

fn inside_fn(a: &[Resolved]) -> Value {
    Value::Bool(inside_part(
        a[0].topo(),
        &rot(&a[1]),
        &xyz(&a[2]),
        num(&a[3]),
        num(&a[4]),
        num(&a[5]),
    ))
}

fn nearest_fn(a: &[Resolved]) -> Value {
    match nearest_part(
        a[0].topo(),
        &rot(&a[1]),
        &xyz(&a[2]),
        num(&a[3]),
        num(&a[4]),
        num(&a[5]),
        a.get(6).map_or(0.45, num), // JS default rMax = 0.45
    ) {
        Some(n) => obj(&[("d", n.d), ("cosUp", n.cos_up)]),
        None => Value::Null,
    }
}

fn clearance_fn(a: &[Resolved]) -> Value {
    let verts: Vec<[f64; 3]> = a[3]
        .plain()
        .arr()
        .iter()
        .map(|p| <[f64; 3]>::try_from(nums(p).as_slice()).unwrap())
        .collect();
    match solid_clearance(
        a[0].topo(),
        &rot(&a[1]),
        &xyz(&a[2]),
        &verts,
        a.get(4).map_or(0.45, num),
    ) {
        Some(c) => obj(&[
            ("d", c.d),
            ("cosUp", c.cos_up),
            ("x", c.x),
            ("y", c.y),
            ("z", c.z),
        ]),
        None => Value::Null,
    }
}

#[test]
fn inside_part_direct() {
    replay("insidePart", 5000, inside_fn);
}
#[test]
fn inside_part_captured() {
    replay("insidePart.captured", 100, inside_fn);
}
#[test]
fn nearest_part_direct() {
    replay("nearestPart", 10000, nearest_fn);
}
#[test]
fn nearest_part_captured() {
    replay("nearestPart.captured", 100, nearest_fn);
}
#[test]
fn solid_clearance_direct() {
    replay("solidClearance", 1000, clearance_fn);
}
#[test]
fn solid_clearance_captured() {
    replay("solidClearance.captured", 100, clearance_fn);
}
