//! Replay of `parity/corpus/testkit/*.jsonl` (recorded by `parity/oracle/testkit.js` from the
//! upstream helpers) through the Rust helpers, bit for bit (`Tol::Exact`).

use parity::{Case, Tol, Value, compare, load_cases};
use sf_testkit::*;

fn check(c: &Case, actual: Value) {
    let expected = c
        .out
        .as_ref()
        .unwrap_or_else(|| panic!("{}: recorded a throw", c.id));
    if let Err(m) = compare(&actual, expected, &Tol::Exact) {
        panic!(
            "{}: out{}: actual {} vs expected {}",
            c.id, m.path, m.actual, m.expected
        );
    }
}

fn nums(c: &Case) -> Vec<f64> {
    c.args.iter().map(|a| a.num()).collect()
}

fn f32v(v: &[f64]) -> Value {
    Value::F32(v.iter().map(|&x| x as f32).collect())
}

fn v3(v: &Value) -> [f64; 3] {
    let a = v.arr();
    [a[0].num(), a[1].num(), a[2].num()]
}

fn verts(v: &Value) -> Vec<[f64; 3]> {
    v.arr().iter().map(v3).collect()
}

fn vv(v: &[[f64; 3]]) -> Value {
    Value::Arr(v.iter().map(|p| Value::from(p.to_vec())).collect())
}

fn opt(o: &Value, key: &str) -> Option<f64> {
    let Value::Obj(kv) = o else {
        panic!("options must be an object")
    };
    kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.num())
}

fn cases(f: &str) -> Vec<Case> {
    let c = load_cases("testkit", f);
    assert!(!c.is_empty(), "{f}: empty corpus");
    c
}

#[test]
fn block_matches_js() {
    for c in cases("block") {
        let a = nums(&c);
        check(&c, f32v(&block(a[0], a[1], a[2], a[3], a[4], a[5])));
    }
}

#[test]
fn tilted_block_soup_matches_js() {
    for c in cases("tiltedBlockSoup") {
        let a = nums(&c);
        check(
            &c,
            f32v(&tilted_block_soup(a[0], a[1], a[2], a[3], a[4], a[5], a[6])),
        );
    }
}

#[test]
fn holed_plate_soup_matches_js() {
    for c in cases("holedPlateSoup") {
        let a = nums(&c);
        check(&c, f32v(&holed_plate_soup(a[0], a[1], a[2], a[3], a[4])));
    }
}

#[test]
fn circular_bore_matches_js() {
    for c in cases("circularBore") {
        let o = &c.args[0];
        let d = CircularBore::default();
        let opts = CircularBore {
            diameter: opt(o, "diameter").unwrap_or(d.diameter),
            length: opt(o, "length").unwrap_or(d.length),
            segments: opt(o, "segments").map_or(d.segments, |v| v as usize),
            half: opt(o, "half").unwrap_or(d.half),
            axial_segments: opt(o, "axialSegments").map_or(d.axial_segments, |v| v as usize),
        };
        check(&c, Value::F64(circular_bore(&opts)));
    }
}

#[test]
fn recessed_body_matches_js() {
    for c in cases("recessedBody") {
        let o = &c.args[0];
        let d = RecessedBody::default();
        let translate = match o {
            Value::Obj(kv) => kv
                .iter()
                .find(|(k, _)| k == "translate")
                .map(|(_, v)| v3(v)),
            _ => panic!("options must be an object"),
        };
        let opts = RecessedBody {
            segments: opt(o, "segments").map_or(d.segments, |v| v as usize),
            height: opt(o, "height").unwrap_or(d.height),
            opening: opt(o, "opening").unwrap_or(d.opening),
            bore: opt(o, "bore").unwrap_or(d.bore),
            depth: opt(o, "depth").unwrap_or(d.depth),
            outer: opt(o, "outer").unwrap_or(d.outer),
            yaw: opt(o, "yaw").unwrap_or(d.yaw),
            translate: translate.unwrap_or(d.translate),
        };
        check(&c, Value::F64(recessed_body(&opts)));
    }
}

#[test]
fn rot_x_and_rot_y_match_js_for_every_degree() {
    let (x, y) = (cases("rotX"), cases("rotY"));
    assert_eq!((x.len(), y.len()), (721, 721));
    for c in x {
        check(&c, Value::from(rot_x(c.args[0].num()).to_vec()));
    }
    for c in y {
        check(&c, Value::from(rot_y(c.args[0].num()).to_vec()));
    }
}

#[test]
fn pt_tri_dist2_matches_js() {
    let cs = cases("ptTriDist2");
    assert_eq!(cs.len(), 5000);
    for c in cs {
        let r = pt_tri_dist2(
            v3(&c.args[0]),
            v3(&c.args[1]),
            v3(&c.args[2]),
            v3(&c.args[3]),
        );
        check(&c, Value::from(r));
    }
}

#[test]
fn is_closed_matches_js() {
    for c in cases("isClosed") {
        check(&c, Value::from(is_closed(&verts(&c.args[0]))));
    }
}

#[test]
fn is_oriented_matches_js() {
    for c in cases("isOriented") {
        check(&c, Value::from(is_oriented(&verts(&c.args[0]))));
    }
}

#[test]
fn bbox_matches_js() {
    for c in cases("bbox") {
        let b = bbox(&verts(&c.args[0]));
        let actual = Value::Obj(vec![
            ("lo".into(), Value::from(b.lo.to_vec())),
            ("hi".into(), Value::from(b.hi.to_vec())),
        ]);
        check(&c, actual);
    }
}

#[test]
fn tine_boxes_match_js() {
    let boxv = |b: &TineBox| {
        vec![
            ("verts".to_string(), vv(&b.verts)),
            ("lo".to_string(), Value::from(b.lo.to_vec())),
            ("hi".to_string(), Value::from(b.hi.to_vec())),
        ]
    };
    for c in cases("tineBoxes") {
        let r = tine_boxes(&verts(&c.args[0]));
        let tines = Value::Arr(r.tines.iter().map(|b| Value::Obj(boxv(b))).collect());
        let steps = Value::Arr(
            r.steps
                .iter()
                .map(|s| {
                    let mut kv = boxv(&s.b);
                    kv.push(("tine".into(), Value::from(s.tine as f64)));
                    Value::Obj(kv)
                })
                .collect(),
        );
        check(
            &c,
            Value::Obj(vec![("tines".into(), tines), ("steps".into(), steps)]),
        );
    }
}
