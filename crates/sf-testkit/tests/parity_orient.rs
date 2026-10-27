//! Replay of `parity/corpus/orient/` (recorded by `parity/oracle/orient.js` from web/orient.js):
//! `layer_verdict`, `load_alignment` and `suggest_strength_pose`. Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::orient::*;
use sf_engine::types::Xyz;
use sf_testkit::parity::resolve_args;

fn obj(kv: Vec<(&str, Value)>) -> Value {
    Value::Obj(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

#[test]
fn layer_verdict_replays() {
    let cases = load_cases("orient", "layerVerdict");
    assert!(cases.len() > 100);
    for c in &cases {
        let s = &c.args[0];
        let p = layer_verdict(&Xyz {
            x: s.get("x").num(),
            y: s.get("y").num(),
            z: s.get("z").num(),
        });
        check(
            "layerVerdict",
            c,
            &obj(vec![("posture", Value::from(p.as_str()))]),
            &Tol::Exact,
        );
    }
}

fn align_value(a: Option<LoadAlignment>) -> Value {
    match a {
        None => Value::Null,
        Some(a) => obj(vec![
            ("cross", Value::Num(a.cross)),
            ("quality", Value::from(a.quality.as_str())),
            ("text", Value::from(a.text)),
        ]),
    }
}

#[test]
fn load_alignment_replays() {
    let cases = load_cases("orient", "loadAlignment");
    let (mut none, mut some) = (0, 0);
    for c in &cases {
        let a = load_alignment(p3(&c.args[0]));
        if a.is_some() {
            some += 1
        } else {
            none += 1
        }
        check("loadAlignment", c, &align_value(a), &Tol::Exact);
    }
    assert!(none > 0 && some > 0);
}

#[test]
fn suggest_strength_pose_replays() {
    let cases = load_cases("orient", "suggestStrengthPose");
    assert!(cases.len() > 100);
    let (mut none, mut some) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let threshold = a[2].plain().get("threshold").num();
        let r = suggest_strength_pose(a[0].topo(), p3(a[1].plain()), threshold);
        let actual = match r {
            None => {
                none += 1;
                Value::Null
            }
            Some(p) => {
                some += 1;
                obj(vec![
                    ("rot", num_value(&p.rot)),
                    ("cross", Value::Num(p.cross)),
                    ("quality", Value::from(p.quality.as_str())),
                    ("text", Value::from(p.text)),
                    ("height", Value::Num(p.height)),
                    ("over", Value::Num(p.over)),
                    ("bedArea", Value::Num(p.bed_area)),
                ])
            }
        };
        check("suggestStrengthPose", c, &actual, &Tol::Exact);
    }
    assert!(none > 0 && some > 0);
}
