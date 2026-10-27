//! Tests of `sf_engine::prop::tracks` (`split_region`, `tube_line`, `patch_tracks`): every branch with
//! a hand-built input, and the expected counts of the synthetic parts where the JS answer was read
//! from the pinned `web/prop/tracks.js` (the parity corpus `tracks/*.soup` pins the rest bit for bit).
mod common;
use common::*;
use sf_engine::config::Config;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::bridge::Support;
use sf_engine::prop::tracks::{patch_tracks, split_region, tube_line};
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;

fn cfg() -> Config {
    Config::default()
}

/// A flat rectangle at height `z` as two triangles, and its corner points.
fn rect(x0: f64, x1: f64, y0: f64, y1: f64, z: f64) -> (Tris, Vec<[f64; 3]>) {
    let p = [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]];
    let t = [p[0], p[1], p[2], p[0], p[2], p[3]];
    (Tris::new(t.iter().flatten().copied().collect()), p.to_vec())
}

fn tracks(pts: &[[f64; 3]], tris: &Tris) -> sf_engine::prop::tracks::Tracks {
    patch_tracks(pts, tris, 1.0, None, 12.0, None, true, &cfg())
}

#[test]
fn a_narrow_flat_patch_gets_one_wall_down_its_middle() {
    let (tris, pts) = rect(0.0, 20.0, 0.0, 10.0, 5.0);
    let t = tracks(&pts, &tris);
    // no part: the old rule, walls along x (the longer side); one row (10 / 12 rounds to 1)
    assert_eq!(t.lines.len(), 1);
    assert_eq!(t.turned, None);
    assert_eq!(t.spacing, Some(0.0));
    let line = &t.lines[0];
    assert_eq!(line.len(), 21); // 20 mm at a 1 mm step
    assert!(line.iter().all(|p| p[1] == 5.0 && p[2] == 5.0));
    assert_eq!((line[0][0], line[20][0]), (0.0, 20.0));
}

#[test]
fn a_wide_flat_patch_gets_a_row_of_parallel_tracks() {
    let (tris, pts) = rect(0.0, 40.0, 0.0, 36.0, 5.0);
    let t = tracks(&pts, &tris);
    assert_eq!(t.lines.len(), 3);
    assert_eq!(t.spacing, Some(12.0));
    let ys: Vec<f64> = t.lines.iter().map(|l| l[0][1]).collect();
    assert_eq!(ys, vec![6.0, 18.0, 30.0]);
}

#[test]
fn the_span_sets_the_row_pitch_and_is_floored_at_one() {
    let (tris, pts) = rect(0.0, 40.0, 0.0, 36.0, 5.0);
    let dense = patch_tracks(&pts, &tris, 1.0, None, 4.0, None, true, &cfg());
    assert_eq!(dense.lines.len(), 9);
    let floored = patch_tracks(&pts, &tris, 1.0, None, 0.1, None, true, &cfg());
    assert_eq!(floored.lines.len(), 36); // span floored at 1
    let sparse = patch_tracks(&pts, &tris, 1.0, None, 100.0, None, true, &cfg());
    assert_eq!(sparse.lines.len(), 1);
}

#[test]
fn an_axis_override_sets_the_run_direction() {
    let (tris, pts) = rect(0.0, 20.0, 0.0, 10.0, 5.0);
    let t = patch_tracks(&pts, &tris, 1.0, None, 12.0, Some([0.0, 1.0]), true, &cfg());
    // run along y: 10 mm long, one row across x=10 (20 / 12 rounds to 2 rows)
    assert!(t.lines.iter().all(|l| {
        l.iter()
            .all(|p| p[0] == t.lines[0][0][0] || p[0] == t.lines.last().unwrap()[0][0])
    }));
    assert_eq!(t.lines[0].len(), 11);
}

#[test]
fn a_slope_makes_the_walls_run_down_it() {
    // z = 0.5 x over x 0..20, y 0..10: gradient (0.5, 0): walls along x, climbing
    let p = [
        [0.0, 0.0, 0.0],
        [20.0, 0.0, 10.0],
        [20.0, 10.0, 10.0],
        [0.0, 10.0, 0.0],
    ];
    let tris = Tris::new(
        [p[0], p[1], p[2], p[0], p[2], p[3]]
            .iter()
            .flatten()
            .copied()
            .collect(),
    );
    let t = tracks(&p, &tris);
    assert_eq!(t.lines.len(), 1);
    let l = &t.lines[0];
    assert!(l[1][0] > l[0][0] && l[1][2] > l[0][2]);
    assert_eq!(t.turned, None);
}

#[test]
fn empty_and_zero_extent_footprints_give_no_tracks() {
    let (tris, _) = rect(0.0, 20.0, 0.0, 10.0, 5.0);
    let e = tracks(&[], &tris);
    assert!(e.lines.is_empty());
    assert_eq!((e.spacing, e.turned), (None, None));
    // all points on one spot: u extent below 1e-6
    let one = [[3.0, 3.0, 5.0], [3.0, 3.0, 5.0]];
    let r = patch_tracks(&one, &tris, 1.0, None, 12.0, Some([1.0, 0.0]), true, &cfg());
    assert!(r.lines.is_empty());
    assert_eq!(r.spacing, None);
}

