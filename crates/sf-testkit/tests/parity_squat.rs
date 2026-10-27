//! Replays `parity/corpus/squat/` (recorded by `parity/oracle/squat.js`) with `Tol::Exact`:
//! `sweep_squat` and `build_squat_bed`, called directly and as the 54 golden scenes' builds (and
//! builds with the crest on everywhere and vertical pins) call them. The crest's module state is
//! the `state` argument.

use std::sync::Arc;

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::config::Config;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::squat::{build_squat_bed, sweep_squat};
use sf_engine::tris::Tris;
use sf_engine::types::Xyz;
use sf_testkit::parity::{resolve, seated};
use sf_testkit::parity_config::config;

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        other => other.arr().iter().map(Value::num).collect(),
    }
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

fn part_of(v: &Value) -> Option<Arc<Tris>> {
    match v {
        Value::F64(a) => Some(Arc::new(Tris::new(a.clone()))),
        Value::Obj(o) if o.len() == 1 && o[0].0 == "$seated" => Some(seated(&o[0].1)),
        Value::Null => None,
        other => panic!("unexpected part {other:?}"),
    }
}

fn crest_of(state: &Value, cfg: &Config) -> Crest {
    let lh = match state.get("layerHeight") {
        Value::Num(n) => *n,
        _ => 0.0, // undefined
    };
    Crest::new(part_of(state.get("part")), lh, cfg)
}

fn replay(function: &str, min: usize, mut f: impl FnMut(&Case) -> Value) {
    let cases = load_cases("squat", &format!("{function}.captured"));
    assert!(cases.len() >= min, "{function}: {} cases", cases.len());
    for c in &cases {
        if let Err(m) = compare(&f(c), c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "squat/{function}/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

fn flat4(out: &[[f64; 4]]) -> Value {
    Value::F64(out.iter().flatten().copied().collect())
}

#[test]
fn sweep_squat_replays() {
    let (mut built, mut tagged) = (0, 0);
    replay("sweepSquat", 100, |c| {
        let a = &c.args;
        let cfg = config(a.last().unwrap().get("$config"));
        let ctx = BuildCtx::new(&cfg, crest_of(&a[a.len() - 2], &cfg));
        let inset: Option<Vec<f64>> = match &a[3] {
            Value::Arr(v) => Some(v.iter().map(Value::num).collect()),
            _ => None,
        };
        let mut out = Vec::new();
        let ret = sweep_squat(&line(&a[0]), a[1].num(), &mut out, inset.as_deref(), &ctx);
        built += usize::from(ret);
        tagged += out.iter().filter(|v| v[3] == 1.0).count();
        Value::Obj(vec![
            ("ret".into(), Value::Bool(ret)),
            ("tris".into(), flat4(&out)),
        ])
    });
    assert!(built > 60, "only {built} walls built");
    assert!(tagged > 0, "no crest vertices in the corpus");
}

#[test]
fn build_squat_bed_replays() {
    let mut placed_walls = 0;
    replay("buildSquatBed", 40, |c| {
        let a = &c.args;
        let cfg = config(a.last().unwrap().get("$config"));
        let ctx = BuildCtx::new(&cfg, crest_of(&a[a.len() - 2], &cfg));
        let region = part_of(&a[1]).expect("regionTris");
        let topo = resolve(&a[2]).topo().clone();
        let rot: [f64; 9] = nums(&a[3]).try_into().expect("rot");
        let o = &a[4];
        let offset = Xyz {
            x: o.get("x").num(),
            y: o.get("y").num(),
            z: o.get("z").num(),
        };
        let claimed: Option<Vec<bool>> = match &a[6] {
            Value::Arr(v) => Some(v.iter().map(|b| matches!(b, Value::Bool(true))).collect()),
            _ => None,
        };
        // `triRange` is absolute in the caller's `out`: start from the length it had
        let from = c.out.as_ref().unwrap().get("from").num() as usize;
        let mut out = vec![[0.0; 4]; from];
        let placed = build_squat_bed(
            &line(&a[0]),
            &region,
            &topo,
            &rot,
            &offset,
            &mut out,
            claimed.as_deref(),
            &ctx,
        );
        placed_walls += placed.len();
        let placed = Value::Arr(
            placed
                .into_iter()
                .map(|w| {
                    Value::Obj(vec![
                        ("span".into(), w.span.into()),
                        ("height".into(), w.height.into()),
                        ("stations".into(), (w.stations as f64).into()),
                        ("volume".into(), w.volume.into()),
                        ("squat".into(), Value::Bool(w.squat)),
                        (
                            "line".into(),
                            Value::Arr(w.line.into_iter().map(|p| p.to_vec().into()).collect()),
                        ),
                        (
                            "triRange".into(),
                            vec![w.tri_range[0] as f64, w.tri_range[1] as f64].into(),
                        ),
                    ])
                })
                .collect(),
        );
        Value::Obj(vec![
            ("placed".into(), placed),
            ("from".into(), (from as f64).into()),
            ("tris".into(), flat4(&out[from..])),
        ])
    });
    assert!(placed_walls > 20, "only {placed_walls} squat walls placed");
}
