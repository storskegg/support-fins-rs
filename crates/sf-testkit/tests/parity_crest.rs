//! Replays `parity/corpus/crest/` (recorded by `parity/oracle/crest.js`) with `Tol::Exact`.
//! JS keeps `part` / `layerH` in module state; each case carries it as a `state` argument.
//! `P(o, z)` is fixed to `[2 + 0.5 * o, 3 - 0.25 * o, z]`; a vertex tag is a bool per vertex.

mod common;
use common::nums;
use std::sync::Arc;

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::config::Config;
use sf_engine::prop::crest::{
    Crest, CrestKind, crest_cut, crest_h, crest_kinds, crest_on, drop_collapsed, emit_crest,
    flat_crest_ring, flat_cut, mark_interface, split_interface,
};
use sf_engine::tris::Tris;
use sf_engine::types::V4;
use sf_testkit::parity_config::config;

/// A recorded Float64Array of xyz triples as untagged soup vertices.
fn verts(v: &Value) -> Vec<V4> {
    nums(v).chunks(3).map(|c| [c[0], c[1], c[2], 0.0]).collect()
}

/// The vertices' xyz, as recorded (the tag is compared separately, as bools).
fn flat(v: &[V4]) -> Value {
    Value::F64(v.iter().flat_map(|v| [v[0], v[1], v[2]]).collect())
}

fn tagged(v: &[V4]) -> Value {
    Value::Arr(v.iter().map(|v| Value::Bool(v[3] == 1.0)).collect())
}

fn p(o: f64, z: f64) -> [f64; 3] {
    [2.0 + 0.5 * o, 3.0 - 0.25 * o, z]
}

fn cfg_of(c: &Case) -> Config {
    config(c.args.last().unwrap().get("$config"))
}

fn crest_of(state: &Value, cfg: &Config) -> Crest {
    let part = match state.get("part") {
        Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
        _ => None,
    };
    let lh = match state.get("layerHeight") {
        Value::Num(n) => *n,
        _ => 0.0, // undefined
    };
    Crest::new(part, lh, cfg)
}

fn kind_value(k: Option<CrestKind>) -> Value {
    match k {
        Some(CrestKind::Flat) => "flat".into(),
        Some(CrestKind::Slope) => "slope".into(),
        None => Value::Null,
    }
}

fn replay(function: &str, min: usize, mut f: impl FnMut(&Case) -> Value) {
    let cases = load_cases("crest", function);
    assert!(cases.len() >= min, "{function}: {} cases", cases.len());
    for c in &cases {
        if let Err(m) = compare(&f(c), c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "crest/{function}/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

#[test]
fn crest_on_replays() {
    replay("crest_on", 8, |c| Value::Bool(crest_on(&cfg_of(c))));
}

#[test]
fn crest_h_replays() {
    replay("crest_h", 40, |c| {
        let cfg = cfg_of(c);
        Value::Num(crest_h(&crest_of(&c.args[0], &cfg), &cfg))
    });
}

#[test]
fn crest_kinds_replays() {
    let mut some = 0;
    replay("crest_kinds", 100, |c| {
        let cfg = cfg_of(c);
        let crest = crest_of(&c.args[1], &cfg);
        let pts: Vec<[f64; 3]> = c.args[0]
            .arr()
            .iter()
            .map(|v| {
                let a = nums(v);
                [a[0], a[1], a[2]]
            })
            .collect();
        match crest_kinds(&crest, &pts, &cfg) {
            None => Value::Null,
            Some(k) => {
                some += k.iter().flatten().count();
                Value::Arr(k.into_iter().map(kind_value).collect())
            }
        }
    });
    assert!(some > 50, "only {some} crest stations in the corpus");
}

#[test]
fn crest_cut_replays() {
    replay("crest_cut", 500, |c| {
        let cfg = cfg_of(c);
        let crest = crest_of(&c.args[6], &cfg);
        let kind = match &c.args[3] {
            Value::Str(s) if s == "flat" => Some(CrestKind::Flat),
            Value::Str(s) if s == "slope" => Some(CrestKind::Slope),
            _ => None,
        };
        match crest_cut(
            &crest,
            c.args[0].num(),
            c.args[1].num(),
            c.args[2].num(),
            kind,
            c.args[4].num(),
            c.args[5].num(),
            &cfg,
        ) {
            None => Value::Null,
            Some(r) => Value::Obj(vec![
                ("cut".into(), r.cut.into()),
                ("zt".into(), r.zt.into()),
                ("wc".into(), r.wc.into()),
                (
                    "ring".into(),
                    Value::Arr(r.ring(p).into_iter().map(|v| v.to_vec().into()).collect()),
                ),
            ]),
        }
    });
}

#[test]
fn flat_cut_replays() {
    replay("flat_cut", 80, |c| {
        let cfg = cfg_of(c);
        Value::Num(flat_cut(&crest_of(&c.args[1], &cfg), c.args[0].num(), &cfg))
    });
}

#[test]
fn flat_crest_ring_replays() {
    replay("flat_crest_ring", 80, |c| {
        let cfg = cfg_of(c);
        let ring = flat_crest_ring(
            &crest_of(&c.args[2], &cfg),
            p,
            c.args[0].num(),
            c.args[1].num(),
            &cfg,
        );
        Value::Arr(ring.into_iter().map(|v| v.to_vec().into()).collect())
    });
}

#[test]
fn emit_crest_replays() {
    replay("emit_crest", 7, |c| {
        let secs: Vec<Option<Vec<[f64; 3]>>> = c.args[0]
            .arr()
            .iter()
            .map(|s| match s {
                Value::Null => None,
                s => Some(
                    s.arr()
                        .iter()
                        .map(|v| {
                            let a = nums(v);
                            [a[0], a[1], a[2]]
                        })
                        .collect(),
                ),
            })
            .collect();
        let mut out = Vec::new();
        emit_crest(&secs, &mut out);
        Value::Obj(vec![
            ("tris".into(), flat(&out)),
            ("tagged".into(), tagged(&out)),
        ])
    });
}

#[test]
fn mark_interface_replays() {
    replay("mark_interface", 20, |c| {
        let mut tris = verts(&c.args[0]);
        mark_interface(&mut tris, c.args[1].num() as usize);
        tagged(&tris)
    });
}

#[test]
fn drop_collapsed_replays() {
    replay("drop_collapsed", 20, |c| {
        let mut tris = verts(&c.args[0]);
        drop_collapsed(&mut tris, c.args[1].num() as usize);
        flat(&tris)
    });
}

#[test]
fn split_interface_replays() {
    replay("split_interface", 20, |c| {
        let mut tris = verts(&c.args[0]);
        for (v, t) in tris.iter_mut().zip(c.args[1].arr()) {
            if matches!(t, Value::Bool(true)) {
                v[3] = 1.0;
            }
        }
        let (body, iface) = split_interface(&tris);
        Value::Obj(vec![
            ("body".into(), flat(&body)),
            ("iface".into(), flat(&iface)),
        ])
    });
}