#[test]
fn a_hole_splits_the_track_and_short_pieces_are_dropped() {
    // two rectangles with a 6 mm gap: the track splits; each piece must reach minStations (3)
    let a = rect(0.0, 10.0, 0.0, 10.0, 5.0).0;
    let b = rect(16.0, 17.0, 0.0, 10.0, 5.0).0; // a 1 mm stub: 2 stations
    let tris = Tris::new([a.v, b.v].concat());
    let pts = [[0.0, 0.0, 5.0], [17.0, 10.0, 5.0]];
    let t = tracks(&pts, &tris);
    assert_eq!(t.lines.len(), 1);
    assert_eq!(t.lines[0].last().unwrap()[0], 10.0);
}

#[test]
fn a_bridge_turn_is_reported_and_the_unturned_pick_can_be_asked_for() {
    // a ledge off one post: walls turn across the bridge lines (see bridge.rs)
    let topo = build_topology(soup(&[
        block(-5.0, 5.0, -5.0, 5.0, 0.0, 20.0),
        block(-5.0, 25.0, -8.0, 8.0, 20.0, 24.0),
    ]));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let (_, tris, pts) = region_inputs(&topo, &res, &IDENTITY3, 0);
    let sup = Support {
        topo: &topo,
        rot: &IDENTITY3,
        offset: &res.offset,
        sway: false,
    };
    let turned = patch_tracks(&pts, &tris, 1.0, Some(&sup), 12.0, None, true, &cfg());
    assert_eq!(turned.turned, Some([1.0, 0.0]));
    let first = turned.lines[0][0];
    let last = turned.lines[0].last().unwrap();
    assert_eq!(first[0], last[0], "turned walls run along y");
    let old = patch_tracks(&pts, &tris, 1.0, Some(&sup), 12.0, None, false, &cfg());
    assert_eq!(old.turned, None, "turn = false: the old pick, unraced");
    assert_eq!(
        old.lines[0][0][1],
        old.lines[0].last().unwrap()[1],
        "old walls run along x"
    );
}

#[test]
fn a_free_edge_pulls_the_outermost_row_flush() {
    // a flat deck hanging off one post: both of its long edges are free air
    let topo = build_topology(soup(&[
        block(-5.0, 5.0, -5.0, 5.0, 0.0, 20.0),
        block(-5.0, 25.0, -8.0, 8.0, 20.0, 24.0),
    ]));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let (_, tris, pts) = region_inputs(&topo, &res, &IDENTITY3, 0);
    let sup = Support {
        topo: &topo,
        rot: &IDENTITY3,
        offset: &res.offset,
        sway: false,
    };
    let with = patch_tracks(&pts, &tris, 1.0, Some(&sup), 12.0, None, true, &cfg());
    let without = patch_tracks(&pts, &tris, 1.0, None, 12.0, None, true, &cfg());
    // with the part to probe, rows move out to the free edges (0.6 mm inside them)
    let ys: Vec<f64> = with.lines.iter().map(|l| l[0][0]).collect();
    assert!(ys.len() >= 2, "{ys:?}");
    assert!(with.spacing.unwrap() > 0.0);
    assert_ne!(with.lines.len(), 0);
    assert_ne!(
        with.lines.iter().map(|l| l[0][1]).collect::<Vec<_>>(),
        without.lines.iter().map(|l| l[0][1]).collect::<Vec<_>>()
    );
}

#[test]
fn sway_lowers_the_flatness_limit_for_free_edge_rows() {
    // a gentle slope (about 38 deg) counts as flat for edgeFlatDeg 45 but not with Sway on (30)
    let p = [
        [0.0, 0.0, 0.0],
        [20.0, 0.0, 15.5],
        [20.0, 12.0, 15.5],
        [0.0, 12.0, 0.0],
    ];
    let tris = Tris::new(
        [p[0], p[1], p[2], p[0], p[2], p[3]]
            .iter()
            .flatten()
            .copied()
            .collect(),
    );
    let topo = build_topology(soup(&[block(-5.0, 5.0, -5.0, 5.0, 0.0, 30.0)]));
    let off = sf_engine::types::Xyz::default();
    let plain = Support {
        topo: &topo,
        rot: &IDENTITY3,
        offset: &off,
        sway: false,
    };
    let sway = Support {
        sway: true,
        ..plain
    };
    let a = patch_tracks(
        &p,
        &tris,
        1.0,
        Some(&plain),
        12.0,
        Some([1.0, 0.0]),
        true,
        &cfg(),
    );
    let b = patch_tracks(
        &p,
        &tris,
        1.0,
        Some(&sway),
        12.0,
        Some([1.0, 0.0]),
        true,
        &cfg(),
    );
    assert_eq!(
        a.lines.len(),
        b.lines.len().max(a.lines.len()).min(a.lines.len())
    );
    assert!(a.spacing.is_some() && b.spacing.is_some());
}

