//! Replay of `parity/corpus/obj/{readOBJ,isObj}.jsonl` (recorded by `parity/oracle/obj.js`).

use parity::{Tol, Value, compare, load_cases};
use sf_io::obj::{ObjFile, is_obj, read_obj};

fn arr3(a: [f64; 3]) -> Value {
    Value::from(a.to_vec())
}

fn to_value(f: &ObjFile) -> Value {
    Value::Obj(vec![(
        "objects".into(),
        Value::Arr(
            f.objects
                .iter()
                .map(|o| {
                    Value::Obj(vec![
                        ("name".into(), Value::from(o.name.as_str())),
                        (
                            "positions".into(),
                            Value::F32(o.positions.iter().map(|&v| v as f32).collect()),
                        ),
                        ("tris".into(), Value::from(o.tris as f64)),
                        ("meshes".into(), Value::from(o.meshes as f64)),
                        (
                            "bbox".into(),
                            Value::Obj(vec![
                                ("lo".into(), arr3(o.bbox.lo)),
                                ("hi".into(), arr3(o.bbox.hi)),
                                ("size".into(), arr3(o.bbox.size)),
                            ]),
                        ),
                    ])
                })
                .collect(),
        ),
    )])
}

#[test]
fn read_obj_matches_the_oracle() {
    let cases = load_cases("obj", "readOBJ");
    assert!(cases.len() > 50);
    for c in cases {
        let Value::U8(bytes) = &c.args[0] else {
            panic!("{}: arg", c.id)
        };
        match (read_obj(bytes), &c.out, &c.throws) {
            (Ok(f), Some(expected), None) => {
                if let Err(m) = compare(&to_value(&f), expected, &Tol::Exact) {
                    panic!(
                        "{}: out{}: actual {} vs expected {}",
                        c.id, m.path, m.actual, m.expected
                    );
                }
            }
            (Err(e), None, Some(msg)) => assert_eq!(&e, msg, "{}", c.id),
            (r, out, throws) => panic!(
                "{}: Rust {:?}, JS out={} throws={:?}",
                c.id,
                r.map(|_| ()),
                out.is_some(),
                throws
            ),
        }
    }
}

#[test]
fn is_obj_matches_the_oracle() {
    let cases = load_cases("obj", "isObj");
    assert!(cases.len() > 80);
    for c in cases {
        let Value::U8(bytes) = &c.args[0] else {
            panic!("{}: arg", c.id)
        };
        let expected = c.out.as_ref().unwrap();
        if let Err(m) = compare(&Value::from(is_obj(bytes)), expected, &Tol::Exact) {
            panic!("{}: actual {} vs expected {}", c.id, m.actual, m.expected);
        }
    }
}
