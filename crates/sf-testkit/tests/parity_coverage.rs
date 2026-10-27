//! Replay of `parity/corpus/coverage/*` (recorded by `parity/oracle/coverage.js` from
//! web/fins/coverage.js). Exact. `classify` and `automaticTargets` get their topology from the
//! `{"$topo": id}` reference (or, for the synthetic parts, build it from the recorded soup).
mod common;
use ::parity::{Case, Tol, Value, load_cases};
use common::*;
use sf_engine::fins::coverage::*;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::tris::Tris;
use sf_engine::types::Rot3;
use sf_testkit::parity::{Resolved, resolve_args};

fn replay(file: &str, min: usize, run: impl Fn(&Case) -> Value) {
    let cases = load_cases("coverage", file);
    assert!(cases.len() >= min, "{file}: {}", cases.len());
    for c in &cases {
        check(file, c, &run(c), &Tol::Exact);
    }
}

fn bools(v: &[bool]) -> Value {
    Value::Arr(v.iter().map(|&b| Value::Bool(b)).collect())
}

#[test]
fn reach_of_applies_the_closure() {
    replay("reachOf", 6, |c| {
        let tops = pts3(&c.args[0]);
        let r = c.args[1].num();
        let queries = pts3(&c.args[2]);
        let adds = pts3(&c.args[3]);
        let mut near = reach_of(&tops, r);
        let before: Vec<bool> = queries
            .iter()
            .map(|q| near.near(q[0], q[1], q[2]))
            .collect();
        for p in &adds {
            near.add(*p);
        }
        let after: Vec<bool> = queries
            .iter()
            .map(|q| near.near(q[0], q[1], q[2]))
            .collect();
        Value::Obj(vec![
            ("before".into(), bools(&before)),
            ("after".into(), bools(&after)),
        ])
    });
}

fn classify_ref(c: &Case) -> Value {
    let a = resolve_args(&c.args);
    let (res, _) = analysis_full(&a[1]);
    Value::U8(classify(a[0].topo(), &res, &rot3(a[2].plain())))
}

#[test]
fn classify_direct() {
    replay("classify", 150, classify_ref);
}
#[test]
fn classify_captured() {
    // a captured result may already carry `coverageClasses`: JS returns that, recomputing from
    // the same overhang flags gives the same classes
    replay("classify.captured", 50, classify_ref);
}

fn soup_part(c: &Case) -> (Topology, Analysis, Rot3) {
    let topo = build_topology(nums(&c.args[0]));
    let rot = rot3(&c.args[2]);
    let res = analyze(&topo, c.args[1].num(), &rot);
    (topo, res, rot)
}

#[test]
fn classify_soup() {
    replay("classify.soup", 50, |c| {
        let (topo, res, rot) = soup_part(c);
        Value::U8(classify(&topo, &res, &rot))
    });
}

fn num_arr(v: &[f64]) -> Value {
    Value::Arr(v.iter().map(|&x| x.into()).collect())
}

fn xyz_value(p: sf_engine::types::Xyz) -> Value {
    Value::Obj(vec![
        ("x".into(), p.x.into()),
        ("y".into(), p.y.into()),
        ("z".into(), p.z.into()),
    ])
}

/// The augmented result in the key order JS builds it.
fn targets_value(t: &Targets) -> Value {
    let a = &t.analysis;
    let regions = Value::Arr(
        a.regions
            .iter()
            .map(|g| {
                Value::Obj(vec![
                    (
                        "faces".into(),
                        Value::Arr(g.faces.iter().map(|&f| (f as f64).into()).collect()),
                    ),
                    ("area".into(), g.area.into()),
                ])
            })
            .collect(),
    );
    Value::Obj(vec![
        ("over".into(), Value::U8(a.over.clone())),
        ("kept".into(), Value::U8(a.kept.clone())),
        ("onBed".into(), Value::U8(a.on_bed.clone())),
        ("regions".into(), regions),
        ("rawRegionCount".into(), (a.raw_region_count as f64).into()),
        ("overArea".into(), a.over_area.into()),
        ("bedArea".into(), a.bed_area.into()),
        ("overFaceCount".into(), (a.over_face_count as f64).into()),
        ("offset".into(), xyz_value(a.offset)),
        ("size".into(), xyz_value(a.size)),
        (
            "coverageClasses".into(),
            Value::U8(t.coverage_classes.clone()),
        ),
        (
            "excludedSupportFaces".into(),
            Value::U8(t.excluded_support_faces.clone()),
        ),
        (
            "excludedSupportTris".into(),
            num_arr(&t.excluded_support_tris.v),
        ),
        (
            "excludedSupportVolumes".into(),
            pts_value(&t.excluded_support_volumes),
        ),
        (
            "smallHoleRegions".into(),
            (t.small_hole_regions as f64).into(),
        ),
    ])
}

fn targets_ref(c: &Case) -> Value {
    let a = resolve_args(&c.args);
    let (res, _) = analysis_full(&a[1]);
    targets_value(&automatic_targets(a[0].topo(), &res, &rot3(a[2].plain())))
}

#[test]
fn automatic_targets_direct() {
    replay("automaticTargets", 150, targets_ref);
}
#[test]
fn automatic_targets_captured() {
    replay("automaticTargets.captured", 30, targets_ref);
}
#[test]
fn automatic_targets_soup() {
    replay("automaticTargets.soup", 50, |c| {
        let (topo, res, rot) = soup_part(c);
        targets_value(&automatic_targets(&topo, &res, &rot))
    });
}

/// `touchesSmallHole` reads only `result.excludedSupportTris`: a stub `Targets` carries it.
fn stub(r: &Resolved) -> Targets {
    let tris = match r.plain() {
        Value::Obj(o) => o
            .iter()
            .find(|(k, _)| k == "excludedSupportTris")
            .map(|(_, v)| nums(v))
            .unwrap_or_default(),
        other => panic!("result argument: {other:?}"),
    };
    let analysis = Analysis {
        over: vec![],
        kept: vec![],
        on_bed: vec![],
        regions: vec![],
        raw_region_count: 0,
        over_area: 0.0,
        bed_area: 0.0,
        over_face_count: 0,
        offset: Default::default(),
        size: Default::default(),
    };
    Targets {
        analysis,
        coverage_classes: vec![],
        excluded_support_faces: vec![],
        excluded_support_tris: Tris::new(tris),
        excluded_support_volumes: vec![],
        small_hole_regions: 0,
    }
}

fn touches(c: &Case) -> Value {
    let a = resolve_args(&c.args);
    Value::Bool(touches_small_hole(&stub(&a[0]), &pts3(a[1].plain())))
}

#[test]
fn touches_small_hole_direct() {
    replay("touchesSmallHole", 3, touches);
}
#[test]
fn touches_small_hole_soup() {
    replay("touchesSmallHole.soup", 30, touches);
}
