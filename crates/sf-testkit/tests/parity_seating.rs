//! Replay of `parity/corpus/seating/*` (recorded by `parity/oracle/seating.js` from web/fins/seating.js).
//! Exact: the functions are arithmetic on their inputs. `seatingOf` is replayed once `FinConfig` has
//! `pad_min_area` (track B).
use ::parity::{Tol, Value, compare, load_cases};
use sf_engine::fins::seating::{bed_contact, seated_part_tris};
use sf_engine::overhangs::Analysis;
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};

fn rot3(v: &Value) -> Rot3 {
    let xs: Vec<f64> = match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        o => panic!("rot: {o:?}"),
    };
    xs.try_into().expect("9 numbers")
}
fn xyz(v: &Value) -> Xyz {
    Xyz {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
    }
}

#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn seated_part_tris_direct() {
    let cases = load_cases("seating", "seatedPartTris");
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let t = seated_part_tris(a[0].topo(), &rot3(a[1].plain()), &xyz(a[2].plain()));
        compare(
            &Value::F64(t.v.clone()),
            c.out.as_ref().unwrap(),
            &Tol::Exact,
        )
        .unwrap_or_else(|m| panic!("seatedPartTris {}: {m:?}", c.id));
    }
}

/// bedContact reads only `result.offset` (and seatingOf only `result.bedArea`). buildFins hands it a
/// spread copy of the analyze result with extra fields, which the recorder cannot tie to a reference
/// and records in full: take the two numbers from it.
fn analysis_of(r: &Resolved) -> Analysis {
    match r {
        Resolved::Analysis(a) => a.clone(),
        Resolved::Plain(v) => Analysis {
            over: vec![],
            kept: vec![],
            on_bed: vec![],
            regions: vec![],
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: match v.get("bedArea") {
                Value::Num(n) => *n,
                other => panic!("bedArea: {other:?}"),
            },
            over_face_count: 0,
            offset: xyz(v.get("offset")),
            size: Xyz::default(),
        },
        other => panic!("analysis argument: {other:?}"),
    }
}

fn bed_contact_replay(file: &str) {
    let cases = load_cases("seating", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let bc = bed_contact(a[0].topo(), &analysis_of(&a[1]), &rot3(a[2].plain()));
        let actual = Value::Obj(vec![
            (
                "pts".into(),
                Value::Arr(bc.pts.iter().map(|p| Value::from(p.to_vec())).collect()),
            ),
            ("mx".into(), Value::Num(bc.mx)),
            ("my".into(), Value::Num(bc.my)),
        ]);
        compare(&actual, c.out.as_ref().unwrap(), &Tol::Exact)
            .unwrap_or_else(|m| panic!("bedContact {}: {m:?}", c.id));
    }
}

#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn bed_contact_direct() {
    bed_contact_replay("bedContact");
}
#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn bed_contact_captured() {
    bed_contact_replay("bedContact.captured");
}
#[test]
#[ignore = "needs M1 track A: build_topology, analyze, grid_dims (todo!() until #11 merges)"]
fn seated_part_tris_captured_rebuilds_the_same_soup() {
    // the captured args are references; the call's `out` is the soup JS built for them
    let cases = load_cases("seating", "seatedPartTris.captured");
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let t = seated_part_tris(a[0].topo(), &rot3(a[1].plain()), &xyz(a[2].plain()));
        compare(
            &Value::F64(t.v.clone()),
            c.out.as_ref().unwrap(),
            &Tol::Exact,
        )
        .unwrap_or_else(|m| panic!("seatedPartTris {}: {m:?}", c.id));
    }
}
