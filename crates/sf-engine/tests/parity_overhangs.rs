//! Replays `parity/corpus/overhangs/*` (recorded from web/overhangs.js by
//! `parity/oracle/overhangs.js`): `buildTopology` and `analyze`, direct and captured from the
//! 54 golden scenes. Every comparison is `Tol::Exact`: both functions are pure arithmetic on
//! their inputs. The recorded `_zr/_parent/_over/_kept/_onBed` scratch keys are ignored.
//! `analyze` cases name a model; its topology is rebuilt by the Rust `build_topology`
//! from the position array recorded in `buildTopology.jsonl`, so the replay tests both.

use std::collections::HashMap;

use parity::{Tol, Value, compare, load_cases};
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};

fn pos_of(v: &Value) -> Vec<f64> {
    match v {
        Value::F32(a) => a.iter().map(|&x| x as f64).collect(),
        other => panic!("position array is not a Float32Array: {other:?}"),
    }
}

fn f32_value(pos: &[f64]) -> Value {
    Value::F32(pos.iter().map(|&x| x as f32).collect())
}

fn check(case: &str, what: &str, actual: &Value, expected: &Value) {
    if let Err(m) = compare(actual, expected, &Tol::Exact) {
        panic!(
            "{case}: {what}{} differs: got {}, expected {}",
            m.path, m.actual, m.expected
        );
    }
}

fn topo_fields(t: &Topology) -> Vec<(&'static str, Value)> {
    vec![
        ("pos", f32_value(&t.pos)),
        ("nFaces", (t.n_faces as f64).into()),
        ("nrm", Value::F64(t.nrm.clone())),
        ("area", Value::F64(t.area.clone())),
        ("vid", Value::I32(t.vid.clone())),
        ("adjA", Value::I32(t.adj_a.clone())),
        ("adjB", Value::I32(t.adj_b.clone())),
        ("vertexCount", (t.vertex_count as f64).into()),
        ("edgeCount", (t.edge_count as f64).into()),
    ]
}

fn replay_build_topology(file: &str) -> usize {
    let cases = load_cases("overhangs", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let expected = c.out.as_ref().unwrap();
        let t = build_topology(pos_of(&c.args[0]));
        for (key, actual) in topo_fields(&t) {
            check(&c.id, key, &actual, expected.get(key));
        }
    }
    cases.len()
}

#[test]
fn build_topology_direct() {
    assert_eq!(replay_build_topology("buildTopology"), 29);
}

#[test]
fn build_topology_captured() {
    assert_eq!(replay_build_topology("buildTopology.captured"), 54);
}

fn region_values(a: &Analysis) -> Value {
    Value::Arr(
        a.regions
            .iter()
            .map(|r| {
                Value::Obj(vec![
                    (
                        "faces".to_string(),
                        Value::Arr(r.faces.iter().map(|&f| (f as f64).into()).collect()),
                    ),
                    ("area".to_string(), r.area.into()),
                ])
            })
            .collect(),
    )
}

fn xyz(x: f64, y: f64, z: f64) -> Value {
    Value::Obj(vec![
        ("x".to_string(), x.into()),
        ("y".to_string(), y.into()),
        ("z".to_string(), z.into()),
    ])
}

fn analysis_fields(a: &Analysis) -> Vec<(&'static str, Value)> {
    vec![
        ("over", Value::U8(a.over.clone())),
        ("kept", Value::U8(a.kept.clone())),
        ("onBed", Value::U8(a.on_bed.clone())),
        ("regions", region_values(a)),
        ("rawRegionCount", (a.raw_region_count as f64).into()),
        ("overArea", a.over_area.into()),
        ("bedArea", a.bed_area.into()),
        ("overFaceCount", (a.over_face_count as f64).into()),
        ("offset", xyz(a.offset.x, a.offset.y, a.offset.z)),
        ("size", xyz(a.size.x, a.size.y, a.size.z)),
    ]
}

/// `aliased`: the direct recorder kept each result's `over/kept/onBed` as the topo's shared
/// scratch arrays and encoded them only at the end, so every case of a model carries the flags
/// of that model's LAST analyze. (This is the aliasing `Analysis` documents.) Those three keys
/// are then checked against the last result computed for the model.
fn replay_analyze(file: &str, aliased: bool) -> usize {
    let mut topos: HashMap<String, Topology> = HashMap::new();
    for c in load_cases("overhangs", "buildTopology") {
        topos.insert(c.id.clone(), build_topology(pos_of(&c.args[0])));
    }
    let cases = load_cases("overhangs", file);
    assert!(!cases.is_empty());
    let mut last: HashMap<String, Analysis> = HashMap::new();
    for c in &cases {
        let model = c.args[0].str();
        let topo = topos
            .get(model)
            .unwrap_or_else(|| panic!("{}: no recorded model {model}", c.id));
        let rot: Vec<f64> = c.args[2].arr().iter().map(Value::num).collect();
        let rot: [f64; 9] = rot.try_into().expect("rot has 9 numbers");
        let result = analyze(topo, c.args[1].num(), &rot);
        let expected = c.out.as_ref().unwrap();
        for (key, actual) in analysis_fields(&result) {
            if aliased && matches!(key, "over" | "kept" | "onBed") {
                continue;
            }
            check(&c.id, key, &actual, expected.get(key));
        }
        last.insert(model.to_string(), result);
    }
    if aliased {
        for c in &cases {
            let result = &last[c.args[0].str()];
            let expected = c.out.as_ref().unwrap();
            for (key, actual) in analysis_fields(result) {
                if matches!(key, "over" | "kept" | "onBed") {
                    check(&c.id, key, &actual, expected.get(key));
                }
            }
        }
    }
    cases.len()
}

#[test]
fn analyze_direct() {
    assert_eq!(replay_analyze("analyze", true), 29 * 3 * 8);
}

#[test]
fn analyze_captured() {
    assert_eq!(replay_analyze("analyze.captured", false), 54);
}
