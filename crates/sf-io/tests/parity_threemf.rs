//! Replay of `parity/corpus/threemf/*.jsonl` (recorded by `parity/oracle/threemf.js`): direct
//! cases, and the calls the upstream tests `tests/threemf.test.js` and `tests/session.test.js`
//! really make (`*.captured.jsonl`). `Tol::Exact`; the writers byte for byte.

use indexmap::IndexMap;
use parity::{Case, Tol, Value, compare, load_cases};
use sf_io::json::Json;
use sf_io::paintcode::Paint;
use sf_io::threemf::*;
use std::sync::Arc;

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

fn tris(v: &Value) -> Vec<[f64; 3]> {
    match v {
        Value::Arr(a) => a
            .iter()
            .map(|p| {
                let p = p.arr();
                [p[0].num(), p[1].num(), p[2].num()]
            })
            .collect(),
        _ => Vec::new(), // undefined
    }
}

fn field<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(kv) => kv
            .iter()
            .find(|(kk, _)| kk == k)
            .map(|(_, v)| v)
            .filter(|v| !matches!(v, Value::Undefined | Value::Null)),
        _ => None,
    }
}

fn json_of(v: &Value) -> Json {
    match v {
        Value::Num(n) => Json::Num(*n),
        Value::Str(s) => Json::Str(s.clone()),
        Value::Bool(b) => Json::Bool(*b),
        Value::Null => Json::Null,
        Value::Arr(a) => Json::Arr(a.iter().map(json_of).collect()),
        Value::Obj(kv) => Json::Obj(kv.iter().map(|(k, v)| (k.clone(), json_of(v))).collect()),
        o => panic!("not JSON: {o:?}"),
    }
}

fn value_of(j: &Json) -> Value {
    match j {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Num(n) => Value::Num(*n),
        Json::Str(s) => Value::Str(s.clone()),
        Json::Arr(a) => Value::Arr(a.iter().map(value_of).collect()),
        Json::Obj(kv) => Value::Obj(kv.iter().map(|(k, v)| (k.clone(), value_of(v))).collect()),
    }
}

fn colors_of(v: &Value) -> PartColors {
    let paint = field(v, "paint")
        .map(|p| {
            p.arr()
                .iter()
                .map(|e| match e {
                    Value::Undefined => None,
                    o => Some(Arc::new(Paint {
                        name: o.get("name").str().into(),
                        v: o.get("v").str().into(),
                    })),
                })
                .collect()
        })
        .unwrap_or_default();
    let rgb = field(v, "rgb")
        .map(|p| {
            p.arr()
                .iter()
                .map(|e| match e {
                    Value::Undefined => None,
                    o => Some(o.str().to_string()),
                })
                .collect()
        })
        .unwrap_or_default();
    PartColors { paint, rgb }
}

fn pairs_of(v: &Value) -> Vec<(String, String)> {
    v.arr()
        .iter()
        .map(|p| (p.arr()[0].str().to_string(), p.arr()[1].str().to_string()))
        .collect()
}

fn extras_of(v: Option<&Value>) -> Vec<ExtraPart> {
    let Some(v) = v else { return Vec::new() };
    v.arr()
        .iter()
        .map(|e| {
            let s = field(e, "settings");
            ExtraPart {
                kind: match e.get("kind").str() {
                    "negative" => PartKind::Negative,
                    "modifier" => PartKind::Modifier,
                    "blocker" => PartKind::Blocker,
                    "enforcer" => PartKind::Enforcer,
                    k => panic!("kind {k}"),
                },
                name: e.get("name").str().to_string(),
                settings: PartSettings {
                    bambu: s.and_then(|s| field(s, "bambu")).map(pairs_of),
                    prusa3: s.and_then(|s| field(s, "prusa3")).map(json_of),
                    prusa2: s.and_then(|s| field(s, "prusa2")).map(pairs_of),
                },
                tris: tris(e.get("tris")),
            }
        })
        .collect()
}

fn bool_of(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        _ => false,
    }
}

fn str_or(v: Option<&Value>, d: &str) -> String {
    v.map_or(d.to_string(), |v| v.str().to_string())
}

fn opts_of(v: Option<&Value>) -> ThreeMfOpts {
    let Some(o) = v else {
        return ThreeMfOpts::default();
    };
    ThreeMfOpts {
        separate: bool_of(field(o, "separate")),
        session: field(o, "session").map(json_of),
        iface: field(o, "iface").map(tris).unwrap_or_default(),
        colors: field(o, "colors").map(colors_of),
        extras: extras_of(field(o, "extras")),
    }
}

