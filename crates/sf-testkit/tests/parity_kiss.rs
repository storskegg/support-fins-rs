//! Replays `parity/corpus/kiss/kissEnds*.jsonl` (recorded from web/kiss.js by
//! `parity/oracle/kiss.js`). `kissEnds` calls `insidePart` hundreds of times per case, so a
//! matching result tests that, the topology (`{"$topo": id}`) and the bisection in one.
//! `Tol::Exact`.

mod common;
use ::parity::{Tol, Value, compare, load_cases};
use common::nums;
use sf_engine::kiss::{KissOpts, kiss_ends};
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::{Resolved, resolve_args};

fn num(r: &Resolved) -> f64 {
    r.plain().num()
}

fn replay(file: &str, min_cases: usize) {
    let cases = load_cases("kiss", file);
    assert!(cases.len() >= min_cases, "{file}: {} cases", cases.len());
    for c in &cases {
        let a = resolve_args(&c.args);
        let rot = <Rot3>::try_from(nums(a[1].plain()).as_slice()).expect("rot has 9 numbers");
        let off = a[2].plain();
        let offset = Xyz {
            x: off.get("x").num(),
            y: off.get("y").num(),
            z: off.get("z").num(),
        };
        let o = a[9].plain();
        let has = |k: &str| matches!(o, Value::Obj(f) if f.iter().any(|(fk, _)| fk == k));
        let opts = KissOpts {
            half: o.get("half").num(),
            back: o.get("back").num(),
            reach: o.get("reach").num(),
            // JS default `contactOffset = -0.01` applies when the key is absent
            contact_offset: if has("contactOffset") {
                o.get("contactOffset").num()
            } else {
                -0.01
            },
        };
        let e = kiss_ends(
            a[0].topo(),
            &rot,
            &offset,
            num(&a[3]),
            num(&a[4]),
            num(&a[5]),
            num(&a[6]),
            num(&a[7]),
            num(&a[8]),
            &opts,
        );
        let pair = |p: [f64; 2]| Value::Arr(vec![p[0].into(), p[1].into()]);
        let got = Value::Obj(vec![
            ("bot".into(), pair(e.bot)),
            ("top".into(), pair(e.top)),
        ]);
        if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "{file}/{}: out{} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

#[test]
fn kiss_ends_direct() {
    replay("kissEnds", 1000);
}

#[test]
fn kiss_ends_captured() {
    replay("kissEnds.captured", 100);
}
