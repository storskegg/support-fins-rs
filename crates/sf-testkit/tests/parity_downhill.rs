//! Replay of `parity/corpus/downhill/` (recorded by `parity/oracle/downhill.js` from web/prop/downhill.js).
//! Exact. A captured call records the arguments it passed; the JS defaults fill the rest.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::downhill::{DownhillOpts, downhill_lines};
use sf_engine::tris::Tris;
use sf_testkit::parity::{Resolved, resolve_args};

fn replay(file: &str) {
    let cases = load_cases("downhill", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a: Vec<Resolved> = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let js = a.len() - 1; // the trailing $config is not a JS argument
        let pts = pts3(a[0].plain());
        let tris = match a[1].plain() {
            Value::F64(v) => Tris::new(v.clone()),
            o => panic!("regionTris: {o:?}"),
        };
        let num = |i: usize, default: f64| {
            if js > i && !matches!(a[i].plain(), Value::Undefined) {
                a[i].plain().num()
            } else {
                default
            }
        };
        let step = num(2, cfg.prop.station_step);
        let pitch = num(3, cfg.prop.max_unsupported_span);
        let (mut h, mut reach, mut cover) = (None, None, None);
        if js > 4
            && let Value::Obj(o) = a[4].plain()
        {
            for (k, v) in o {
                match (k.as_str(), v) {
                    (_, Value::Undefined) => {}
                    ("h", v) => h = Some(v.num()),
                    ("reach", v) => reach = Some(v.num()),
                    ("cover", v) => cover = Some(pts3(v)),
                    (k, _) => panic!("unknown option {k}"),
                }
            }
        }
        let opts = DownhillOpts {
            h,
            cover: cover.as_deref(),
            reach,
        };
        let actual = match downhill_lines(&pts, &tris, step, pitch, &opts, &cfg) {
            Some(lines) => Value::Arr(lines.iter().map(|l| pts_value(l)).collect()),
            None => Value::Null,
        };
        check(file, c, &actual, &Tol::Exact);
    }
}

#[test]
fn downhill_lines_direct() {
    replay("downhillLines");
}
#[test]
fn downhill_lines_captured() {
    replay("downhillLines.captured");
}
