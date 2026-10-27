//! Replay of `parity/corpus/draw/` (recorded by `parity/oracle/draw.js` from web/draw.js): `drawn_line`
//! and `drawn_wall` over every model/pose/option set (direct) and as the golden scenes' builds and
//! three Full-coverage builds call them (rows.js, fill.js). Exact. `drawnWall` sets the crest state
//! itself, so every call starts from a context with none.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_line, drawn_wall};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_testkit::parity::{Resolved, resolve_args};

/// `opts[key]` as a value, absent / `undefined` as `None`.
fn field<'a>(opts: &'a Resolved, key: &str) -> Option<&'a Value> {
    let v = match opts {
        Resolved::Plain(Value::Obj(o)) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        Resolved::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| match v {
            Resolved::Plain(v) => v,
            other => panic!("opts.{key}: {other:?}"),
        }),
        other => panic!("opts: {other:?}"),
    };
    v.filter(|v| !matches!(v, Value::Undefined))
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Num(n)) => *n != 0.0 && !n.is_nan(),
        Some(_) => true,
    }
}

fn num_opt(v: Option<&Value>) -> Option<f64> {
    v.filter(|v| !matches!(v, Value::Null)).map(Value::num)
}

fn opts_of(r: &Resolved) -> DrawOpts<'_> {
    let topo = match r {
        Resolved::Obj(o) => o
            .iter()
            .find(|(k, _)| k == "topo")
            .map(|(_, v)| v.topo().as_ref()),
        _ => None,
    };
    DrawOpts {
        under: truthy(field(r, "under")),
        plate_only: truthy(field(r, "plateOnly")),
        brace: truthy(field(r, "brace")),
        brace_tall: truthy(field(r, "braceTall")),
        brace_clear: None,
        through: field(r, "through")
            .filter(|v| !matches!(v, Value::Null))
            .map(p3),
        layer_height: num_opt(field(r, "layerHeight")),
        tines: truthy(field(r, "tines")),
        topo,
        rot: field(r, "rot").map(rot3),
        offset: field(r, "offset").map(xyz),
        tine_density: num_opt(field(r, "tineDensity")),
    }
}

#[test]
fn drawn_line_replays() {
    for file in ["drawnLine.captured"] {
        let cases = load_cases("draw", file);
        assert!(cases.len() > 1000);
        let (mut some, mut none) = (0, 0);
        for c in &cases {
            let a = resolve_args(&c.args);
            let cfg = cfg_of(&a);
            // trailing arguments the caller left out take JS's defaults (step = PROP.stationStep,
            // band = Infinity, under = false, through = null); the last argument is the config
            let given = |i: usize| -> Option<&Value> {
                (i < a.len() - 1)
                    .then(|| a[i].plain())
                    .filter(|v| !matches!(v, Value::Undefined))
            };
            let through = match given(6) {
                None | Some(Value::Null) => None,
                Some(v) => Some(p3(v)),
            };
            let r = drawn_line(
                p3(a[0].plain()),
                p3(a[1].plain()),
                &tris_of(&a[2]),
                given(3).map_or(cfg.prop.station_step, Value::num),
                given(4).map_or(f64::INFINITY, Value::num),
                truthy(given(5)),
                through,
                &cfg,
            );
            let actual = match &r {
                Some(l) => {
                    some += 1;
                    pts_value(l)
                }
                None => {
                    none += 1;
                    Value::Null
                }
            };
            check(file, c, &actual, &Tol::Exact);
        }
        assert!(some > 0 && none > 0);
    }
}

#[test]
fn drawn_wall_replays() {
    let file = "drawnWall.captured";
    let cases = load_cases("draw", file);
    assert!(cases.len() > 1000);
    let (mut plate, mut attached, mut squat, mut refused) = (0, 0, 0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        let opts = opts_of(&a[4]);
        let r = drawn_wall(
            p3(a[0].plain()),
            p3(a[1].plain()),
            &tris_of(&a[2]),
            a[3].plain().num(),
            &opts,
            &mut ctx,
        );
        let actual = match &r {
            DrawnWall::Refused { reason } => {
                refused += 1;
                Value::Obj(vec![
                    ("ok".into(), Value::Bool(false)),
                    ("reason".into(), Value::from(reason.as_str())),
                ])
            }
            DrawnWall::Ok(w) => {
                let mut o = vec![
                    ("ok".to_string(), Value::Bool(true)),
                    (
                        "tris".into(),
                        Value::F64(w.tris.iter().flatten().copied().collect()),
                    ),
                    ("length".into(), Value::Num(w.length)),
                    ("height".into(), Value::Num(w.height)),
                ];
                if let Some(p) = w.part_attached {
                    o.push(("partAttached".into(), Value::Bool(p)));
                }
                if w.squat {
                    o.push(("squat".into(), Value::Bool(true)));
                    squat += 1;
                } else if w.part_attached.is_some() {
                    attached += 1;
                } else {
                    plate += 1;
                }
                o.push(("tines".into(), Value::Num(w.tines as f64)));
                if let Some(b) = w.braces {
                    o.push(("braces".into(), Value::Num(b as f64)));
                }
                o.push(("top".into(), pts_value(&w.top)));
                if let Some(f) = &w.floors {
                    o.push(("floors".into(), num_value(f)));
                }
                Value::Obj(o)
            }
        };
        check(file, c, &actual, &Tol::Exact);
    }
    assert!(
        plate > 0 && attached > 0 && squat > 0 && refused > 0,
        "the corpus must reach every return: {plate} {attached} {squat} {refused}"
    );
}
