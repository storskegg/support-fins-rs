//! Replays `parity/corpus/cutout/*.captured.jsonl` (recorded by `parity/oracle/cutout.js`):
//! `holesFor`, `honeycombHoles` and `cutWall`, called directly and as real builds call them.
//! `Tol::Exact`. The trailing `{"$config": ...}` argument is the config snapshot at call time.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::config::Config;
use sf_engine::cutout::{Station, Wall, cut_wall, holes_for, honeycomb_holes};
use sf_testkit::parity_config::config;

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        other => other.arr().iter().map(Value::num).collect(),
    }
}

/// The case's config: the trailing `$config` argument, applied over the defaults.
fn cfg_of(c: &Case) -> Config {
    config(c.args.last().expect("args").get("$config"))
}

fn opt(v: &Value, key: &str) -> Option<f64> {
    match v {
        Value::Obj(o) => match o.iter().find(|(k, _)| k == key) {
            Some((_, Value::Undefined)) | None => None,
            Some((_, x)) => Some(x.num()),
        },
        _ => panic!("expected an object"),
    }
}

fn station(v: &Value) -> Station {
    let p = nums(v.get("p"));
    Station {
        p: [p[0], p[1]],
        sx: v.get("sx").num(),
        sy: v.get("sy").num(),
        top: v.get("top").num(),
        ztip: v.get("ztip").num(),
        hole_top: opt(v, "holeTop"),
        tip_half: opt(v, "tipHalf"),
        sh: opt(v, "sh"),
        bot: v.get("bot").num(),
        bot_tip: v.get("botTip").num(),
        taper_bot: matches!(v.get("taperBot"), Value::Bool(true)),
        bot_n: opt(v, "botN"),
        bot_p: opt(v, "botP"),
    }
}

fn sections(v: &Value) -> Vec<Vec<[f64; 3]>> {
    v.arr()
        .iter()
        .map(|s| {
            s.arr()
                .iter()
                .map(|p| {
                    let a = nums(p);
                    [a[0], a[1], a[2]]
                })
                .collect()
        })
        .collect()
}

fn replay(function: &str, min: usize, mut f: impl FnMut(&Case) -> Value) {
    let cases = load_cases("cutout", &format!("{function}.captured"));
    assert!(cases.len() >= min, "{function}: {} cases", cases.len());
    for c in &cases {
        let want = c.out.as_ref().expect("out");
        if let Err(m) = compare(&f(c), want, &Tol::Exact) {
            panic!(
                "cutout/{function}/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

fn polygons(polys: Vec<Vec<[f64; 2]>>) -> Value {
    Value::Arr(
        polys
            .into_iter()
            .map(|p| {
                Value::Arr(
                    p.into_iter()
                        .map(|v| Value::Arr(vec![v[0].into(), v[1].into()]))
                        .collect(),
                )
            })
            .collect(),
    )
}

#[test]
fn holes_for_replays() {
    replay("holesFor", 400, |c| {
        let holes = holes_for(
            c.args[0].str(),
            c.args[1].num(),
            c.args[2].num(),
            &cfg_of(c),
        );
        Value::Arr(
            holes
                .into_iter()
                .map(|h| {
                    Value::Obj(vec![
                        ("a".into(), h.a.into()),
                        ("z0".into(), h.z0.into()),
                        ("z1".into(), h.z1.into()),
                    ])
                })
                .collect(),
        )
    });
}

#[test]
fn honeycomb_holes_replays() {
    replay("honeycombHoles", 60, |c| {
        polygons(honeycomb_holes(
            &nums(&c.args[0]),
            &nums(&c.args[1]),
            &nums(&c.args[2]),
            c.args[3].num(),
            c.args[4].num(),
            &cfg_of(c),
        ))
    });
}

#[test]
fn cut_wall_replays() {
    let mut cut = 0;
    replay("cutWall", 50, |c| {
        let st: Vec<Station> = c.args[0].arr().iter().map(station).collect();
        let w = &c.args[3];
        let wall = Wall {
            th: w.get("th").num(),
            tip: w.get("tip").num(),
            min_stations: w.get("minStations").num(),
        };
        let mut out = Vec::new();
        let ret = cut_wall(&st, &sections(&c.args[1]), &mut out, &wall, &cfg_of(c));
        cut += usize::from(ret);
        Value::Obj(vec![
            ("ret".into(), Value::Bool(ret)),
            (
                "tris".into(),
                Value::F64(out.iter().flatten().copied().collect()),
            ),
        ])
    });
    assert!(cut >= 20, "only {cut} recorded walls were actually cut");
}
