//! Port of upstream `tests/curved.test.js`: curved walls (issue #25), the "Curved walls" checkbox
//! (web/prop/curved.js). With it ticked, downhill walls are tried under every region and a region
//! whose downhill walls bend past CURVED.minBend always swaps to them; in Auto the fill pass then
//! takes the overhang the curves left bare. Unticked, the build is untouched. (`globalThis.
//! __TINECAP` is `build_fins_capturing`'s second value; it lives in sf-io's tests for `read_stl`.)
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins_capturing};
use sf_engine::fins::coverage::classify;
use sf_engine::fins::downhill::held_area;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::prop::build::PropRecord;
use sf_engine::prop::curved::line_bend;
use sf_engine::types::{IDENTITY3, Rot3};
use sf_io::stl::read_stl;
use sf_testkit::upstream_root;

fn example(name: &str) -> Arc<Topology> {
    let bytes = std::fs::read(upstream_root().join(format!("tests/fixtures/{name}.stl"))).unwrap();
    Arc::new(build_topology(read_stl(&bytes).unwrap()))
}

/// This file's own `rotX` (note the sign: not tests/_util.js's).
fn rot_x(d: f64) -> Rot3 {
    let (c, s) = (
        (d * std::f64::consts::PI / 180.0).cos(),
        (d * std::f64::consts::PI / 180.0).sin(),
    );
    [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c]
}

struct B {
    b: FinsBuilt,
    low: f64,
    held: f64,
}

fn build(topo: &Arc<Topology>, f: impl FnOnce(&mut FinsOpts), rot: &Rot3) -> B {
    let res = analyze(topo, 45.0, rot);
    let mut o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        ..Default::default()
    };
    f(&mut o);
    let cfg = Config::default();
    let (b, caps) = build_fins_capturing(topo, &res, rot, &o, &cfg);
    let low = caps
        .iter()
        .fold(f64::INFINITY, |l, t| if t.z < l { t.z } else { l });
    let held = held_area(topo, &res, rot, &b, &classify(topo, &res, rot), &cfg);
    B { b, low, held }
}

fn same(a: &FinsBuilt, b: &FinsBuilt) -> bool {
    a.triangles == b.triangles
}

fn bent(b: &FinsBuilt) -> Vec<&PropRecord> {
    let cfg = Config::default();
    b.props
        .iter()
        .filter(|q| q.downhill == Some(true) && line_bend(&q.line, &cfg) > cfg.curved.min_bend)
        .collect()
}

// curved.test.js:34
#[test]
fn curved_a_straight_line_has_no_bend_a_quarter_arc_a_clear_one_a_wobbling_start_none() {
    let cfg = Config::default();
    let straight: Vec<[f64; 3]> = (0..20)
        .map(|i| [i as f64, 0.5 * i as f64, 10.0 - i as f64])
        .collect();
    assert!(
        line_bend(&straight, &cfg) < 1e-6,
        "straight line bends {}",
        line_bend(&straight, &cfg)
    );
    let r = 20.0;
    let arc: Vec<[f64; 3]> = (0..32)
        .map(|i| {
            let a = (i as f64 / 31.0) * (std::f64::consts::PI / 2.0);
            [r * a.cos(), r * a.sin(), 10.0]
        })
        .collect();
    let b = line_bend(&arc, &cfg);
    // the trimmed spread reads a 90 deg arc short of 90, but far past minBend
    assert!(
        b > 2.0 * cfg.curved.min_bend && b < 95.0,
        "quarter arc read as {b:.1} deg"
    );
    assert!(
        line_bend(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], &cfg) == 0.0,
        "a two-station line must read as straight"
    );
    // a straight wall whose first station sits 1 mm off the line (the climb's start)
    let wobble: Vec<[f64; 3]> = straight
        .iter()
        .enumerate()
        .map(|(i, p)| if i == 0 { [p[0], p[1] + 1.0, p[2]] } else { *p })
        .collect();
    assert!(
        line_bend(&wobble, &cfg) < cfg.curved.min_bend,
        "a wobbling start read as a {:.1} deg bend",
        line_bend(&wobble, &cfg)
    );
    // stations needn't be 1 mm apart: the same arc at half the stations reads the same
    let sparse: Vec<[f64; 3]> = arc
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, p)| *p)
        .collect();
    assert!(
        (line_bend(&sparse, &cfg) - b).abs() < 10.0,
        "sparse arc {:.1} vs {b:.1}",
        line_bend(&sparse, &cfg)
    );
}

