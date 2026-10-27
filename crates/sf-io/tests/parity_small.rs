//! Replays of the `scale`, `exportname` and `paintcode` corpora (recorded by
//! `parity/oracle/{scale,exportname,paintcode}.js`), all `Tol::Exact`.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::jsnum;
use sf_io::exportname::{ExportOpts, export_name};
use sf_io::paintcode::{FilamentPainter, Paint, fill_unpainted, is_whole, whole_code};
use sf_io::scale::{scale_about, scale_from_percent};
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

/// JS `Number(v)` on a recorded value.
fn js_number(v: &Value) -> f64 {
    match v {
        Value::Num(n) => *n,
        Value::Str(s) => jsnum::string_to_number(s),
        Value::Bool(b) => f64::from(*b),
        Value::Null => 0.0,
        Value::Undefined => f64::NAN,
        other => panic!("not a primitive: {other:?}"),
    }
}

fn v3(v: &Value) -> [f64; 3] {
    let a = v.arr();
    [a[0].num(), a[1].num(), a[2].num()]
}

fn opt_str(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Null => None,
        o => panic!("{o:?}"),
    }
}

#[test]
fn scale_about_matches_js() {
    let cs = load_cases("scale", "scaleAbout");
    assert!(cs.len() > 100);
    for c in cs {
        let Value::F32(src) = &c.args[0] else {
            panic!("src")
        };
        let src: Vec<f64> = src.iter().map(|&v| v as f64).collect();
        let out = scale_about(&src, c.args[1].num(), v3(&c.args[2]), v3(&c.args[3]));
        check(&c, Value::F32(out.iter().map(|&v| v as f32).collect()));
    }
}

#[test]
fn scale_from_percent_matches_js() {
    let cs = load_cases("scale", "scaleFromPercent");
    assert!(cs.len() > 100);
    for c in cs {
        let r = scale_from_percent(js_number(&c.args[0]));
        check(&c, r.map_or(Value::Null, Value::Num));
    }
}

#[test]
fn export_name_matches_js() {
    let cs = load_cases("exportname", "exportName");
    assert!(cs.len() > 100);
    for c in cs {
        let field = |name: &str| match &c.args[2] {
            Value::Obj(kv) => kv
                .iter()
                .find(|(k, _)| k == name)
                .map_or(f64::NAN, |(_, v)| js_number(v)),
            o => panic!("{o:?}"),
        };
        let o = ExportOpts {
            nozzle: field("nozzle"),
            layer_height: field("layerHeight"),
        };
        check(
            &c,
            Value::from(export_name(c.args[0].str(), c.args[1].str(), &o)),
        );
    }
}

#[test]
fn whole_code_matches_js() {
    for c in load_cases("paintcode", "wholeCode") {
        check(
            &c,
            whole_code(c.args[0].num()).map_or(Value::Null, Value::from),
        );
    }
}

#[test]
fn is_whole_matches_js() {
    let cs = load_cases("paintcode", "isWhole");
    assert!(cs.len() > 1000);
    for c in cs {
        check(&c, Value::from(is_whole(c.args[0].str())));
    }
}

#[test]
fn fill_unpainted_matches_js() {
    let cs = load_cases("paintcode", "fillUnpainted");
    assert!(cs.len() > 10000);
    for c in cs {
        check(
            &c,
            fill_unpainted(c.args[0].str(), c.args[1].num()).map_or(Value::Null, Value::from),
        );
    }
}

#[test]
fn filament_painter_matches_js() {
    for c in load_cases("paintcode", "filamentPainter") {
        let mut painter = FilamentPainter::new(c.args[0].str());
        let mut outs = Vec::new();
        for call in c.args[1].arr() {
            let call = call.arr();
            let p = match &call[0] {
                Value::Null => None,
                o => Some(Arc::new(Paint {
                    name: o.get("name").str().into(),
                    v: o.get("v").str().into(),
                })),
            };
            let r = painter.paint(p.as_ref(), call[1].num());
            outs.push(match r {
                None => Value::Null,
                Some(p) => Value::Obj(vec![
                    ("name".into(), Value::from(p.name.as_str())),
                    ("v".into(), Value::from(p.v.as_str())),
                ]),
            });
        }
        let _ = opt_str;
        check(&c, Value::Arr(outs));
    }
}
