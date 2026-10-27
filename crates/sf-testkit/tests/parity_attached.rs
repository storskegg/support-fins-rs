//! Replay of `parity/corpus/attached/` (recorded by `parity/oracle/attached.js` from
//! web/prop/attached.js): `build_part_attached` over every model and pose (each region's contact
//! line against the whole seated part, with the sweep's config varied) and as the 54 golden
//! scenes' builds call it. Exact. `state` is the crest's module state at the call.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::attached::{Attached, build_part_attached};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_testkit::parity::{Resolved, resolve_args};
use std::sync::Arc;

fn verdict(a: &Attached) -> Value {
    match a {
        Attached::NoFloor => Value::Obj(vec![]),
        Attached::Floored(w) => Value::Obj(vec![("floored".into(), Value::from(w.as_str()))]),
        Attached::Built(p) => Value::Obj(vec![
            ("ok".into(), Value::Bool(true)),
            (
                "prop".into(),
                Value::Obj(vec![
                    ("span".into(), Value::Num(p.span)),
                    ("height".into(), Value::Num(p.height)),
                    ("stations".into(), Value::Num(p.stations as f64)),
                    ("volume".into(), Value::Num(p.volume)),
                    ("partAttached".into(), Value::Bool(p.part_attached)),
                    ("inBore".into(), Value::Bool(p.in_bore)),
                    ("line".into(), pts_value(&p.line)),
                ]),
            ),
        ]),
    }
}

/// The crest state `{part, layerHeight}`: with a `$seated` part it resolves as an object, otherwise
/// (`null`, or a soup recorded in full) it is a plain value.
fn crest_of(state: &Resolved, cfg: &sf_engine::config::Config) -> Crest {
    let (part, layer): (Option<Arc<Tris>>, f64) = match state {
        Resolved::Plain(v) => (
            match v.get("part") {
                Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
                Value::Null => None,
                other => panic!("crest part {other:?}"),
            },
            match v.get("layerHeight") {
                Value::Num(n) => *n,
                _ => 0.0, // undefined
            },
        ),
        r => (
            match r.get("part") {
                Resolved::Seated(t) => Some(t.clone()),
                other => panic!("crest part {other:?}"),
            },
            match r.get("layerHeight") {
                Resolved::Plain(Value::Num(n)) => *n,
                _ => 0.0,
            },
        ),
    };
    Crest::new(part, layer, cfg)
}

fn replay(file: &str) {
    let cases = load_cases("attached", file);
    assert!(!cases.is_empty());
    let (mut built, mut floored) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let ctx = BuildCtx::new(&cfg, crest_of(&a[6], &cfg));
        let line = pts3(a[0].plain());
        let mut out = Vec::new();
        let r = build_part_attached(
            &line,
            &tris_of(&a[1]),
            a[2].topo(),
            &rot3(a[3].plain()),
            &xyz(a[4].plain()),
            &mut out,
            &ctx,
        );
        match &r {
            Attached::Built(_) => built += 1,
            Attached::Floored(_) => floored += 1,
            Attached::NoFloor => {}
        }
        let actual = Value::Obj(vec![
            ("ret".into(), verdict(&r)),
            (
                "tris".into(),
                Value::F64(out.iter().flatten().copied().collect()),
            ),
        ]);
        check(file, c, &actual, &Tol::Exact);
    }
    assert!(
        built > 0 && floored > 0,
        "the corpus must reach built walls and refusals"
    );
}

#[test]
fn build_part_attached_replays() {
    replay("buildPartAttached");
}
