//! Replay of `parity/corpus/rival/tubeRivalry.jsonl` (recorded by `parity/oracle/rival.js`): each case
//! scripts what every patch "builds", and the loop buildProps runs is replayed through
//! `tube_rivalry`. `Tol::Exact`.

use std::sync::Arc;

use indexmap::IndexSet;
use parity::{Tol, Value, compare, load_cases};
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Region, build_topology};
use sf_engine::prop::build::{Acc, Patch, PropRecord};
use sf_engine::prop::rival::tube_rivalry;
use sf_engine::tris::Tris;
use sf_engine::types::Xyz;

fn f64s(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        o => panic!("{o:?}"),
    }
}

fn opt<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(kv) => kv.iter().find(|(kk, _)| kk == k).map(|(_, v)| v),
        _ => None,
    }
}

struct Spec {
    name: String,
    patch: Patch,
    small: Vec<Spec>,
}

fn spec_of(v: &Value) -> Spec {
    let tris = Arc::new(Tris::new(f64s(v.get("tris"))));
    let small: Vec<Spec> = opt(v, "smallTube")
        .map(|s| s.arr().iter().map(spec_of).collect())
        .unwrap_or_default();
    let mut patch = Patch {
        region: v.get("region").num() as usize,
        area: v.get("area").num(),
        tris,
        reach: opt(v, "reach").map(|r| Arc::new(Tris::new(f64s(r)))),
        arc: matches!(opt(v, "arc"), Some(Value::Bool(true))),
        ..Default::default()
    };
    if opt(v, "smallTube").is_some() {
        patch.small_tube = Some(small.iter().map(|s| s.patch.clone()).collect());
    }
    Spec {
        name: v.get("name").str().to_string(),
        patch,
        small,
    }
}

#[test]
fn tube_rivalry_matches_the_oracle() {
    let cases = load_cases("rival", "tubeRivalry");
    assert!(cases.len() >= 300);
    let (mut kept_tube, mut kept_patches) = (0, 0);
    for c in cases {
        let s = &c.args[0];
        let soup = f64s(s.get("soup"));
        let topo = build_topology(soup.iter().map(|&v| (v as f32) as f64).collect());
        let regions: Vec<Region> = s
            .get("regions")
            .arr()
            .iter()
            .map(|r| Region {
                faces: r
                    .get("faces")
                    .arr()
                    .iter()
                    .map(|f| f.num() as usize)
                    .collect(),
                area: r.get("area").num(),
            })
            .collect();
        let res = Analysis {
            over: vec![],
            kept: vec![],
            on_bed: vec![],
            regions,
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: 0.0,
            over_face_count: 0,
            offset: Xyz::default(),
            size: Xyz::default(),
        };
        let part = f64s(s.get("part"));
        let cfg = Config::default();
        let mut rv = tube_rivalry(&topo, &res, &part, &cfg);
        let specs: Vec<Spec> = s.get("patches").arr().iter().map(spec_of).collect();
        let mut patches: Vec<Patch> = specs.iter().map(|s| s.patch.clone()).collect();
        let mut names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
        let mut acc = Acc::default();
        let builds = s.get("builds");
        let mut i = 0;
        while i < patches.len() {
            let mark = rv.begin(&mut acc, &patches[i]);
            let b = builds.get(&names[i]);
            let base = acc.out.len();
            for v in b.get("out").arr() {
                let a = v.arr();
                acc.out.push([a[0].num(), a[1].num(), a[2].num(), 0.0]);
            }
            for q in b.get("props").arr() {
                acc.props.push(PropRecord {
                    region: Some(q.get("region").num() as usize),
                    tines: q.get("tines").num() as usize,
                    tri_ranges: vec![[base, acc.out.len()]],
                    ..Default::default()
                });
            }
            acc.tally.tines += b.get("tines").num() as usize;
            acc.tally.sag_risk |= matches!(b.get("sagRisk"), Value::Bool(true));
            if let Value::Obj(kv) = b.get("skipped") {
                for (k, v) in kv {
                    let n = v.num() as usize;
                    for _ in 0..n {
                        acc.skipped.bump(k);
                    }
                }
            }
            if matches!(b.get("served"), Value::Bool(true)) {
                acc.served.insert(patches[i].region);
            }
            if patches[i].small_tube.is_some() {
                let before = patches.len();
                rv.tube_done(
                    &mut acc,
                    i,
                    mark.expect("a small tube gets a mark"),
                    &mut patches,
                );
                // the queued rivals are the big children, in order
                let big: Vec<String> = specs_children(&specs, &names[i])
                    .iter()
                    .filter(|c| c.patch.area >= 12.0)
                    .map(|c| c.name.clone())
                    .collect();
                assert_eq!(patches.len() - before, big.len());
                names.extend(big);
            }
            i += 1;
        }
        rv.finish(&mut acc);

        let served: Vec<Value> = acc.served.iter().map(|&r| Value::from(r as f64)).collect();
        let _: IndexSet<usize> = acc.served.clone();
        let sk = acc.skipped;
        let skipped = Value::Obj(
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
        );
        let props: Vec<Value> = acc
            .props
            .iter()
            .map(|q| {
                Value::Obj(vec![
                    (
                        "region".into(),
                        q.region.map_or(Value::Null, |r| Value::from(r as f64)),
                    ),
                    ("tines".into(), Value::from(q.tines as f64)),
                    (
                        "triRanges".into(),
                        Value::Arr(
                            q.tri_ranges
                                .iter()
                                .map(|r| Value::from(vec![r[0] as f64, r[1] as f64]))
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect();
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
            ("skipped".into(), skipped),
            ("served".into(), Value::Arr(served)),
            (
                "tally".into(),
                Value::Obj(vec![
                    ("tines".into(), Value::from(acc.tally.tines as f64)),
                    ("sagRisk".into(), Value::from(acc.tally.sag_risk)),
                ]),
            ),
            ("patches".into(), Value::from(patches.len() as f64)),
        ]);
        if let Err(m) = compare(&out, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "{}: out{}: actual {} vs expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
        if acc.props.is_empty() {
            kept_patches += 1;
        } else {
            kept_tube += 1;
        }
    }
    assert!(
        kept_tube > 20 && kept_patches > 20,
        "scenario mix: {kept_tube} / {kept_patches}"
    );
}

fn specs_children<'a>(specs: &'a [Spec], name: &str) -> &'a [Spec] {
    for s in specs {
        if s.name == name {
            return &s.small;
        }
    }
    &[]
}
