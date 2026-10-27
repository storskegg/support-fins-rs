//! Replay of `parity/corpus/fill/*` (recorded by `parity/oracle/fill.js` from web/fins/fill.js and,
//! through it, web/fins/rows.js): `fill_reach`, `bare_area` and `fill_coverage` on real Auto builds
//! of the stress models (every option of the pass: the slider's sparse / middle / dense, rows on and
//! off, Plate only, Separate walls, no tines, island starts, `fillRegions`) and `bareArea` as the 54
//! golden scenes' builds call it. Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use indexmap::IndexSet;
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt};
use sf_engine::fins::coverage::automatic_targets;
use sf_engine::fins::fill::*;
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::islands::Island;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::types::V4;
use sf_testkit::parity::resolve_args;

fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(o) => o.iter().find(|(key, _)| key == k).map(|(_, x)| x),
        _ => None,
    }
}

fn opt_num(v: &Value, k: &str) -> Option<f64> {
    match get(v, k) {
        Some(Value::Num(n)) => Some(*n),
        _ => None,
    }
}

fn opt_bool(v: &Value, k: &str) -> Option<bool> {
    match get(v, k) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn fin_records(v: &Value) -> Vec<FinRecord> {
    v.arr()
        .iter()
        .enumerate()
        .map(|(i, f)| FinRecord {
            height: 0.0,
            length: 0.0,
            tines: 0,
            rows: 0,
            stilt: 0.0,
            lean: 0.0,
            bearing: 0.0,
            site: None,
            id: i,
            kind: match f.get("kind").str() {
                "prop" => FinKind::Prop,
                "wedge" => FinKind::Wedge,
                _ => FinKind::Sway,
            },
            tri_ranges: vec![],
            line: match f.get("line") {
                Value::Arr(l) if l.is_empty() => vec![],
                l => pts3(l),
            },
            span: 0.0,
            short: false,
            fill: false,
            joined: false,
            braces: None,
            ribs: None,
        })
        .collect()
}

#[test]
fn fill_reach_replay() {
    for c in load_cases("fill", "fillReach") {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        check(
            "fillReach",
            &c,
            &Value::Num(fill_reach(a[0].plain().num(), &cfg)),
            &Tol::Exact,
        );
    }
}

fn bare_replay(file: &str) -> usize {
    let cases = load_cases("fill", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let rot = rot3(a[2].plain());
        let targets = automatic_targets(a[0].topo(), a[1].analysis(), &rot);
        let opts = FillOpts {
            coverage: opt_num(a[3].plain(), "coverage"),
            ..Default::default()
        };
        let (area, total) = bare_area(
            a[0].topo(),
            &targets,
            &rot,
            &opts,
            &fin_records(a[4].plain()),
            &cfg,
        );
        let actual = Value::Obj(vec![
            ("area".into(), Value::Num(area)),
            ("total".into(), Value::Num(total)),
        ]);
        check(file, c, &actual, &Tol::Exact);
    }
    cases.len()
}

#[test]
fn bare_area_replay() {
    assert!(bare_replay("bareArea") > 200);
}

#[test]
fn bare_area_in_the_golden_scenes() {
    assert!(bare_replay("bareArea.captured") >= 20);
}

fn islands_of(v: &Value) -> Vec<Island> {
    match v {
        Value::Arr(a) => a
            .iter()
            .map(|i| Island {
                z: i.get("z").num(),
                area: i.get("area").num(),
                points: pts3(i.get("points")),
                x: i.get("x").num(),
                y: i.get("y").num(),
            })
            .collect(),
        _ => vec![],
    }
}

fn v3_value(p: &[[f64; 3]]) -> Value {
    pts_value(p)
}

fn stats_value(s: &FillRun) -> Value {
    let n = |x: usize| Value::Num(x as f64);
    let mut o: Vec<(String, Value)> = vec![
        ("walls".into(), n(s.walls)),
        ("tries".into(), n(s.tries)),
        ("checks".into(), n(s.checks)),
        ("capped".into(), Value::Bool(s.capped)),
        (
            "refused".into(),
            Value::Obj(s.refused.iter().map(|(k, v)| (k.clone(), n(*v))).collect()),
        ),
        ("startArea".into(), Value::Num(s.start_area)),
        ("islands".into(), n(s.islands)),
    ];
    for (k, v) in [
        ("joined", s.joined),
        ("rows", s.rows),
        ("rowBuilds", s.row_builds),
        ("ribs", s.ribs),
        ("ties", s.ties),
    ] {
        if let Some(v) = v {
            o.push((k.into(), n(v)));
        }
    }
    if let Some(b) = s.islands_capped {
        o.push(("islandsCapped".into(), Value::Bool(b)));
    }
    if let Some(l) = s.rows_lost {
        o.push(("rowsLost".into(), Value::Num(l)));
    }
    // the recorder writes the keys sorted (JS adds them as events happen)
    o.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Obj(o)
}

fn prop_value(p: &FillProp) -> Value {
    let n = |x: usize| Value::Num(x as f64);
    let mut o: Vec<(String, Value)> = vec![
        ("line".into(), v3_value(&p.line)),
        (
            "triRanges".into(),
            Value::Arr(
                p.tri_ranges
                    .iter()
                    .map(|r| num_value(&[r[0] as f64, r[1] as f64]))
                    .collect(),
            ),
        ),
        ("height".into(), Value::Num(p.height)),
        ("span".into(), Value::Num(p.span)),
        ("tines".into(), n(p.tines)),
        ("partAttached".into(), Value::Bool(p.part_attached)),
        ("squat".into(), Value::Bool(p.squat)),
        ("fill".into(), Value::Bool(p.fill)),
        (
            "joined".into(),
            if p.joined {
                Value::Bool(true)
            } else {
                Value::Undefined
            },
        ),
    ];
    if let Some(b) = p.braces {
        o.push(("braces".into(), n(b)));
    }
    if let Some(t) = p.ties {
        o.push(("ties".into(), n(t)));
    }
    Value::Obj(o)
}

#[test]
fn fill_coverage_replay() {
    let cases = load_cases("fill", "fillCoverage");
    assert!(cases.len() > 200);
    let (mut walls, mut rows, mut ribs, mut ties, mut capped, mut islands) = (0, 0, 0, 0, 0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let rot = rot3(a[2].plain());
        let topo = a[0].topo();
        let targets = automatic_targets(topo, a[1].analysis(), &rot);
        let o = a[3].plain();
        let isl = islands_of(get(o, "islands").unwrap_or(&Value::Null));
        let opts = FillOpts {
            coverage: opt_num(o, "coverage"),
            long_walls: opt_bool(o, "longWalls"),
            fill_regions: match get(o, "fillRegions") {
                Some(Value::Arr(r)) => Some(
                    r.iter()
                        .map(|x| x.num() as usize)
                        .collect::<IndexSet<usize>>(),
                ),
                _ => None,
            },
            islands: &isl,
            plate_only: opt_bool(o, "plateOnly") == Some(true),
            separate_walls: opt_bool(o, "separateWalls") == Some(true),
            tines: opt_bool(o, "tines"),
            tine_density: opt_num(o, "tineDensity"),
            layer_height: opt_num(o, "layerHeight"),
        };
        let bv = a[4].plain();
        let mut built = FinsBuilt::empty(Seating {
            kind: SeatingKind::Face,
            span: 0.0,
            bed_area: 0.0,
        });
        built.fins = fin_records(bv.get("fins"));
        built.triangles = nums(bv.get("triangles"))
            .chunks(3)
            .map(|c| [c[0], c[1], c[2], 0.0])
            .collect::<Vec<V4>>();
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        let f = fill_coverage(topo, &targets, &rot, &opts, &built, &mut ctx);
        walls += !f.props.is_empty() as usize;
        rows += f.stats.rows.is_some_and(|r| r > 0) as usize;
        ribs += f.stats.ribs.is_some_and(|r| r > 0) as usize;
        ties += f.stats.ties.is_some_and(|r| r > 0) as usize;
        capped += f.stats.capped as usize;
        islands += (f.stats.islands > 0) as usize;
        let actual = Value::Obj(vec![
            (
                "tris".into(),
                Value::F64(
                    f.triangles
                        .iter()
                        .flat_map(|v| [v[0], v[1], v[2], if v[3] == 1.0 { 1.0 } else { 0.0 }])
                        .collect(),
                ),
            ),
            (
                "props".into(),
                Value::Arr(f.props.iter().map(prop_value).collect()),
            ),
            ("tines".into(), Value::Num(f.tines as f64)),
            (
                "unserved".into(),
                Value::Obj(vec![
                    ("area".into(), Value::Num(f.unserved.area)),
                    ("pts".into(), v3_value(&f.unserved.pts)),
                    ("islands".into(), Value::Num(f.unserved.islands as f64)),
                    ("islandPts".into(), v3_value(&f.unserved.island_pts)),
                ]),
            ),
            ("stats".into(), stats_value(&f.stats)),
        ]);
        check("fillCoverage", c, &actual, &Tol::Exact);
    }
    assert!(
        walls > 100 && rows > 30 && ribs > 20 && ties > 30 && capped > 0,
        "walls {walls} rows {rows} ribs {ribs} ties {ties} capped {capped} islands {islands}"
    );
}
