//! Replays `parity/corpus/brace/` and `parity/corpus/ties/` (recorded by `parity/oracle/brace.js`)
//! with `Tol::Exact`.

use std::cell::Cell;

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::prop::brace::{BraceKinds, brace_cross_rib, brace_wall};
use sf_engine::prop::ties::tie_walls;
use sf_engine::tris::Tris;
use sf_testkit::parity_config::config;

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        other => other.arr().iter().map(Value::num).collect(),
    }
}

fn p3(v: &Value) -> Vec<[f64; 3]> {
    v.arr()
        .iter()
        .map(|p| {
            let a = nums(p);
            [a[0], a[1], a[2]]
        })
        .collect()
}

fn opt_get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, x)| x),
        _ => None,
    }
}

fn p2(v: &Value) -> [f64; 2] {
    let a = nums(v);
    [a[0], a[1]]
}

/// `clear` as a spec: `{mode: 'always' | 'never' | 'maxX' | 'rejectFirst'}`.
fn clear_fn(spec: &Value) -> impl Fn(&[[f64; 3]]) -> bool {
    let mode = spec.get("mode").str().to_string();
    let x = match spec {
        Value::Obj(o) if o.iter().any(|(k, _)| k == "x") => spec.get("x").num(),
        _ => 0.0,
    };
    let n = match spec {
        Value::Obj(o) if o.iter().any(|(k, _)| k == "n") => spec.get("n").num(),
        _ => 0.0,
    };
    let calls = Cell::new(0.0);
    move |r: &[[f64; 3]]| match mode.as_str() {
        "always" => true,
        "never" => false,
        "maxX" => r.iter().all(|v| v[0] <= x),
        "rejectFirst" => {
            calls.set(calls.get() + 1.0);
            calls.get() > n
        }
        other => panic!("clear mode {other}"),
    }
}

fn check(function: &str, c: &Case, got: Value) {
    if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
        panic!(
            "{function}/{}: {} differs: got {}, expected {}",
            c.id, m.path, m.actual, m.expected
        );
    }
}

fn flat(out: &[[f64; 3]]) -> Value {
    Value::F64(out.iter().flatten().copied().collect())
}

#[test]
fn brace_wall_replays() {
    let cases = load_cases("brace", "brace_wall");
    assert!(cases.len() > 80);
    let mut built_any = 0;
    for c in &cases {
        let a = &c.args;
        let cfg = config(a[6].get("$config"));
        let top = p3(&a[0]);
        let floor = nums(&a[1]);
        let tris = Tris::new(nums(&a[2]));
        let kinds = BraceKinds {
            slim: matches!(opt_get(&a[5], "slim"), Some(Value::Bool(true))),
            tall: matches!(opt_get(&a[5], "tall"), Some(Value::Bool(true))),
        };
        let mut out = Vec::new();
        let built = brace_wall(
            &top,
            |i| floor[i],
            &tris,
            a[3].num(),
            &mut out,
            clear_fn(&a[4]),
            kinds,
            &cfg,
        );
        built_any += built;
        check(
            "brace_wall",
            c,
            Value::Obj(vec![
                ("built".into(), (built as f64).into()),
                ("tris".into(), flat(&out)),
            ]),
        );
    }
    assert!(
        built_any > 50,
        "only {built_any} ribs built across the corpus"
    );
}

#[test]
fn brace_cross_rib_replays() {
    let cases = load_cases("brace", "brace_cross_rib");
    assert!(cases.len() > 100);
    let mut built = 0;
    for c in &cases {
        let a = &c.args;
        let cfg = config(a[10].get("$config"));
        let tris = Tris::new(nums(&a[0]));
        let lean = p2(&a[6]);
        let mut out = Vec::new();
        let ret = brace_cross_rib(
            &mut out,
            &tris,
            p2(&a[1]),
            p2(&a[2]),
            a[3].num(),
            a[4].num(),
            a[5].num(),
            lean,
            clear_fn(&a[7]),
            a[8].num(),
            a[9].num(),
            &cfg,
        );
        built += usize::from(ret.is_some());
        let ret = match ret {
            Some(r) => Value::Obj(vec![
                ("reach".into(), r.reach.into()),
                ("th".into(), r.th.into()),
            ]),
            None => Value::Null,
        };
        check(
            "brace_cross_rib",
            c,
            Value::Obj(vec![("ret".into(), ret), ("tris".into(), flat(&out))]),
        );
    }
    assert!(built > 30, "only {built} cross ribs built");
}

#[test]
fn tie_walls_replays() {
    let cases = load_cases("ties", "tie_walls");
    assert!(cases.len() > 100);
    let mut tied = 0;
    for c in &cases {
        let a = &c.args;
        let cfg = config(a[5].get("$config"));
        let tris = Tris::new(nums(&a[3]));
        let mut out = Vec::new();
        let n = tie_walls(
            p2(&a[0]),
            p2(&a[1]),
            a[2].num(),
            &tris,
            a[4].num(),
            &mut out,
            &cfg,
        );
        tied += usize::from(n > 0);
        check(
            "tie_walls",
            c,
            Value::Obj(vec![
                ("n".into(), (n as f64).into()),
                ("tris".into(), flat(&out)),
            ]),
        );
    }
    assert!(tied > 20, "only {tied} ties built");
}
