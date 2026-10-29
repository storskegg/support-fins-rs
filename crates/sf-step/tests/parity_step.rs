//! Replay of `parity/corpus/step/{isStep,readStepFile,readStep}.jsonl` (recorded by `parity/oracle/step.js`):
//! the web's WASM OpenCascade against the native one, on the same bytes.
//!
//! Tolerance: EXACT. The native kernel is built from the same OpenCascade commit as the web's, with the same
//! maths library (tools/occt/wasm_math.h), so the tree, names, vertex coordinates (as f32), index buffers and
//! therefore the triangles come out bit for bit the same: 18 inputs, 0 differences. (With Apple's libm instead,
//! the doubles differ in the last bit for 4% of the vertices of a sphere and the mesher then picks other
//! diagonals among cocircular points on 10 of the 23 meshes; docs/PARITY.md.)

use parity::{Tol, Value, compare, load_cases};
use sf_step::{RawNode, RawResult, StepObject, is_step, read_raw, step_objects};

fn num(v: usize) -> Value {
    Value::from(v as f64)
}

fn node_value(nodes: &[RawNode], i: usize) -> Value {
    let n = &nodes[i];
    Value::Obj(vec![
        ("name".into(), Value::from(n.name.as_str())),
        (
            "meshes".into(),
            Value::Arr(n.meshes.iter().map(|&m| num(m)).collect()),
        ),
        (
            "children".into(),
            Value::Arr(n.children.iter().map(|&c| node_value(nodes, c)).collect()),
        ),
    ])
}

fn raw_value(r: Option<&RawResult>) -> Value {
    let Some(r) = r else {
        return Value::Obj(vec![("success".into(), Value::Bool(false))]);
    };
    Value::Obj(vec![
        ("success".into(), Value::Bool(true)),
        ("root".into(), node_value(&r.nodes, 0)),
        (
            "meshes".into(),
            Value::Arr(
                r.meshes
                    .iter()
                    .map(|m| {
                        Value::Obj(vec![
                            ("name".into(), Value::from(m.name.as_str())),
                            ("position".into(), Value::F32(m.positions.clone())),
                            ("index".into(), Value::U32(m.indices.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn objects_value(objects: &[StepObject]) -> Value {
    let arr3 = |a: [f64; 3]| Value::from(a.to_vec());
    Value::Obj(vec![(
        "objects".into(),
        Value::Arr(
            objects
                .iter()
                .map(|o| {
                    Value::Obj(vec![
                        ("name".into(), Value::from(o.name.as_str())),
                        (
                            "positions".into(),
                            Value::F32(o.positions.iter().map(|&v| v as f32).collect()),
                        ),
                        ("tris".into(), num(o.tris)),
                        ("meshes".into(), num(o.meshes)),
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

fn bytes_of(c: &parity::Case) -> &[u8] {
    let Value::U8(b) = &c.args[0] else {
        panic!("{}: arg", c.id)
    };
    b
}

fn fail(id: &str, m: parity::Mismatch) -> ! {
    panic!(
        "{id}: out{}: actual {} vs expected {}",
        m.path, m.actual, m.expected
    )
}

#[test]
fn the_kernel_is_in_this_build() {
    assert!(
        sf_step::available(),
        "OpenCascade 7.6.1 was not found when sf-step was built: run `just occt` (crates/sf-step/README.md)"
    );
}

#[test]
fn the_native_kernel_tessellates_as_the_wasm_one_does() {
    let cases = load_cases("step", "readStepFile");
    assert!(cases.len() >= 15, "{} cases", cases.len());
    for c in cases {
        let raw = read_raw(bytes_of(&c)).unwrap();
        if let Err(m) = compare(
            &raw_value(raw.as_ref()),
            c.out.as_ref().unwrap(),
            &Tol::Exact,
        ) {
            fail(&c.id, m);
        }
    }
}

#[test]
fn step_objects_match_the_oracle() {
    let cases = load_cases("step", "readStep");
    assert!(cases.len() >= 15);
    for c in cases {
        let r = read_raw(bytes_of(&c))
            .unwrap()
            .ok_or("the CAD kernel could not read this STEP file".to_string());
        let objects = r.and_then(|r| step_objects(Some(&r)));
        match (objects, &c.out, &c.throws) {
            (Ok(o), Some(expected), None) => {
                if let Err(m) = compare(&objects_value(&o), expected, &Tol::Exact) {
                    fail(&c.id, m);
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
fn is_step_matches_the_oracle() {
    let cases = load_cases("step", "isStep");
    assert!(cases.len() >= 15);
    for c in cases {
        let expected = c.out.as_ref().unwrap();
        if let Err(m) = compare(&Value::from(is_step(bytes_of(&c))), expected, &Tol::Exact) {
            fail(&c.id, m);
        }
    }
}
