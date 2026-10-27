//! Replay of `parity/corpus/circular/*` (recorded by `parity/oracle/circular.js` from
//! web/prop/circular.js). Exact: every output is arithmetic on the inputs (plus `surfaceZAt`).
//! `circular_tracks` calls `sweep::foot_for` (track B), so its replays are ignored until that
//! lands in `main` (run them with `--ignored`).
mod common;
use ::parity::{Case, Tol, Value, load_cases};
use common::*;
use sf_engine::config::Config;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::circular::*;
use sf_engine::tris::Tris;
use sf_testkit::parity::{Resolved, resolve_args};

fn pair(p: [f64; 2]) -> Value {
    Value::Arr(vec![p[0].into(), p[1].into()])
}

fn frame_value(f: &CircularFrame) -> Value {
    let mut o = vec![
        ("centre".to_string(), pair(f.centre)),
        ("innerRadius".to_string(), f.inner_radius.into()),
        ("outerRadius".to_string(), f.outer_radius.into()),
        (
            "radialProfile".to_string(),
            Value::Arr(vec![pair(f.radial_profile[0]), pair(f.radial_profile[1])]),
        ),
        ("angularExtent".to_string(), pair(f.angular_extent)),
        ("phase".to_string(), f.phase.into()),
        ("axis".to_string(), pair(f.axis)),
    ];
    if let Some(s) = &f.surrounding {
        o.push((
            "surrounding".to_string(),
            Value::Obj(vec![
                ("ceiling".into(), s.ceiling.into()),
                ("reach".into(), s.reach.into()),
                ("area".into(), s.area.into()),
                ("centroid".into(), pair(s.centroid)),
                ("phase".into(), s.phase.map_or(Value::Null, Value::from)),
            ]),
        ));
    }
    Value::Obj(o)
}

fn frame_of(v: &Value) -> CircularFrame {
    let two = |v: &Value| -> [f64; 2] { nums(v).try_into().expect("2 numbers") };
    let prof = v.get("radialProfile").arr();
    CircularFrame {
        centre: two(v.get("centre")),
        inner_radius: v.get("innerRadius").num(),
        outer_radius: v.get("outerRadius").num(),
        radial_profile: [two(&prof[0]), two(&prof[1])],
        angular_extent: two(v.get("angularExtent")),
        phase: v.get("phase").num(),
        axis: two(v.get("axis")),
        surrounding: None,
    }
}

fn map_value(m: &indexmap::IndexMap<usize, CircularFrame>) -> Value {
    Value::Map(
        m.iter()
            .map(|(k, f)| (Value::Num(*k as f64), frame_value(f)))
            .collect(),
    )
}

fn opt_frame(f: Option<CircularFrame>) -> Value {
    f.as_ref().map_or(Value::Null, frame_value)
}

fn replay(file: &str, min: usize, run: impl Fn(&Case) -> Value) {
    let cases = load_cases("circular", file);
    assert!(cases.len() >= min, "{file}: {}", cases.len());
    for c in &cases {
        check(file, c, &run(c), &Tol::Exact);
    }
}

fn footprint(c: &Case) -> Value {
    opt_frame(circular_footprint(&nums(&c.args[0])))
}

#[test]
fn circular_footprint_direct() {
    replay("circularFootprint", 30, footprint);
}
#[test]
fn circular_footprint_captured() {
    replay("circularFootprint.captured", 10, footprint);
}
#[test]
fn circular_footprint_soup() {
    replay("circularFootprint.soup", 40, footprint);
}

fn regions_ref(c: &Case) -> Value {
    let a = resolve_args(&c.args);
    let cfg = cfg_of(&a);
    let (res, _) = analysis_full(&a[1]);
    map_value(&circular_regions(
        a[0].topo(),
        &res,
        &rot3(a[2].plain()),
        &CircularCache::new(),
        &cfg,
    ))
}

#[test]
fn circular_regions_direct() {
    replay("circularRegions", 30, regions_ref);
}
#[test]
fn circular_regions_captured() {
    replay("circularRegions.captured", 40, regions_ref);
}

#[test]
fn circular_regions_soup() {
    let cfg = Config::default();
    replay("circularRegions.soup", 40, |c| {
        let topo = build_topology(nums(&c.args[0]));
        let rot = rot3(&c.args[2]);
        let res = analyze(&topo, c.args[1].num(), &rot);
        map_value(&circular_regions(
            &topo,
            &res,
            &rot,
            &CircularCache::new(),
            &cfg,
        ))
    });
}

fn tracks_run(c: &Case) -> Value {
    let a = resolve_args(&c.args);
    let cfg = cfg_of(&a);
    let frame = frame_of(a[0].plain());
    let tris = Tris::new(nums(a[1].plain()));
    let out = circular_tracks(&frame, &tris, a[2].plain().num(), a[3].plain().num(), &cfg);
    out.map_or(Value::Null, |l| {
        Value::Arr(l.iter().map(|t| pts_value(t)).collect())
    })
}

#[test]
#[ignore = "needs sweep::foot_for (M2 track B)"]
fn circular_tracks_soup() {
    replay("circularTracks.soup", 30, tracks_run);
}
