//! Port of upstream `tests/coverage.test.js`. The "Wide-face coverage" slider must actually change
//! how densely a broad overhang is lined. It was wired to a dead code path once, so Auto ignored it
//! entirely -- coverage 0 and 1 gave byte-identical output. These pin that the slider moves density
//! monotonically, and only ever ADDS support (denser is never fewer fins, so it can't strand an
//! overhang).
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::build_fins;
use sf_engine::overhangs::analyze;
use sf_testkit::tilted_block_topo;
use std::sync::Arc;

// A broad tilted plate: wide across the overhang so it takes a ROW of fins, which is the only place
// coverage bites (a narrow part keeps its bracing).
fn plate_build(coverage: f64) -> sf_engine::fins::build::FinsBuilt {
    let topo = Arc::new(tilted_block_topo(-40.0, 40.0, -45.0, 45.0, -6.0, 6.0, 55.0));
    let res = analyze(&topo, 60.0, &I);
    build_fins(
        &topo,
        &res,
        &I,
        &auto_opts(Some(coverage)),
        &Config::default(),
    )
}

fn fin_count(coverage: f64) -> usize {
    plate_build(coverage).fins.len()
}

// coverage.test.js:25
#[test]
fn coverage_dense_places_strictly_more_fins_than_sparse() {
    let sparse = fin_count(0.0);
    let dense = fin_count(1.0);
    assert!(
        dense > sparse,
        "slider is inert: coverage 0 -> {sparse} fins, coverage 1 -> {dense}"
    );
}

// coverage.test.js:31
#[test]
fn coverage_density_is_monotonic_and_never_drops_below_sparse() {
    let (c0, c05, c1) = (fin_count(0.0), fin_count(0.5), fin_count(1.0));
    assert!(
        c05 >= c0 && c1 >= c05,
        "not monotonic: {c0} -> {c05} -> {c1}"
    );
}

// coverage.test.js:41
#[test]
fn coverage_the_neutral_default_never_cries_sag_on_a_wide_plate() {
    // Regression: round(vExt/cap) lands a hair over the cap even at coverage 0.5, so an
    // achieved-spacing test warned on the DEFAULT. The warning must fire only when the user
    // actually dragged BELOW centre, not from rounding at the neutral pitch.
    assert!(
        !plate_build(0.5).sag_risk,
        "sag warning fired at the neutral default (0.5) -- false alarm"
    );
    assert!(
        !plate_build(1.0).sag_risk,
        "sag warning fired at the densest setting"
    );
}

// coverage.test.js:49
#[test]
fn coverage_dragging_below_centre_does_warn_on_a_wide_multi_row_plate() {
    assert!(
        plate_build(0.0).sag_risk,
        "no sag warning at the sparsest setting on a wide plate"
    );
}

// Matthew's screenshot (cube-tines-35deg-RAW-tilted.stl): the walls ran the full WIDTH of the face
// but stopped ~3mm up the slope from the part's bottom edge, leaving the lowest band -- where the
// overhang meets the pad -- in air. Cause: stations sit 1mm apart and the wall began at the first one
// >= minHeight (1.5mm), while the squat pass wants 3 stations/4mm of low band -- a slope running INTO
// the bed has ~1 low station, so nobody built it. Pin the wall's low end: every wall reaches down to
// the squat floor (top ~minHeightSquat, overhang <1mm up), not the old 1.9mm, and the geometry
// comes within ~1.2mm of the bottom edge.
// coverage.test.js:60
#[test]
fn coverage_walls_run_down_the_slope_to_the_part_bottom_edge_not_stop_3mm_short() {
    let cfg = Config::default();
    let topo = Arc::new(tilted_block_topo(
        -20.0, 20.0, -20.0, 20.0, -20.0, 20.0, 35.0,
    ));
    let (_, b) = build(&topo, &I, &auto_opts(Some(0.5)));
    // The bottom edge is where the overhang meets z = 0: 35deg tilt about X puts it at
    // y = -20*cos35 + 20*sin35 (the cube's (-20,+20) corner in y,z), ~-4.91.
    let a = (35.0 * std::f64::consts::PI) / 180.0;
    let edge_y = -20.0 * a.cos() + 20.0 * a.sin();
    let walls: Vec<_> = b.props.iter().filter(|p| !p.squat).collect();
    assert!(
        walls.len() >= 3,
        "expected a row of walls, got {}",
        walls.len()
    );
    for p in walls {
        let low = p
            .line
            .iter()
            .copied()
            .reduce(|m, q| if q[2] < m[2] { q } else { m })
            .unwrap();
        let low_top = low[2]; // props[].line is the wall top (gap already off)
        assert!(
            low_top <= cfg.prop.min_height_squat + 0.15,
            "wall at x={:.1} stops with its top {:.2}mm up -- should run down to ~{}mm",
            low[0],
            low_top,
            cfg.prop.min_height_squat
        );
        assert!(
            low[1] - edge_y < 1.4,
            "wall at x={:.1} ends {:.2}mm up-slope of the bottom edge",
            low[0],
            low[1] - edge_y
        );
    }
}