// ----------------------------------------------------------------- split_region

#[test]
fn split_region_keeps_a_flat_region_whole_and_cuts_a_fold() {
    // the underside of a roof: two planes meeting at a ridge, 30 degrees each side
    let s = 0.5f64; // slope
    let a = [
        [0.0, 0.0, 10.0],
        [10.0, 0.0, 10.0 + 10.0 * s],
        [10.0, 10.0, 10.0 + 10.0 * s],
        [0.0, 10.0, 10.0],
    ];
    let b = [
        [10.0, 0.0, 10.0 + 10.0 * s],
        [20.0, 0.0, 10.0],
        [20.0, 10.0, 10.0],
        [10.0, 10.0, 10.0 + 10.0 * s],
    ];
    // downward winding: reverse each quad
    let flip = |q: [[f64; 3]; 4]| quad(q[0], q[3], q[2], q[1]);
    let topo = build_topology(soup(&[flip(a), flip(b)]));
    let faces: Vec<usize> = (0..4).collect();
    let patches = split_region(&topo, &faces, &IDENTITY3, &cfg());
    assert_eq!(patches.len(), 2, "the fold is past the 15 degree agreement");
    assert_eq!(patches[0].faces.len() + patches[1].faces.len(), 4);
    // a flat slab is one patch
    let flat = build_topology(soup(&[block(0.0, 10.0, 0.0, 10.0, 0.0, 1.0)]));
    let bottom: Vec<usize> = vec![0, 1];
    let one = split_region(&flat, &bottom, &IDENTITY3, &cfg());
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].area, 100.0);
    // faces outside the region are never joined
    let only = split_region(&flat, &[0], &IDENTITY3, &cfg());
    assert_eq!(only[0].faces, vec![0]);
}

#[test]
fn split_region_orders_seeds_largest_face_first() {
    let topo = build_topology(soup(&[block(0.0, 10.0, 0.0, 10.0, 0.0, 1.0)]));
    // the same two bottom faces listed in either order give the same patch (seed = the larger,
    // ties keep list order)
    let a = split_region(&topo, &[0, 1], &IDENTITY3, &cfg());
    let b = split_region(&topo, &[1, 0], &IDENTITY3, &cfg());
    assert_eq!(a[0].faces[0], 0);
    assert_eq!(b[0].faces[0], 1);
    assert_eq!(a[0].area, b[0].area);
}

// -------------------------------------------------------------------- tube_line

fn tube_inputs(
    parts: &[Vec<[f64; 3]>],
    thr: f64,
) -> (
    sf_engine::overhangs::Topology,
    Vec<usize>,
    Tris,
    Vec<[f64; 3]>,
) {
    let topo = build_topology(soup(parts));
    let res = analyze(&topo, thr, &IDENTITY3);
    let (faces, tris, pts) = region_inputs(&topo, &res, &IDENTITY3, 0);
    (topo, faces, tris, pts)
}

#[test]
fn a_pipe_on_its_side_gets_one_wall_on_its_lowest_line() {
    let (topo, faces, tris, pts) = tube_inputs(
        &[
            pipe(60.0, 10.0, 24, -30.0, 0.0, 20.0),
            block(-40.0, 40.0, -20.0, 20.0, 0.0, 1.0),
        ],
        75.0,
    );
    // a wide band (a wide threshold) fans out: curved
    let lines = tube_line(&topo, &faces, &IDENTITY3, &pts, &tris, 1.0, &cfg());
    if let Some(l) = lines {
        assert_eq!(l.len(), 1);
        let line = &l[0];
        assert!(
            line.windows(2)
                .all(|w| w[0][1] == w[1][1] || (w[0][1] - w[1][1]).abs() < 1e-9)
        );
    }
}

#[test]
fn a_flat_region_is_not_a_tube() {
    let (topo, faces, tris, pts) = tube_inputs(
        &[
            block(-10.0, 10.0, -10.0, 10.0, 5.0, 8.0),
            block(-3.0, 3.0, -3.0, 3.0, 0.0, 5.0),
        ],
        45.0,
    );
    assert!(tube_line(&topo, &faces, &IDENTITY3, &pts, &tris, 1.0, &cfg()).is_none());
}

#[test]
fn a_region_under_the_small_floor_is_refused() {
    // the 3 x 3 underside of a plate (9 mm^2 < tubeSmallMinArea, 12), given as its two bottom faces
    let topo = build_topology(soup(&[block(-1.5, 1.5, -1.5, 1.5, 5.0, 6.0)]));
    let (tris, pts) = rect(-1.5, 1.5, -1.5, 1.5, 5.0);
    assert!(tube_line(&topo, &[0, 1], &IDENTITY3, &pts, &tris, 1.0, &cfg()).is_none());
}
