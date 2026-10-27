//! Replays `parity/corpus/pieces/*` (recorded from web/pieces.js by `parity/oracle/pieces.js`):
//! `floatingPieces` on the stress models, captured from the golden scenes, and on synthetic
//! multi-body parts. `Tol::Exact`.

use ::parity::{Tol, Value, compare, load_cases};
use sf_engine::overhangs::{Analysis, Region, analyze, build_topology};
use sf_engine::pieces::{FloatingPiece, floating_pieces};
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        other => panic!("expected numbers, got {other:?}"),
    }
}

fn u8s(v: &Value) -> Vec<u8> {
    match v {
        Value::U8(b) => b.clone(),
        other => panic!("expected a Uint8Array, got {other:?}"),
    }
}

fn xyz(v: &Value) -> Xyz {
    Xyz {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
    }
}

/// The captured `result` argument: `buildFins` passes `analyze`'s result with extra keys added,
/// so it is not a plain `$analysis` reference and is recorded in full.
fn analysis_of(r: &Resolved) -> Analysis {
    if let Resolved::Analysis(a) = r {
        return a.clone();
    }
    let v = r.plain();
    Analysis {
        over: u8s(v.get("over")),
        kept: u8s(v.get("kept")),
        on_bed: u8s(v.get("onBed")),
        regions: v
            .get("regions")
            .arr()
            .iter()
            .map(|g| Region {
                faces: nums(g.get("faces")).iter().map(|&f| f as usize).collect(),
                area: g.get("area").num(),
            })
            .collect(),
        raw_region_count: v.get("rawRegionCount").num() as usize,
        over_area: v.get("overArea").num(),
        bed_area: v.get("bedArea").num(),
        over_face_count: v.get("overFaceCount").num() as usize,
        offset: xyz(v.get("offset")),
        size: xyz(v.get("size")),
    }
}

fn pieces_val(ps: &[FloatingPiece]) -> Value {
    Value::Arr(
        ps.iter()
            .map(|p| {
                Value::Obj(vec![
                    ("faces".into(), (p.faces as f64).into()),
                    (
                        "lowest".into(),
                        Value::Arr(p.lowest.iter().map(|&x| x.into()).collect()),
                    ),
                    ("drop".into(), p.drop.into()),
                ])
            })
            .collect(),
    )
}

fn replay_refs(file: &str, min_cases: usize) {
    let cases = load_cases("pieces", file);
    assert!(cases.len() >= min_cases, "{file}: {} cases", cases.len());
    for c in &cases {
        let a = resolve_args(&c.args);
        let rot = <Rot3>::try_from(nums(a[2].plain()).as_slice()).expect("rot has 9 numbers");
        let got = floating_pieces(a[0].topo(), &analysis_of(&a[1]), &rot);
        if let Err(m) = compare(&pieces_val(&got), c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "{file}/{}: out{} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

#[test]
fn floating_pieces_direct() {
    replay_refs("floatingPieces", 100);
}

#[test]
fn floating_pieces_captured() {
    replay_refs("floatingPieces.captured", 50);
}

#[test]
fn floating_pieces_on_synthetic_parts() {
    let cases = load_cases("pieces", "floatingPieces.soup");
    assert!(cases.len() >= 30);
    for c in &cases {
        let pos: Vec<f64> = match &c.args[0] {
            Value::F32(p) => p.iter().map(|&x| x as f64).collect(),
            other => panic!("expected a Float32Array, got {other:?}"),
        };
        let rot = <Rot3>::try_from(nums(&c.args[2]).as_slice()).expect("rot has 9 numbers");
        let topo = build_topology(pos);
        let res = analyze(&topo, c.args[1].num(), &rot);
        let got = floating_pieces(&topo, &res, &rot);
        if let Err(m) = compare(&pieces_val(&got), c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "soup/{}: out{} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}
