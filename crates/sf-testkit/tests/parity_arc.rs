//! Replay of `parity/corpus/arc/` (recorded by `parity/oracle/arc.js` from web/prop/arc.js). Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::arc::arc_line;
use sf_engine::tris::Tris;
use sf_testkit::parity::resolve_args;

fn replay(file: &str) {
    let cases = load_cases("arc", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let pts = pts3(a[0].plain());
        let tris = match a[1].plain() {
            Value::F64(v) => Tris::new(v.clone()),
            o => panic!("regionTris: {o:?}"),
        };
        let cfg = cfg_of(&a);
        let js = a.len() - 1;
        let step = if js > 2 && !matches!(a[2].plain(), Value::Undefined) {
            a[2].plain().num()
        } else {
            cfg.prop.station_step
        };
        let actual = match arc_line(&pts, &tris, step, &cfg) {
            Some(lines) => Value::Arr(lines.iter().map(|l| pts_value(l)).collect()),
            None => Value::Null,
        };
        check(file, c, &actual, &Tol::Exact);
    }
}

#[test]
fn arc_line_direct() {
    replay("arcLine");
}
#[test]
fn arc_line_captured() {
    replay("arcLine.captured");
}
