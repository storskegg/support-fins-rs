//! Replays `parity/corpus/drawarc/` (recorded by `parity/oracle/drawarc.js`), bit for bit.
use parity::{Tol, Value, compare, load_cases};
use sf_engine::drawarc::{arc_stations, arc_through};

fn p3(v: &Value) -> [f64; 3] {
    let a = v.arr();
    [a[0].num(), a[1].num(), a[2].num()]
}

#[test]
fn arc_through_replays() {
    let cases = load_cases("drawarc", "arc_through");
    assert!(cases.len() > 300);
    for c in &cases {
        let (a, b, m) = (p3(&c.args[0]), p3(&c.args[1]), p3(&c.args[2]));
        let got = match arc_through([a[0], a[1]], [b[0], b[1]], [m[0], m[1]]) {
            None => Value::Null,
            Some(r) => Value::Obj(vec![
                ("cx".into(), r.cx.into()),
                ("cy".into(), r.cy.into()),
                ("r".into(), r.r.into()),
                ("a0".into(), r.a0.into()),
                ("sweep".into(), r.sweep.into()),
                ("toM".into(), r.to_m.into()),
            ]),
        };
        if let Err(e) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "arc_through/{}: {} got {} expected {}",
                c.id, e.path, e.actual, e.expected
            );
        }
    }
}

#[test]
fn arc_stations_replays() {
    let cases = load_cases("drawarc", "arc_stations");
    assert!(cases.len() > 300);
    for c in &cases {
        let got = match arc_stations(
            p3(&c.args[0]),
            p3(&c.args[1]),
            p3(&c.args[2]),
            c.args[3].num(),
            c.args[4].num(),
        ) {
            None => Value::Null,
            Some(v) => Value::Arr(v.into_iter().map(|p| p.to_vec().into()).collect()),
        };
        if let Err(e) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "arc_stations/{}: {} got {} expected {}",
                c.id, e.path, e.actual, e.expected
            );
        }
    }
}
