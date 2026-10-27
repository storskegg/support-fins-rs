//! Replays `parity/corpus/solids/` (recorded by `parity/oracle/solids.js`) through
//! `sf_engine::solids`, bit for bit.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::solids::{
    Radius, box_extrude, loft_extrude, ribbon, rounded_flange, rounded_polygon, stack_extrude,
};

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        other => other.arr().iter().map(Value::num).collect(),
    }
}

fn v3(v: &Value) -> [f64; 3] {
    let a = nums(v);
    [a[0], a[1], a[2]]
}

fn poly(v: &Value) -> Vec<[f64; 2]> {
    v.arr()
        .iter()
        .map(|p| {
            let a = nums(p);
            [a[0], a[1]]
        })
        .collect()
}

fn sections(v: &Value) -> Vec<Vec<[f64; 3]>> {
    v.arr()
        .iter()
        .map(|s| s.arr().iter().map(v3).collect())
        .collect()
}

/// `P(a, b, c) = o + a*u + b*v + c*w`, per component ((o + a*u) + b*v) + c*w.
fn frame(v: &Value) -> impl Fn(f64, f64, f64) -> [f64; 3] {
    let (o, u, vv, w) = (
        v3(v.get("o")),
        v3(v.get("u")),
        v3(v.get("v")),
        v3(v.get("w")),
    );
    move |a, b, c| std::array::from_fn(|k| o[k] + a * u[k] + b * vv[k] + c * w[k])
}

/// The vertices' xyz as the recorded Float64Array; every vertex these emitters push is untagged.
fn flat3(out: &[[f64; 4]]) -> Value {
    assert!(
        out.iter().all(|v| v[3] == 0.0),
        "an untagged emitter pushed a tagged vertex"
    );
    Value::F64(out.iter().flat_map(|v| [v[0], v[1], v[2]]).collect())
}

fn replay(function: &str, min: usize, f: impl Fn(&Case) -> Value) {
    let cases = load_cases("solids", function);
    assert!(cases.len() >= min, "{function}: {} cases", cases.len());
    for c in &cases {
        let want = c.out.as_ref().expect("out");
        if let Err(m) = compare(&f(c), want, &Tol::Exact) {
            panic!(
                "solids/{function}/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

#[test]
fn rounded_polygon_replays() {
    replay("rounded_polygon", 500, |c| {
        let radius = match &c.args[1] {
            Value::Num(r) => Radius::All(*r),
            other => {
                let per = nums(other);
                return Value::Arr(
                    rounded_polygon(&poly(&c.args[0]), Radius::Each(&per), c.args[2].num())
                        .into_iter()
                        .map(|p| Value::Arr(vec![p[0].into(), p[1].into()]))
                        .collect(),
                );
            }
        };
        Value::Arr(
            rounded_polygon(&poly(&c.args[0]), radius, c.args[2].num())
                .into_iter()
                .map(|p| Value::Arr(vec![p[0].into(), p[1].into()]))
                .collect(),
        )
    });
}

#[test]
fn rounded_flange_replays() {
    replay("rounded_flange", 60, |c| {
        let mut out = Vec::new();
        rounded_flange(&sections(&c.args[0]), &mut out);
        flat3(&out)
    });
}

#[test]
fn ribbon_replays() {
    replay("ribbon", 100, |c| {
        let mut out = Vec::new();
        ribbon(&sections(&c.args[0]), &mut out);
        flat3(&out)
    });
}

#[test]
fn box_extrude_replays() {
    replay("box_extrude", 200, |c| {
        let mut out = Vec::new();
        box_extrude(
            &poly(&c.args[0]),
            c.args[1].num(),
            c.args[2].num(),
            frame(&c.args[3]),
            &mut out,
        );
        flat3(&out)
    });
}

#[test]
fn loft_extrude_replays() {
    replay("loft_extrude", 40, |c| {
        let mut out = Vec::new();
        loft_extrude(
            &poly(&c.args[0]),
            &poly(&c.args[1]),
            c.args[2].num(),
            c.args[3].num(),
            frame(&c.args[4]),
            &mut out,
        );
        flat3(&out)
    });
}

#[test]
fn stack_extrude_replays() {
    replay("stack_extrude", 40, |c| {
        let mut out = Vec::new();
        let polys: Vec<Vec<[f64; 2]>> = c.args[0].arr().iter().map(poly).collect();
        stack_extrude(&polys, &nums(&c.args[1]), frame(&c.args[2]), &mut out);
        flat3(&out)
    });
}
