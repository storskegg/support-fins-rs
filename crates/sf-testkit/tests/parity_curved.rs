//! Replays `parity/corpus/curved/` (recorded by `parity/oracle/curved.js`), bit for bit. The
//! trailing argument is the `$config` snapshot of CURVED.
use parity::{Tol, Value, compare, load_cases};
use sf_engine::prop::curved::{bends, line_bend};
use sf_testkit::parity_config::config;

fn line(v: &Value) -> Vec<[f64; 3]> {
    v.arr()
        .iter()
        .map(|p| {
            let a = p.arr();
            [a[0].num(), a[1].num(), a[2].num()]
        })
        .collect()
}

#[test]
fn line_bend_replays() {
    let cases = load_cases("curved", "line_bend");
    assert!(cases.len() > 100);
    for c in &cases {
        let cfg = config(c.args[1].get("$config"));
        let got = Value::Num(line_bend(&line(&c.args[0]), &cfg));
        if let Err(e) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "line_bend/{}: {} got {} expected {}",
                c.id, e.path, e.actual, e.expected
            );
        }
    }
}

#[test]
fn bends_replays() {
    let cases = load_cases("curved", "bends");
    assert!(cases.len() > 50);
    let mut yes = 0;
    for c in &cases {
        let cfg = config(c.args[1].get("$config"));
        // `q.line ?? []`
        let lines: Vec<Vec<[f64; 3]>> = c.args[0]
            .arr()
            .iter()
            .map(|w| match w {
                Value::Obj(o) => o
                    .iter()
                    .find(|(k, _)| k == "line")
                    .map(|(_, l)| line(l))
                    .unwrap_or_default(),
                _ => panic!("wall"),
            })
            .collect();
        let r = bends(lines.iter().map(|l| l.as_slice()), &cfg);
        yes += usize::from(r);
        if let Err(e) = compare(&Value::Bool(r), c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "bends/{}: {} got {} expected {}",
                c.id, e.path, e.actual, e.expected
            );
        }
    }
    assert!(
        yes > 5 && yes < cases.len() - 5,
        "{yes} of {} bend",
        cases.len()
    );
}
