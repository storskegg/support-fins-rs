//! Replays `parity/corpus/islands/*` (recorded from web/islands.js by `parity/oracle/islands.js`):
//! `findIslands` on the stress models (`$topo` / `$analysis` references) and on synthetic soups
//! (the replay builds the topology and the analysis). Pure arithmetic on the inputs, so
//! `Tol::Exact`. The `ISLAND` config is the default unless a `$config` argument says otherwise.

use ::parity::{Tol, Value, compare, load_cases};
use sf_engine::islands::{IslandOpts, find_islands};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::types::Rot3;
use sf_testkit::parity::{Resolved, resolve_args};
use sf_testkit::parity_config::config;

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        other => panic!("expected numbers, got {other:?}"),
    }
}

fn rot_of(v: &Value) -> Rot3 {
    <Rot3>::try_from(nums(v).as_slice()).expect("rot has 9 numbers")
}

fn opts_of(v: Option<&Value>) -> IslandOpts {
    let mut o = IslandOpts::default();
    let Some(Value::Obj(fields)) = v else {
        return o;
    };
    for (k, x) in fields {
        let n = Some(x.num());
        match k.as_str() {
            "layer" => o.layer = n,
            "grace" => o.grace = n,
            "minArea" => o.min_area = n,
            "plateZ" => o.plate_z = n,
            "merge" => o.merge = n,
            "mergeZ" => o.merge_z = n,
            "maxFaces" => o.max_faces = n,
            other => panic!("unknown findIslands option {other}"),
        }
    }
    o
}

fn islands_val(v: &[sf_engine::islands::Island]) -> Value {
    Value::Arr(
        v.iter()
            .map(|i| {
                Value::Obj(vec![
                    ("z".into(), i.z.into()),
                    ("area".into(), i.area.into()),
                    (
                        "points".into(),
                        Value::Arr(
                            i.points
                                .iter()
                                .map(|p| Value::Arr(p.iter().map(|&x| x.into()).collect()))
                                .collect(),
                        ),
                    ),
                    ("x".into(), i.x.into()),
                    ("y".into(), i.y.into()),
                ])
            })
            .collect(),
    )
}

fn check(file: &str, id: &str, got: &Value, want: &Value) {
    if let Err(m) = compare(got, want, &Tol::Exact) {
        panic!(
            "{file}/{id}: out{} differs: got {}, expected {}",
            m.path, m.actual, m.expected
        );
    }
}

#[test]
fn find_islands_direct() {
    let cases = load_cases("islands", "findIslands");
    assert!(cases.len() >= 250);
    for c in &cases {
        let a = resolve_args(&c.args);
        let (cfg, n) = match a.last() {
            Some(Resolved::Config(v)) => (config(v), a.len() - 1),
            _ => (config(&Value::Obj(vec![])), a.len()),
        };
        let opts = opts_of(a.get(3).filter(|_| n > 3).map(|r| r.plain()));
        let got = find_islands(
            a[0].topo(),
            a[1].analysis(),
            &rot_of(a[2].plain()),
            &opts,
            &cfg,
        );
        check(
            "findIslands",
            &c.id,
            &islands_val(&got),
            c.out.as_ref().unwrap(),
        );
    }
}

#[test]
fn find_islands_on_synthetic_parts() {
    let cases = load_cases("islands", "findIslands.soup");
    assert!(cases.len() >= 100);
    let cfg = config(&Value::Obj(vec![]));
    for c in &cases {
        let pos: Vec<f64> = match &c.args[0] {
            Value::F32(p) => p.iter().map(|&x| x as f64).collect(),
            other => panic!("expected a Float32Array, got {other:?}"),
        };
        let rot = rot_of(&c.args[2]);
        let topo = build_topology(pos);
        let res = analyze(&topo, c.args[1].num(), &rot);
        let opts = opts_of(c.args.get(3));
        let got = find_islands(&topo, &res, &rot, &opts, &cfg);
        check(
            "findIslands.soup",
            &c.id,
            &islands_val(&got),
            c.out.as_ref().unwrap(),
        );
    }
}
