//! Port of upstream `tests/tails.test.js`: low TAILS. A tall wall runs on down the slope to the squat
//! floor (prop/clearance.js withLowTails); six side effects found by a sweep are pinned so they can't
//! creep back. The tail may only ADD coverage and grip, never take any away.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins_capturing};
use sf_engine::overhangs::{Topology, analyze};
use sf_engine::types::Rot3;
use sf_testkit::{load_model, rot_x, rot_y, tilted_block_topo};
use std::sync::Arc;

/// 3x3 col-major, as prototype/stress/run.js
fn mul(a: &Rot3, b: &Rot3) -> Rot3 {
    let mut m = [0.0; 9];
    for c in 0..3 {
        for r in 0..3 {
            for k in 0..3 {
                m[c * 3 + r] += a[k * 3 + r] * b[c * 3 + k];
            }
        }
    }
    m
}

struct Out {
    b: FinsBuilt,
    caps: Vec<sf_engine::prop::ctx::TineSeed>,
    low_tine: f64,
}

fn build(topo: &Arc<Topology>, rot: &Rot3, coverage: f64, downhill: Option<bool>) -> Out {
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        coverage: Some(coverage),
        downhill,
        ..Default::default()
    };
    let (b, caps) =
        build_fins_capturing(topo, &analyze(topo, 45.0, rot), rot, &o, &Config::default());
    let low_tine = caps.iter().map(|t| t.z).fold(f64::INFINITY, f64::min);
    Out { b, caps, low_tine }
}

fn cube35() -> Arc<Topology> {
    Arc::new(tilted_block_topo(
        -20.0, 20.0, -20.0, 20.0, -20.0, 20.0, 35.0,
    ))
}

// tails.test.js:27
#[test]
fn tails_the_comb_reaches_the_base_without_losing_tines_35deg_cube() {
    // Anchored at the lowest nub that grips, each wall keeps main's 11 and the lowest tine drops from
    // 2.69 (main) to ~1.25mm.
    let Out { b, low_tine, .. } = build(&cube35(), &I, 0.5, None);
    let walls = b.props.iter().filter(|p| !p.squat).count();
    assert!(
        b.tines >= 11 * walls,
        "tail cost tines: {} on {walls} walls (< 11 each)",
        b.tines
    );
    assert!(
        low_tine < 1.5,
        "lowest tine {low_tine:.2}mm -- the comb didn't reach the base"
    );
}

// :37
#[test]
fn tines_one_evenly_spaced_comb_from_the_bottom_none_jammed_at_the_top_35deg_cube() {
    // Every gap must be a real comb step (dense 2mm or edge-biased 4mm), and the top end keeps
    // PROP.tineTopClear bare.
    let Out { b, caps, .. } = build(&cube35(), &I, 0.5, None);
    let walls: Vec<_> = b
        .props
        .iter()
        .filter(|p| !p.squat && p.line.len() > 1)
        .collect();
    assert!(!walls.is_empty(), "no walls to test");
    let top_clear = Config::default().prop.tine_top_clear;
    for w in walls {
        let l = &w.line;
        let last = *l.last().unwrap();
        let (lo, hi) = if l[0][2] <= last[2] {
            (l[0], last)
        } else {
            (last, l[0])
        };
        let len = (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
        let (ux, uy) = ((hi[0] - lo[0]) / len, (hi[1] - lo[1]) / len);
        let mut along: Vec<f64> = caps
            .iter()
            .filter(|t| ((t.x - lo[0]) * -uy + (t.y - lo[1]) * ux).abs() < 1.5)
            .map(|t| (t.x - lo[0]) * ux + (t.y - lo[1]) * uy)
            .collect();
        along.sort_by(|a, c| a.partial_cmp(c).unwrap());
        assert!(along.len() >= 3, "wall got {} tines", along.len());
        for i in 1..along.len() {
            let g = along[i] - along[i - 1];
            assert!(
                (g - 2.0).abs() < 0.05 || (g - 4.0).abs() < 0.05,
                "uneven comb: gap {g:.2}mm between tines {} and {i}",
                i - 1
            );
        }
        assert!(
            len - along.last().unwrap() >= top_clear - 0.05,
            "tine jammed {:.2}mm from the wall's top end",
            len - along.last().unwrap()
        );
    }
}

// :70
#[test]
fn tails_a_tail_tip_does_not_count_as_a_wall_under_a_neighbouring_face_wedge_kept() {
    // The same cube rotated a further 45deg: propServesPatch read the tail tip as a wall under the
    // steep face, dropping its wedge (5 -> 0 tines, no brace beyond the walls).
    let Out { b, .. } = build(&cube35(), &rot_x(45.0), 0.5, None);
    assert!(
        b.fins.len() > b.props.len(),
        "wedge dropped: {} fins for {} walls",
        b.fins.len(),
        b.props.len()
    );
    assert!(b.tines >= 3, "wedge tines lost: {}", b.tines); // main: 3
}

// :79
#[test]
fn tails_the_squat_pass_only_yields_stations_the_built_wall_covers_torus_x45() {
    // The squat ledge at x -11.7..-6.7 sits next to a BLOCKED tall station, not a built wall.
    // Downhill walls off: they take this ledge over on main's torus X45.
    let topo = Arc::new(load_model("torus"));
    let Out { b, .. } = build(&topo, &rot_x(45.0), 0.5, Some(false));
    let squat = b
        .props
        .iter()
        .filter(|p| p.squat && p.line[0][0] < -6.0)
        .count();
    assert!(squat >= 1, "torus lost its squat wall on the low ledge");
}

// :89
#[test]
fn tails_a_tail_never_promotes_a_stub_past_min_span_tube_x25_sparse() {
    // Counting the tail toward minSpan built a short wall that then "served" a face and dropped its
    // wedge: 28 (main) -> 20 tines. minSpan measures the body only.
    let topo = Arc::new(load_model("tube"));
    let Out { b, .. } = build(&topo, &rot_x(25.0), 0.0, None);
    assert!(b.tines >= 28, "tube lost its wedge tines: {}", b.tines);
}

// :96
#[test]
fn tails_body_membership_is_judged_before_settling_sphere_x45y30() {
    // The wall's lowest body station sat at 1.51mm; settling beside its new tail lowered it a hair
    // under 1.5, re-read as tail, and the body fell under minSpan -- the sphere's only wall was dropped.
    let topo = Arc::new(load_model("sphere"));
    let Out { b, .. } = build(&topo, &mul(&rot_y(30.0), &rot_x(45.0)), 0.5, None);
    assert!(
        !b.props.is_empty() && b.tines >= 4,
        "sphere wall dropped: {} walls, {} tines",
        b.props.len(),
        b.tines
    ); // main: 4
}

// :104
#[test]
fn tines_an_exact_nearest_face_tie_tries_every_tied_face_staircase_y35() {
    // A step's underside and the side face were EXACTLY equidistant; which one won hung on 1e-15 of
    // float noise. Every tied face is now tried. (main: 33 on this pose.)
    let topo = Arc::new(load_model("staircase"));
    let Out { b, .. } = build(&topo, &rot_y(35.0), 0.5, None);
    assert!(
        b.tines >= 33,
        "staircase lost tines at an inside corner: {}",
        b.tines
    );
}
