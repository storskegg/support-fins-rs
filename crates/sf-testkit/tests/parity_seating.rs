//! Replay of `parity/corpus/seating/*` (recorded by `parity/oracle/seating.js` from web/fins/seating.js).
//! Exact: the functions are arithmetic on their inputs.
mod common;
use ::parity::{Tol, Value, compare, load_cases};
use common::xyz;
use sf_engine::fins::seating::{SeatingKind, bed_contact, seated_part_tris, seating_of};
use sf_engine::overhangs::Analysis;
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};
use sf_testkit::parity_config::config;

fn rot3(v: &Value) -> Rot3 {
    let xs: Vec<f64> = match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        o => panic!("rot: {o:?}"),
    };
    xs.try_into().expect("9 numbers")
}
#[test]
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
            offset: match v {
                Value::Obj(o) if o.iter().any(|(k, _)| k == "offset") => xyz(v.get("offset")),
                _ => Xyz::default(), // seatingOf's synthetic cases pass a bare {bedArea}
            },
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
fn bed_contact_direct() {
    bed_contact_replay("bedContact");
}
#[test]
fn bed_contact_captured() {
    bed_contact_replay("bedContact.captured");
}
#[test]
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

fn seating_of_replay(file: &str) {
    let cases = load_cases("seating", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let pts: Vec<[f64; 2]> = a[1]
            .plain()
            .arr()
            .iter()
            .map(|p| match p.arr() {
                [x, y] => [x.num(), y.num()],
                o => panic!("{o:?}"),
            })
            .collect();
        let cfg = config(a[2].config());
        let s = seating_of(&analysis_of(&a[0]), &pts, &cfg);
        let kind = match s.kind {
            SeatingKind::Face => "face",
            SeatingKind::Edge => "edge",
            SeatingKind::Point => "point",
        };
        let actual = Value::Obj(vec![
            ("kind".into(), Value::from(kind)),
            ("span".into(), Value::Num(s.span)),
            ("bedArea".into(), Value::Num(s.bed_area)),
        ]);
        compare(&actual, c.out.as_ref().unwrap(), &Tol::Exact)
            .unwrap_or_else(|m| panic!("seatingOf {}: {m:?}", c.id));
    }
}

#[test]
fn seating_of_direct() {
    seating_of_replay("seatingOf");
}
#[test]
fn seating_of_captured() {
    seating_of_replay("seatingOf.captured");
}