fn model_of(m: &Value) -> Model {
    Model {
        part_tris: tris(m.get("partTris")),
        fin_tris: field(m, "finTris").map(tris).unwrap_or_default(),
        // an absent name is `String(undefined)` in the objects' names
        name: str_or(field(m, "name"), "undefined"),
        iface: field(m, "iface").map(tris).unwrap_or_default(),
        colors: field(m, "colors").map(colors_of),
        extras: extras_of(field(m, "extras")),
    }
}

fn run_write_three_mf(c: &Case) -> Value {
    let o = opts_of(c.args.get(3));
    let name = c
        .args
        .get(2)
        .filter(|v| !matches!(v, Value::Undefined))
        .map_or("Support Fins".to_string(), |v| v.str().to_string());
    Value::U8(write_three_mf(
        &tris(&c.args[0]),
        &tris(&c.args[1]),
        &name,
        &o,
    ))
}

fn run_write_plate(c: &Case) -> Value {
    let models: Vec<Model> = c.args[0].arr().iter().map(model_of).collect();
    let o = c.args.get(1);
    let title = match o.and_then(|o| field(o, "title")) {
        Some(t) => t.str().to_string(),
        None => models
            .first()
            .map_or("Support Fins".to_string(), |m| m.name.clone()),
    };
    // the first model's name is undefined for some cases: `models[0]?.name ?? 'Support Fins'`
    let title = if c.args[0]
        .arr()
        .first()
        .is_some_and(|m| field(m, "name").is_none())
        && o.and_then(|o| field(o, "title")).is_none()
    {
        "Support Fins".to_string()
    } else {
        title
    };
    let bed = o.and_then(|o| field(o, "bed")).map(|b| Bed {
        x: b.get("x").num(),
        y: b.get("y").num(),
    });
    let session = o.and_then(|o| field(o, "session")).map(json_of);
    Value::U8(write_plate_3mf(
        &models,
        &title,
        o.is_some_and(|o| bool_of(field(o, "separate"))),
        session.as_ref(),
        bed,
    ))
}

fn colors_value(c: &PartColors) -> Value {
    let paint = c
        .paint
        .iter()
        .map(|p| match p {
            None => Value::Undefined,
            Some(p) => Value::Obj(vec![
                ("name".into(), Value::from(p.name.as_str())),
                ("v".into(), Value::from(p.v.as_str())),
            ]),
        })
        .collect();
    let rgb = c
        .rgb
        .iter()
        .map(|c| {
            c.as_ref()
                .map_or(Value::Undefined, |c| Value::from(c.as_str()))
        })
        .collect();
    Value::Obj(vec![
        ("paint".into(), Value::Arr(paint)),
        ("rgb".into(), Value::Arr(rgb)),
    ])
}

fn arr3(a: [f64; 3]) -> Value {
    Value::from(a.to_vec())
}

fn settings_value(s: &PartSettings) -> Value {
    let pairs = |p: &Vec<(String, String)>| {
        Value::Arr(
            p.iter()
                .map(|(k, v)| Value::Arr(vec![Value::from(k.as_str()), Value::from(v.as_str())]))
                .collect(),
        )
    };
    let mut kv = Vec::new();
    if let Some(b) = &s.bambu {
        kv.push(("bambu".to_string(), pairs(b)));
    }
    if let Some(p) = &s.prusa3 {
        kv.push(("prusa3".to_string(), value_of(p)));
    }
    if let Some(p) = &s.prusa2 {
        kv.push(("prusa2".to_string(), pairs(p)));
    }
    Value::Obj(kv)
}

fn positions_value(p: &[f64], double: bool) -> Value {
    if double {
        Value::F64(p.to_vec())
    } else {
        Value::F32(p.iter().map(|&v| v as f32).collect())
    }
}

fn attrs_value(a: &Attrs) -> Value {
    Value::Obj(
        a.iter()
            .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
            .collect(),
    )
}

fn node_value(n: &XmlNode) -> Value {
    Value::Obj(vec![
        ("name".into(), Value::from(n.name.as_str())),
        ("attributes".into(), attrs_value(&n.attributes)),
        (
            "children".into(),
            Value::Arr(n.children.iter().map(node_value).collect()),
        ),
    ])
}

fn pairs_value(p: &[(String, String)]) -> Value {
    Value::Arr(
        p.iter()
            .map(|(k, v)| Value::Arr(vec![Value::from(k.as_str()), Value::from(v.as_str())]))
            .collect(),
    )
}

