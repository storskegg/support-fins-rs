//! Replays every function's recorded V8 cases (parity/corpus/jsnum/<fn>.jsonl) with `Tol::Exact`.
//! No case is skipped or filtered. A failure lists the first mismatching cases with their ids.

use parity::{Tol, Value, compare, load_cases};
use sf_engine::jsnum;

/// Runs `f` over every case of `name`; `f` receives the recorded args.
fn replay(name: &str, f: impl Fn(&[Value]) -> Value) {
    let cases = load_cases("jsnum", name);
    assert!(!cases.is_empty(), "jsnum/{name}: no cases");
    let mut bad = Vec::new();
    for c in &cases {
        let expected = c
            .out
            .as_ref()
            .unwrap_or_else(|| panic!("jsnum/{name} case {} has no out", c.id));
        let actual = f(&c.args);
        if let Err(m) = compare(&actual, expected, &Tol::Exact) {
            bad.push(format!(
                "  {} ({}): got {} want {}",
                c.id, m.path, m.actual, m.expected
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "jsnum/{name}: {} of {} cases differ from V8; first {}:\n{}",
        bad.len(),
        cases.len(),
        bad.len().min(10),
        bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}

fn unary(name: &str, f: fn(f64) -> f64) {
    replay(name, |a| Value::from(f(a[0].num())));
}
fn binary(name: &str, f: fn(f64, f64) -> f64) {
    replay(name, |a| Value::from(f(a[0].num(), a[1].num())));
}
fn variadic(name: &str, f: fn(&[f64]) -> f64) {
    replay(name, |a| {
        let xs: Vec<f64> = a[0].arr().iter().map(Value::num).collect();
        Value::from(f(&xs))
    });
}

macro_rules! unary_tests {
    ($($name:ident),* $(,)?) => { $( #[test] fn $name() { unary(stringify!($name), jsnum::$name); } )* };
}
macro_rules! binary_tests {
    ($($name:ident),* $(,)?) => { $( #[test] fn $name() { binary(stringify!($name), jsnum::$name); } )* };
}

unary_tests!(
    round, trunc, floor, ceil, sign, abs, sqrt, sin, cos, tan, asin, acos, atan, exp, log, log2,
    log10, cbrt
);
binary_tests!(min, max, atan2, pow, ushr);

#[test]
fn min_of() {
    variadic("min_of", jsnum::min_of);
}
#[test]
fn max_of() {
    variadic("max_of", jsnum::max_of);
}
#[test]
fn hypot() {
    variadic("hypot", jsnum::hypot);
}
#[test]
fn to_int32() {
    replay("to_int32", |a| {
        Value::from(jsnum::to_int32(a[0].num()) as f64)
    });
}
#[test]
fn to_uint32() {
    replay("to_uint32", |a| {
        Value::from(jsnum::to_uint32(a[0].num()) as f64)
    });
}
#[test]
fn imul() {
    replay("imul", |a| {
        Value::from(jsnum::imul(a[0].num(), a[1].num()) as f64)
    });
}
#[test]
fn to_js_string() {
    replay("to_js_string", |a| {
        Value::from(jsnum::to_js_string(a[0].num()))
    });
}
#[test]
fn to_fixed() {
    replay("to_fixed", |a| {
        Value::from(jsnum::to_fixed(a[0].num(), a[1].num() as u32))
    });
}
#[test]
fn string_to_number() {
    replay("string_to_number", |a| {
        Value::from(jsnum::string_to_number(a[0].str()))
    });
}
#[test]
fn parse_float() {
    replay("parse_float", |a| {
        Value::from(jsnum::parse_float(a[0].str()))
    });
}