// curved.test.js:53
#[test]
fn curved_the_spiral_ramp_gets_walls_that_follow_its_curve_holding_as_much() {
    // A ramp that winds down to the plate: its downhill walls arc round in plan. Unticked, the arcs
    // weigh 1.5x the straight walls and lose the race, so none shows.
    let topo = example("issue25_spiral");
    let off = build(&topo, |_| {}, &IDENTITY3);
    let on = build(&topo, |o| o.curved_walls = Some(true), &IDENTITY3);
    assert!(
        bent(&off.b).is_empty(),
        "unticked, a bending wall won anyway"
    );
    assert!(
        bent(&on.b).len() >= 2,
        "ticked, {} bending walls",
        bent(&on.b).len()
    );
    assert!(
        on.held >= off.held - 1e-6,
        "held {:.1} -> {:.1} mm2",
        off.held,
        on.held
    );
    assert!(
        on.low <= off.low + 0.1,
        "lowest tine rose {} -> {}",
        off.low,
        on.low
    );
}

// curved.test.js:65
#[test]
fn curved_unticked_is_the_build_without_the_option_ticked_changes_nothing_where_no_wall_bends() {
    for name in ["issue25_spiral", "issue25_curve"] {
        let topo = example(name);
        let base = build(&topo, |_| {}, &IDENTITY3).b;
        assert!(
            same(
                &base,
                &build(&topo, |o| o.curved_walls = Some(false), &IDENTITY3).b
            ),
            "{name}: curvedWalls:false changed the build"
        );
    }
    // the #25 part's downhill walls run straight: nothing for the box to do
    let topo = example("issue25_curve");
    assert!(
        same(
            &build(&topo, |_| {}, &IDENTITY3).b,
            &build(&topo, |o| o.curved_walls = Some(true), &IDENTITY3).b
        ),
        "issue25_curve: ticked changed a straight-walled build"
    );
}

// curved.test.js:76
#[test]
fn curved_forced_a_curve_that_holds_less_still_shows_and_auto_fills_what_it_left_bare() {
    // The torus tilted X30: its one bending downhill wall held less than the straight walls, so a
    // hold-as-much race never picked it. Forced, it wins; the fill pass then walls the overhang it
    // missed, so the part ends up held at least as well.
    let topo = example("torus_flat");
    let off = build(&topo, |_| {}, &rot_x(30.0));
    let on = build(&topo, |o| o.curved_walls = Some(true), &rot_x(30.0));
    assert!(
        bent(&off.b).is_empty(),
        "unticked, a bending wall won anyway"
    );
    assert!(!bent(&on.b).is_empty(), "ticked, the curve did not show");
    let walls = on.b.curved_fill.map(|c| c.walls);
    assert!(
        walls.is_some_and(|w| w >= 1),
        "ticked, the fill added {walls:?} walls"
    );
    assert!(on.b.fill.is_none(), "Auto reported a Full coverage fill");
    assert!(
        on.held >= off.held - 1e-6,
        "held {:.1} -> {:.1} mm2",
        off.held,
        on.held
    );
    assert!(
        on.low <= off.low + 0.1,
        "lowest tine rose {} -> {}",
        off.low,
        on.low
    );
}

// curved.test.js:91
#[test]
fn curved_in_full_coverage_the_curves_stay_only_within_curved_full_slack_of_the_straight_build() {
    // The spiral's arcs hold 0.8% less than Full's rows there: kept. The torus X30's curve grips
    // higher than the straight build: dropped, and the build says so.
    let full = |o: &mut FinsOpts| o.mode = Some("full".into());
    let slack = Config::default().curved.full_slack;
    let spiral = example("issue25_spiral");
    let s_off = build(&spiral, full, &IDENTITY3);
    let s_on = build(
        &spiral,
        |o| {
            full(o);
            o.curved_walls = Some(true);
        },
        &IDENTITY3,
    );
    assert!(
        bent(&s_on.b).len() >= 2 && !s_on.b.curved_dropped,
        "spiral: Full dropped curves within the slack"
    );
    assert!(
        s_on.held >= (1.0 - slack) * s_off.held - 1e-6,
        "spiral: held {:.1} -> {:.1} mm2",
        s_off.held,
        s_on.held
    );
    let torus = example("torus_flat");
    let t_off = build(&torus, full, &rot_x(30.0));
    let t_on = build(
        &torus,
        |o| {
            full(o);
            o.curved_walls = Some(true);
        },
        &rot_x(30.0),
    );
    assert!(
        t_on.b.curved_dropped && bent(&t_on.b).is_empty(),
        "torus X30: Full kept a curve that grips higher"
    );
    assert!(
        (t_on.held - t_off.held).abs() < 1e-6,
        "torus X30: dropped build held {} vs {}",
        t_on.held,
        t_off.held
    );
}
