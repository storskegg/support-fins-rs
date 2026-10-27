//! Replay of `parity/corpus/mold/*` (recorded by `parity/oracle/mold.js` from web/prop/mold.js).
//! Exact: arithmetic on the recorded stations and the rebuilt seated part.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::mold::{FloorPt, floor_line, mold_line};
use sf_testkit::parity::resolve_args;

fn floor_value(f: &[FloorPt]) -> Value {
    Value::Arr(
        f.iter()
            .map(|p| {
                let mut v = vec![p.x, p.y, p.z];
                if let Some(s) = p.sides {
                    v.extend(s);
                }
                Value::from(v)
            })
            .collect(),
    )
}

fn floor_replay(file: &str) {
    let cases = load_cases("mold", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (top, tris, cfg) = (pts3(a[0].plain()), tris_of(&a[1]), cfg_of(&a));
        // a captured call records only the arguments it passed: the JS defaults fill the rest
        let js = a.len() - 1; // the trailing $config is not a JS argument
        let margin = if js > 2 && !matches!(a[2].plain(), Value::Undefined) {
            a[2].plain().num()
        } else {
            1.0
        };
        let mold = js > 3 && matches!(a[3].plain(), Value::Bool(true));
        let actual = floor_value(&floor_line(&top, &tris, margin, mold, &cfg));
        check(file, c, &actual, &Tol::Exact);
    }
}

fn mold_replay(file: &str) {
    let cases = load_cases("mold", file);
    for c in &cases {
        let a = resolve_args(&c.args);
        let (top, tris, cfg) = (pts3(a[0].plain()), tris_of(&a[1]), cfg_of(&a));
        let m = mold_line(&top, &tris, &cfg);
        let actual = Value::Obj(vec![
            ("top".into(), pts_value(&m.top)),
            ("floor".into(), floor_value(&m.floor)),
        ]);
        check(file, c, &actual, &Tol::Exact);
    }
}

#[test]
fn floor_line_direct() {
    floor_replay("floorLine");
}
#[test]
fn floor_line_captured() {
    floor_replay("floorLine.captured");
}
#[test]
fn mold_line_direct() {
    assert!(!load_cases("mold", "moldLine").is_empty());
    mold_replay("moldLine");
}
#[test]
fn mold_line_captured() {
    mold_replay("moldLine.captured");
}
