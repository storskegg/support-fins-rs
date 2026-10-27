//! Replay of `parity/corpus/bridgerace/bridgeRace.jsonl` (recorded by `parity/oracle/bridgerace.js`):
//! each case scripts what every patch "builds" (and what the tine capture sees), and the loop
//! buildProps runs is replayed through `bridge_race`. `Tol::Exact`.

use std::sync::Arc;

use parity::{Tol, Value, compare, load_cases};
use sf_engine::config::Config;
use sf_engine::prop::bridgerace::bridge_race;
use sf_engine::prop::build::{Acc, Lines, Patch, PropRecord};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::{BuildCtx, TineSeed};

fn opt<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(kv) => kv.iter().find(|(kk, _)| kk == k).map(|(_, v)| v),
        _ => None,
    }
}

fn nums(v: &Value) -> Vec<f64> {
    v.arr().iter().map(Value::num).collect()
}

fn pair(r: [usize; 2]) -> Value {
    Value::from(vec![r[0] as f64, r[1] as f64])
}

#[test]
fn bridge_race_matches_the_oracle() {
    let cases = load_cases("bridgerace", "bridgeRace");
    assert!(cases.len() >= 200);
    let (mut twins_kept, mut twins_dropped) = (0, 0);
    for c in cases {
        let s = &c.args[0];
        let cfg = Config::default();
        let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        let cap0 = match s.get("cap") {
            Value::Num(n) => Some(*n as usize),
            _ => None,
        };
        if let Some(n) = cap0 {
            *ctx.tine_cap.borrow_mut() = Some(
                (0..n)
                    .map(|i| TineSeed {
                        x: i as f64,
                        y: 0.0,
                        z: 0.0,
                        bite_x: 0.0,
                        bite_y: 0.0,
                    })
                    .collect(),
            );
        }
        let mut patches: Vec<(Patch, String, Option<[f64; 2]>)> = Vec::new();
        for p in s.get("patches").arr() {
            let turned = opt(p, "turned").map(|t| {
                let a = nums(t);
                [a[0], a[1]]
            });
            let patch = Patch {
                region: p.get("region").num() as usize,
                tris: Arc::new(match p.get("tris") {
                    Value::F64(a) => a.clone(),
                    o => panic!("{o:?}"),
                }),
                axis: opt(p, "axis").map(|t| {
                    let a = nums(t);
                    [a[0], a[1]]
                }),
                ..Default::default()
            };
            patches.push((patch, p.get("name").str().to_string(), turned));
        }
        let mut race = bridge_race();
        let mut acc = Acc::default();
        let builds = s.get("builds");
        let mut seed = 0.0f64;
        let mut i = 0;
        while i < patches.len() {
            race.begin(&mut acc, &patches[i].0, &ctx);
            // every patch here took the patch path: patchTracks gave `lines.turned`
            {
                let lines = Lines {
                    turned: patches[i].2,
                    ..Default::default()
                };
                let mut plain: Vec<Patch> = patches.iter().map(|p| p.0.clone()).collect();
                let before = plain.len();
                race.turned(&acc, i, &lines, &mut plain, &ctx);
                if plain.len() > before {
                    // the twin takes its place in the queue, named `<name>#twin`
                    let name = patches[i].1.clone();
                    patches.insert(i + 1, (plain[i + 1].clone(), name, None));
                }
            }
            let key = if patches[i].0.bridge_twin {
                format!("{}#twin", patches[i].1)
            } else {
                patches[i].1.clone()
            };
            let b = builds.get(&key);
            let base = acc.out.len();
            for v in b.get("out").arr() {
                let a = v.arr();
                acc.out.push([a[0].num(), a[1].num(), a[2].num(), 0.0]);
            }
            let c0 = ctx.cap_len();
            if c0.is_some() {
                for _ in 0..b.get("caps").num() as usize {
                    ctx.capture(TineSeed {
                        x: 1000.0 + seed,
                        y: 1.0,
                        z: 2.0,
                        bite_x: 3.0,
                        bite_y: 4.0,
                    });
                    seed += 1.0;
                }
            }
            if !b.get("out").arr().is_empty() {
                let mut q = PropRecord {
                    region: patches[i].0.region,
                    tines: b.get("tines").num() as usize,
                    tri_ranges: vec![[base, acc.out.len()]],
                    ..Default::default()
                };
                if matches!(b.get("wall"), Value::Bool(true)) {
                    q.wall_range = Some([base, base + (acc.out.len() - base) / 2]);
                }
                if let Some(c0) = c0 {
                    q.caps = Some([c0, ctx.cap_len().unwrap()]);
                }
                acc.props.push(q);
            }
            acc.tally.tines += b.get("tines").num() as usize;
            acc.tally.sag_risk |= matches!(b.get("sagRisk"), Value::Bool(true));
            if let Value::Obj(kv) = b.get("skipped") {
                for (k, v) in kv {
                    for _ in 0..v.num() as usize {
                        acc.skipped.bump(k);
                    }
                }
            }
            if matches!(b.get("served"), Value::Bool(true)) {
                acc.served.insert(patches[i].0.region);
            }
            i += 1;
        }
        race.finish(&mut acc, &ctx);

        let sk = acc.skipped;
        let props: Vec<Value> = acc
            .props
            .iter()
            .map(|q| {
                let mut kv = vec![
                    ("region".to_string(), Value::from(q.region as f64)),
                    ("tines".to_string(), Value::from(q.tines as f64)),
                    (
                        "triRanges".to_string(),
                        Value::Arr(q.tri_ranges.iter().map(|r| pair(*r)).collect()),
                    ),
                ];
                if let Some(w) = q.wall_range {
                    kv.push(("wallRange".into(), pair(w)));
                }
                if let Some(cp) = q.caps {
                    kv.push(("caps".into(), pair(cp)));
                }
                Value::Obj(kv)
            })
            .collect();
        let cap = match ctx.tine_cap.borrow().as_ref() {
            Some(v) => Value::Arr(v.iter().map(|t| Value::from(t.x)).collect()),
            None => Value::Null,
        };
        let out = Value::Obj(vec![
            (
                "out".into(),
                Value::Arr(
                    acc.out
                        .iter()
                        .map(|v| Value::from(vec![v[0], v[1], v[2]]))
                        .collect(),
                ),
            ),
            ("props".into(), Value::Arr(props)),
            (
                "skipped".into(),
                Value::Obj(
                    [
                        "noLine",
                        "wanders",
                        "stub",
                        "blocked",
                        "degenerate",
                        "buried",
                        "weld",
                        "sliver",
                        "onPart",
                    ]
                    .iter()
                    .map(|k| (k.to_string(), Value::from(sk.get(k) as f64)))
                    .collect(),
                ),
            ),
            (
                "served".into(),
                Value::Arr(acc.served.iter().map(|&r| Value::from(r as f64)).collect()),
            ),
            (
                "tally".into(),
                Value::Obj(vec![
                    ("tines".into(), Value::from(acc.tally.tines as f64)),
                    ("sagRisk".into(), Value::from(acc.tally.sag_risk)),
                ]),
            ),
            ("cap".into(), cap),
            ("patches".into(), Value::from(patches.len() as f64)),
        ]);
        if let Err(m) = compare(&out, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "{}: out{}: actual {} vs expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
        if patches.iter().any(|p| p.0.bridge_twin) {
            if acc
                .props
                .iter()
                .any(|q| q.caps.is_some() || q.wall_range.is_some())
            {
                twins_kept += 1
            } else {
                twins_dropped += 1
            }
        }
    }
    assert!(
        twins_kept > 10 && twins_dropped > 10,
        "scenario mix: {twins_kept} / {twins_dropped}"
    );
}