fn source_value(s: &SourceMesh) -> Value {
    Value::Obj(vec![
        ("attributes".into(), attrs_value(&s.attributes)),
        ("vertices".into(), Value::from(s.vertices.clone())),
        ("indices".into(), Value::from(s.indices.clone())),
        (
            "triangleAttributes".into(),
            Value::Arr(
                s.triangle_attributes
                    .iter()
                    .map(|a| a.as_ref().map_or(Value::Undefined, attrs_value))
                    .collect(),
            ),
        ),
        (
            "properties".into(),
            Value::Arr(s.properties.iter().map(node_value).collect()),
        ),
        (
            "modelAttributes".into(),
            s.model_attributes.as_ref().map_or(Value::Null, attrs_value),
        ),
    ])
}

fn component_value(c: &ComponentInfo, double: bool) -> Value {
    Value::Obj(vec![
        ("path".into(), Value::from(c.path.as_str())),
        ("objectId".into(), Value::from(c.object_id.as_str())),
        (
            "partId".into(),
            c.part_id.as_deref().map_or(Value::Null, Value::from),
        ),
        (
            "route".into(),
            Value::Arr(c.route.iter().map(|&i| Value::from(i as f64)).collect()),
        ),
        ("kind".into(), Value::from(c.kind)),
        (
            "transform".into(),
            c.transform.map_or(Value::Null, |m| Value::from(m.to_vec())),
        ),
        (
            "cornerOrder".into(),
            Value::from(c.corner_order.iter().map(|&k| k as f64).collect::<Vec<_>>()),
        ),
        (
            "source".into(),
            c.source.as_deref().map_or(Value::Undefined, source_value),
        ),
        ("firstTriangle".into(), Value::from(c.first_triangle as f64)),
        ("buildIndex".into(), Value::from(c.build_index as f64)),
        ("unitScale".into(), Value::from(c.unit_scale)),
        ("sourceFaces".into(), Value::U32(c.source_faces.clone())),
        ("positions".into(), positions_value(&c.positions, double)),
        (
            "settings".into(),
            Value::Arr(
                c.settings
                    .iter()
                    .map(|p| {
                        Value::Obj(vec![
                            ("attributes".into(), attrs_value(&p.attributes)),
                            ("metadata".into(), pairs_value(&p.metadata)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("objectSettings".into(), pairs_value(&c.object_settings)),
    ])
}

fn result_value(r: &ThreeMf, double: bool, strict: bool) -> Value {
    let objects = r
        .objects
        .iter()
        .map(|o| {
            let mut fields = vec![
                ("name".into(), Value::from(o.name.as_str())),
                ("positions".into(), positions_value(&o.positions, double)),
                ("tris".into(), Value::from(o.tris as f64)),
                ("meshes".into(), Value::from(o.meshes as f64)),
                ("skipped".into(), Value::from(o.skipped as f64)),
                ("dropped".into(), Value::from(o.dropped as f64)),
                (
                    "bbox".into(),
                    Value::Obj(vec![
                        ("lo".into(), arr3(o.bbox.lo)),
                        ("hi".into(), arr3(o.bbox.hi)),
                        ("size".into(), arr3(o.bbox.size)),
                    ]),
                ),
                (
                    "colors".into(),
                    o.colors.as_ref().map_or(Value::Null, colors_value),
                ),
                (
                    "extras".into(),
                    Value::Arr(
                        o.extras
                            .iter()
                            .map(|e| {
                                Value::Obj(vec![
                                    ("kind".into(), Value::from(e.kind.as_str())),
                                    ("name".into(), Value::from(e.name.as_str())),
                                    ("settings".into(), settings_value(&e.settings)),
                                    ("positions".into(), positions_value(&e.positions, double)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ];
            if let Some(cs) = &o.components {
                fields.push((
                    "components".into(),
                    Value::Arr(cs.iter().map(|c| component_value(c, double)).collect()),
                ));
            }
            Value::Obj(fields)
        })
        .collect();
    Value::Obj(vec![
        ("unit".into(), Value::from(r.unit.as_str())),
        ("objects".into(), Value::Arr(objects)),
        (
            "positions".into(),
            positions_value(&r.positions, double && strict),
        ),
        ("meshes".into(), Value::from(r.meshes as f64)),
        ("items".into(), Value::from(r.items as f64)),
        ("skipped".into(), Value::from(r.skipped as f64)),
        ("dropped".into(), Value::from(r.dropped as f64)),
        (
            "session".into(),
            r.session.as_ref().map_or(Value::Null, value_of),
        ),
    ])
}

fn expect_throw_or(c: &Case, got: Result<Value, String>) {
    match (got, &c.out, &c.throws) {
        (Ok(v), Some(_), None) => check(c, v),
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

fn run_read(c: &Case) -> Result<Value, String> {
    let Value::U8(b) = &c.args[0] else {
        panic!("{}", c.id)
    };
    read_three_mf(b).map(|r| result_value(&r, false, false))
}

fn run_read_parts(c: &Case) -> Result<Value, String> {
    let Value::Map(m) = &c.args[0] else {
        panic!("{}", c.id)
    };
    let parts: IndexMap<String, Vec<u8>> = m
        .iter()
        .map(|(k, v)| match (k, v) {
            (Value::Str(k), Value::U8(v)) => (k.clone(), v.clone()),
            o => panic!("{o:?}"),
        })
        .collect();
    let o = c.args.get(1).filter(|v| !matches!(v, Value::Undefined));
    let opts = ReadOpts {
        build_index: o
            .and_then(|o| field(o, "buildIndex"))
            .map(|v| v.num() as usize),
        double_precision: o.is_some_and(|o| bool_of(field(o, "doublePrecision"))),
        outward_winding: o.is_some_and(|o| bool_of(field(o, "outwardWinding"))),
        with_components: o.is_some_and(|o| bool_of(field(o, "withComponents"))),
    };
    read_three_mf_parts(&parts, &opts)
        .map(|r| result_value(&r, opts.double_precision, opts.build_index.is_some()))
}

fn run_merge(c: &Case) -> Value {
    let owned: Vec<(Option<PartColors>, usize)> = c.args[0]
        .arr()
        .iter()
        .map(|o| {
            (
                field(o, "colors").map(colors_of),
                match o.get("positions") {
                    Value::F32(p) => p.len(),
                    p => panic!("{p:?}"),
                },
            )
        })
        .collect();
    let items: Vec<MergeItem> = owned
        .iter()
        .map(|(c, n)| MergeItem {
            colors: c.as_ref(),
            positions_len: *n,
        })
        .collect();
    merge_colors(&items).map_or(Value::Null, |c| colors_value(&c))
}

fn all(f: &str) -> Vec<Case> {
    let mut v = load_cases("threemf", f);
    v.extend(load_cases("threemf", &format!("{f}.captured")));
    assert!(!v.is_empty(), "{f}");
    v
}

#[test]
fn scan_xml_matches_js() {
    for c in load_cases("threemf", "scanXML") {
        let mut events: Vec<Value> = Vec::new();
        scan_xml(c.args[0].str(), &mut |ev| {
            events.push(match ev {
                XmlEvent::Open {
                    name,
                    attrs,
                    self_closing,
                } => Value::Arr(vec![
                    Value::from("open"),
                    Value::from(name),
                    Value::Obj(
                        attrs
                            .iter()
                            .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
                            .collect(),
                    ),
                    Value::from(self_closing),
                ]),
                XmlEvent::Close(name) => Value::Arr(vec![Value::from("close"), Value::from(name)]),
            });
            Ok(())
        })
        .unwrap();
        check(&c, Value::Arr(events));
    }
}

#[test]
fn mesh_xml_matches_js() {
    for c in load_cases("threemf", "meshXML") {
        check(&c, Value::from(mesh_xml(&tris(&c.args[0]), None, None)));
    }
}

#[test]
fn merge_colors_matches_js() {
    for c in all("mergeColors") {
        let mut c = c;
        // JS stores a fractional index (`paint[base + k]` with a non-integer `base`) as an own
        // property of the array, not an element: the recording now shows it as `$props`. The
        // port has no such property (nothing reads it: every consumer indexes the array), so the
        // comparison is of the elements.
        if let Some(Value::Obj(kv)) = &mut c.out {
            for (_, v) in kv.iter_mut() {
                if let Value::ArrProps(items, _) = v {
                    *v = Value::Arr(std::mem::take(items));
                }
            }
        }
        check(&c, run_merge(&c));
    }
}

#[test]
fn write_three_mf_matches_js_byte_for_byte() {
    for c in all("writeThreeMF") {
        check(&c, run_write_three_mf(&c));
    }
}

#[test]
fn write_plate_3mf_matches_js_byte_for_byte() {
    for c in all("writePlate3MF") {
        check(&c, run_write_plate(&c));
    }
}

#[test]
fn read_three_mf_matches_js() {
    for c in all("readThreeMF") {
        expect_throw_or(&c, run_read(&c));
    }
}

#[test]
fn read_three_mf_parts_matches_js() {
    for c in all("readThreeMFParts") {
        expect_throw_or(&c, run_read_parts(&c));
    }
}
