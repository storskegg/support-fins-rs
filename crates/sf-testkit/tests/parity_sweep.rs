//! Replays `parity/corpus/sweep/` (recorded by `parity/oracle/sweep.js`) with `Tol::Exact`:
//! `foot_for`, `stem_half`, `profile_half`, `sweep` and `sweep_between`, called directly and as
//! the 54 golden scenes' builds (and builds with the crest, a cutout pattern and vertical pins
//! switched on) call them. The crest's module state is the `state` argument.

use std::sync::Arc;

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::config::Config;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::sweep::{foot_for, profile_half, stem_half, sweep, sweep_between};
use sf_engine::tris::Tris;
use sf_testkit::parity::seated;
use sf_testkit::parity_config::config;

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        other => other.arr().iter().map(Value::num).collect(),
    }
}

fn cfg_of(c: &Case) -> Config {
    config(c.args.last().unwrap().get("$config"))
}

fn line(v: &Value) -> Vec<[f64; 3]> {
    v.arr()
        .iter()
        .map(|p| {
            let a = nums(p);
            [a[0], a[1], a[2]]
        })
        .collect()
}

fn crest_of(state: &Value, cfg: &Config) -> Crest {
    let part = match state.get("part") {
        Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
        Value::Obj(o) if o.len() == 1 && o[0].0 == "$seated" => Some(seated(&o[0].1)),
        Value::Null => None,
        other => panic!("unexpected crest part {other:?}"),
    };
    let lh = match state.get("layerHeight") {
        Value::Num(n) => *n,
        _ => 0.0, // undefined
    };
    Crest::new(part, lh, cfg)
}

fn replay(function: &str, min: usize, mut f: impl FnMut(&Case) -> Value) {
    let cases = load_cases("sweep", function);
    assert!(cases.len() >= min, "{function}: {} cases", cases.len());
    for c in &cases {
        if let Err(m) = compare(&f(c), c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "sweep/{function}/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

fn result(ret: bool, out: &[[f64; 4]]) -> Value {
    Value::Obj(vec![
        ("ret".into(), Value::Bool(ret)),
        (
            "tris".into(),
            Value::F64(out.iter().flatten().copied().collect()),
        ),
    ])
}

#[test]
fn foot_for_replays() {
    replay("foot_for", 100, |c| {
        Value::Num(foot_for(c.args[0].num(), &cfg_of(c)))
    });
}

#[test]
fn stem_half_replays() {
    replay("stem_half", 100, |c| {
        Value::Num(stem_half(c.args[0].num(), &cfg_of(c)))
    });
}

#[test]
fn profile_half_replays() {
    replay("profile_half", 100, |c| {
        Value::Num(profile_half(c.args[0].num(), c.args[1].num(), &cfg_of(c)))
    });
}

#[test]
fn sweep_replays() {
    let (mut built, mut tagged) = (0, 0);
    replay("sweep", 100, |c| {
        let cfg = cfg_of(c);
        let n = c.args.len();
        let min_h = if n == 5 {
            c.args[2].num()
        } else {
            cfg.prop.min_height
        };
        let ctx = BuildCtx::new(&cfg, crest_of(&c.args[n - 2], &cfg));
        let mut out = Vec::new();
        let ret = sweep(&line(&c.args[0]), c.args[1].num(), &mut out, min_h, &ctx);
        built += usize::from(ret);
        tagged += out.iter().filter(|v| v[3] == 1.0).count();
        result(ret, &out)
    });
    assert!(built > 50, "only {built} walls built");
    assert!(tagged > 0, "no interface-crest vertices in the corpus");
}

#[test]
fn sweep_between_replays() {
    let mut built = 0;
    replay("sweep_between", 50, |c| {
        let cfg = cfg_of(c);
        let n = c.args.len();
        let min_h = if n == 5 {
            c.args[2].num()
        } else {
            cfg.prop.min_height
        };
        let ctx = BuildCtx::new(&cfg, crest_of(&c.args[n - 2], &cfg));
        let bot: Vec<Vec<f64>> = c.args[1].arr().iter().map(nums).collect();
        let mut out = Vec::new();
        let ret = sweep_between(&line(&c.args[0]), &bot, &mut out, min_h, &ctx);
        built += usize::from(ret);
        result(ret, &out)
    });
    assert!(built > 20, "only {built} walls built");
}
