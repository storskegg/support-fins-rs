//! Replay of `parity/corpus/surface/*` (recorded by `parity/oracle/surface.js` from web/prop/surface.js):
//! direct columns over the stress models and fixtures, and the calls the 54 golden scenes make.
//! Exact: the functions are arithmetic on their inputs.
use ::parity::{Tol, Value, compare, load_cases};
use sf_engine::prop::surface::{surface_hits_at, surface_z_at, surface_zs_at};
use sf_engine::tris::Tris;
use sf_testkit::parity::{Resolved, resolve_args};
use std::sync::Arc;

/// The soup argument: a `$seated` reference, or a soup recorded in full.
fn soup(r: &Resolved) -> Arc<Tris> {
    match r {
        Resolved::Seated(t) => t.clone(),
        Resolved::Plain(Value::F64(v)) => Arc::new(Tris::new(v.clone())),
        other => panic!("tris argument: {other:?}"),
    }
}

fn replay(function: &str, f: impl Fn(&Tris, f64, f64) -> Value) {
    let cases = load_cases("surface", function);
    assert!(!cases.is_empty(), "{function}: no cases");
    for c in &cases {
        let a = resolve_args(&c.args);
        let (x, y) = (a[1].plain().num(), a[2].plain().num());
        let actual = f(&soup(&a[0]), x, y);
        compare(&actual, c.out.as_ref().unwrap(), &Tol::Exact)
            .unwrap_or_else(|m| panic!("{function} {}: {m:?}", c.id));
    }
}

fn z(v: Option<f64>) -> Value {
    v.map_or(Value::Null, Value::Num)
}
fn pairs(v: Vec<[f64; 2]>) -> Value {
    Value::Arr(v.into_iter().map(|p| Value::from(p.to_vec())).collect())
}

#[test]
fn surface_z_at_direct() {
    replay("surfaceZAt", |t, x, y| z(surface_z_at(t, x, y)));
}
#[test]
fn surface_hits_at_direct() {
    replay("surfaceHitsAt", |t, x, y| pairs(surface_hits_at(t, x, y)));
}
#[test]
fn surface_zs_at_direct() {
    replay("surfaceZsAt", |t, x, y| Value::from(surface_zs_at(t, x, y)));
}
#[test]
fn surface_z_at_captured() {
    replay("surfaceZAt.captured", |t, x, y| z(surface_z_at(t, x, y)));
}
#[test]
fn surface_hits_at_captured() {
    replay("surfaceHitsAt.captured", |t, x, y| {
        pairs(surface_hits_at(t, x, y))
    });
}
#[test]
fn surface_zs_at_captured() {
    replay("surfaceZsAt.captured", |t, x, y| {
        Value::from(surface_zs_at(t, x, y))
    });
}
