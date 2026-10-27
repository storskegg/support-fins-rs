//! Replay of the `cut` corpus (recorded by `parity/oracle/cut.js` from web/cut.js running the vendored
//! Manifold 3.5.4 WASM): `cutNegatives` on the slicers' own files, cut.test.js's synthetic parts and
//! every stress model bored and notched. The cut mesh must come out position for position and
//! triangle for triangle (`Tol::Exact`): that is Manifold built from source with the WASM build's
//! arithmetic (build.rs) agreeing with it to the bit.
use std::sync::Arc;

use parity::{Tol, Value, compare, load_cases};
use sf_io::paintcode::Paint;
use sf_io::threemf::{PartColors, PartKind, PartSettings, ReadExtra};

fn f32v(v: &Value) -> Vec<f64> {
    match v {
        Value::F32(a) => a.iter().map(|&x| x as f64).collect(),
        o => panic!("not a Float32Array: {o:?}"),
    }
}

fn colors_in(v: &Value) -> Option<PartColors> {
    if matches!(v, Value::Null) {
        return None;
    }
    let col = |name: &str| v.get(name).arr().to_vec();
    let mut out = PartColors::default();
    for p in col("paint") {
        out.paint.push(match &p {
            Value::Undefined => None,
            o => Some(Arc::new(Paint {
                name: o.get("name").str().into(),
                v: o.get("v").str().into(),
            })),
        });
    }
    for r in col("rgb") {
        out.rgb.push(match &r {
            Value::Undefined => None,
            o => Some(o.str().into()),
        });
    }
    Some(out)
}

fn colors_out(c: Option<&PartColors>) -> Value {
    let Some(c) = c else { return Value::Null };
    let paint = c
        .paint
        .iter()
        .map(|p| match p {
            None => Value::Undefined,
            Some(p) => Value::Obj(vec![
                ("name".into(), Value::Str(p.name.clone())),
                ("v".into(), Value::Str(p.v.clone())),
            ]),
        })
        .collect();
    let rgb = c
        .rgb
        .iter()
        .map(|r| {
            r.as_ref()
                .map_or(Value::Undefined, |s| Value::Str(s.clone()))
        })
        .collect();
    Value::Obj(vec![
        ("paint".into(), Value::Arr(paint)),
        ("rgb".into(), Value::Arr(rgb)),
    ])
}

#[test]
fn cut_negatives_matches_js() {
    let cases = load_cases("cut", "cutNegatives");
    assert!(cases.len() > 40, "{} cases", cases.len());
    for c in cases {
        let positions = f32v(&c.args[0]);
        let colors = colors_in(&c.args[1]);
        let extras: Vec<ReadExtra> = c.args[2]
            .arr()
            .iter()
            .map(|e| ReadExtra {
                kind: match e.get("kind").str() {
                    "negative" => PartKind::Negative,
                    "modifier" => PartKind::Modifier,
                    k => panic!("{k}"),
                },
                name: e.get("name").str().into(),
                settings: PartSettings::default(),
                positions: f32v(e.get("positions")),
            })
            .collect();
        let r = sf_cut::cut_negatives(&positions, colors.as_ref(), &extras);
        let actual = Value::Obj(vec![
            (
                "positions".into(),
                Value::F32(r.positions.iter().map(|&x| x as f32).collect()),
            ),
            ("colors".into(), colors_out(r.colors.as_ref())),
            ("cut".into(), Value::Num(r.cut as f64)),
            ("uncut".into(), Value::Num(r.uncut as f64)),
            (
                "why".into(),
                r.why.map_or(Value::Null, |w| Value::Str(w.into())),
            ),
        ]);
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
}
