//! Port of upstream `tests/islands.test.js`: island starts (web/islands.js, local issue 041 / GitHub
//! #18 #257): a feature whose first layer starts in midair while no face around its low point is red.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::islands::{Island, IslandOpts, find_islands};
use sf_engine::overhangs::{Topology, analyze};
use sf_engine::types::Rot3;
use sf_testkit::{assert_close, block, inside_count, load_model, rot_x};
use std::sync::Arc;

/// A cube balanced on a corner: the (1,1,1) diagonal turned onto +Z, so its three lower faces lean
/// at 54.7deg -- steeper than 45, none red -- and meet at one low point.
fn corner_cube(cx: f64, cy: f64, cz: f64, side: f64) -> Vec<f64> {
    let raw = block(
        -side / 2.0,
        side / 2.0,
        -side / 2.0,
        side / 2.0,
        -side / 2.0,
        side / 2.0,
    );
    let d: Vec<f64> = [1.0, 1.0, 1.0].iter().map(|v| v / 3.0f64.sqrt()).collect();
    // Rodrigues: turn d onto z about k = d x z
    let k = [d[1], -d[0], 0.0];
    let s = sf_engine::jsnum::hypot(&k);
    let c = d[2];
    let (kx, ky) = (k[0] / s, k[1] / s);
    let f32r = |x: f64| (x as f32) as f64; // a Float32Array store
    let mut out = vec![0.0; raw.len()];
    for i in (0..raw.len()).step_by(3) {
        let p = [raw[i], raw[i + 1], raw[i + 2]];
        let kp = [ky * p[2], -kx * p[2], kx * p[1] - ky * p[0]]; // k x p
        let kd = kx * p[0] + ky * p[1]; // k . p
        let ax = [kx, ky, 0.0];
        for j in 0..3 {
            out[i + j] = f32r(p[j] * c + kp[j] * s + ax[j] * kd * (1.0 - c));
        }
        out[i] = f32r(out[i] + cx);
        out[i + 1] = f32r(out[i + 1] + cy);
        out[i + 2] = f32r(out[i + 2] + cz);
    }
    out
}

fn topo_parts(parts: &[Vec<f64>]) -> Arc<Topology> {
    // `new Float32Array(...)`: every coordinate rounds to f32 (the `LOW - 0.1` slab)
    topo_of(f32s(&parts.iter().flatten().copied().collect::<Vec<f64>>()))
}

fn islands_with(topo: &Arc<Topology>, rot: &Rot3, opts: &IslandOpts) -> Vec<Island> {
    let res = analyze(topo, 45.0, rot);
    find_islands(topo, &res, rot, opts, &Config::default())
}
fn islands_of(topo: &Arc<Topology>, rot: &Rot3) -> Vec<Island> {
    islands_with(topo, rot, &IslandOpts::default())
}

// a pillar on the plate with a corner-down cube stuck to its side, its low corner 13.07 mm up and
// 5 mm clear of the pillar in plan
fn pillar() -> Vec<f64> {
    block(0.0, 10.0, 0.0, 10.0, 0.0, 30.0)
}
fn low() -> f64 {
    20.0 - 8.0 * 3.0f64.sqrt() / 2.0
}

// islands.test.js:40
#[test]
fn islands_a_corner_down_cube_hanging_off_a_pillar_starts_in_midair_no_face_red() {
    let topo = topo_parts(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let res = analyze(&topo, 45.0, &I);
    let isl = find_islands(&topo, &res, &I, &IslandOpts::default(), &Config::default());
    assert!(isl.len() == 1, "one island, got {}", isl.len());
    // (seated: analyze centres the part on the plate)
    assert_close(isl[0].x, 15.0 + res.offset.x, 0.01, "at the corner in x");
    assert_close(isl[0].y, 5.0 + res.offset.y, 0.01, "at the corner in y");
    assert_close(isl[0].z, low(), 0.01, "at the corner height");
    assert!(
        isl[0].area >= 0.03,
        "a real cross-section one layer up ({:.3} mm2)",
        isl[0].area
    );
}

// islands.test.js:52
#[test]
fn islands_a_start_resting_within_a_layer_of_material_below_is_no_island() {
    // a slab off the pillar whose top is 0.1 mm under the corner
    assert!(
        islands_of(
            &topo_parts(&[
                pillar(),
                corner_cube(15.0, 5.0, 20.0, 8.0),
                block(10.0, 25.0, 0.0, 10.0, 0.0, low() - 0.1)
            ]),
            &I
        )
        .is_empty(),
        "the corner prints on the slab under it"
    );
    // ... ending 0.5 mm short of it in plan, past the 0.3 mm grace: it hangs again
    assert!(
        islands_of(
            &topo_parts(&[
                pillar(),
                corner_cube(15.0, 5.0, 20.0, 8.0),
                block(10.0, 14.5, 0.0, 10.0, 0.0, low() - 0.1)
            ]),
            &I
        )
        .len()
            == 1,
        "a slab out of reach holds nothing"
    );
}

// islands.test.js:61
#[test]
fn islands_red_low_points_are_left_to_the_overhang_plate_starts_to_the_plate() {
    // a square arm's low corners are corners of its flat (red) underside
    assert!(
        islands_of(
            &topo_parts(&[pillar(), block(10.0, 18.0, 0.0, 8.0, 16.0, 24.0)]),
            &I
        )
        .is_empty(),
        "a flat-bottomed arm is red, not an island"
    );
    assert!(
        islands_of(
            &topo_parts(&[corner_cube(0.0, 0.0, 8.0 * 3.0f64.sqrt() / 2.0, 8.0)]),
            &I
        )
        .is_empty(),
        "a cube on its corner stands on the plate"
    );
}

// islands.test.js:67
#[test]
fn islands_the_stress_models_have_none_bar_lowledge_at_y45_a_45_45_knife_edge() {
    for name in [
        "cube",
        "sphere",
        "torus",
        "lbracket",
        "staircase",
        "portal",
        "tshape",
        "cone",
    ] {
        let topo = Arc::new(load_model(name));
        for d in [0.0, 45.0, -45.0] {
            assert!(islands_of(&topo, &rot_x(d)).is_empty(), "{name} X{d}");
        }
    }
    let ry = |d: f64| {
        let (c, s) = (
            (d * std::f64::consts::PI / 180.0).cos(),
            (d * std::f64::consts::PI / 180.0).sin(),
        );
        [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c]
    };
    let edge = islands_of(&Arc::new(load_model("lowledge")), &ry(45.0));
    assert!(
        edge.len() == 1,
        "lowledge Y45: one island, got {}",
        edge.len()
    );
    let ys: Vec<f64> = edge[0].points.iter().map(|p| p[1]).collect();
    let span = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - ys.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        span > 39.0,
        "the whole 40 mm edge is the target, not its middle"
    );
}

// islands.test.js:79
#[test]
fn islands_starts_within_2_mm_merge_into_one_island_farther_ones_stay_apart() {
    let two = |dy: f64| {
        islands_of(
            &topo_parts(&[
                pillar(),
                corner_cube(15.0, 5.0, 20.0, 8.0),
                corner_cube(15.0, 5.0 + dy, 20.0, 8.0),
            ]),
            &I,
        )
    };
    let near = two(1.5);
    assert!(
        near.len() == 1 && near[0].points.len() == 2,
        "1.5 mm apart: one island of two starts, got {}",
        near.len()
    );
    assert!(two(4.0).len() == 2, "4 mm apart: two islands");
}

// islands.test.js:86
#[test]
fn islands_a_start_under_the_area_floor_is_dropped() {
    let topo = topo_parts(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let a = islands_of(&topo, &I)[0].area;
    let o = IslandOpts {
        min_area: Some(a + 0.01),
        ..Default::default()
    };
    assert!(
        islands_with(&topo, &I, &o).is_empty(),
        "a {a:.3} mm2 start under a higher floor"
    );
}

// An open steep surface, z = 2x (its faces lean out and down), on a 0.1 mm grid, with one vertex
// pushed 0.3 mm down: a low point, but its own surface runs lower within a layer -- the slicer prints
// it on the layer below.
fn dimpled_wall() -> Vec<f64> {
    let (n, h) = (20usize, 0.1);
    let z =
        |i: usize, j: usize| 10.0 + 2.0 * i as f64 * h - if i == 10 && j == 10 { 0.3 } else { 0.0 };
    let p = |i: usize, j: usize| [i as f64 * h, j as f64 * h, z(i, j)];
    let mut t: Vec<[f64; 3]> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            t.extend([
                p(i, j),
                p(i, j + 1),
                p(i + 1, j + 1),
                p(i, j),
                p(i + 1, j + 1),
                p(i + 1, j),
            ]);
        }
    }
    f32s(&t.into_iter().flatten().collect::<Vec<_>>())
}

// islands.test.js:106
#[test]
fn islands_a_dimple_in_a_steep_wall_prints_on_the_wall_below_it() {
    let topo = topo_of(dimpled_wall());
    let res = analyze(&topo, 45.0, &I);
    // (the sheet's own bottom edge, at z 10, is a start in midair too)
    let opts = IslandOpts {
        plate_z: Some(-1.0),
        min_area: Some(0.0),
        ..Default::default()
    };
    // seen as a candidate when the layer is thinner than its drop to the row below...
    let thin = islands_with(
        &topo,
        &I,
        &IslandOpts {
            layer: Some(0.05),
            grace: Some(0.0),
            ..opts
        },
    );
    assert!(
        thin.len() == 2,
        "the dimple is a downward low point ({} starts)",
        thin.len()
    );
    // ... and dropped at a real layer: the wall under it is within reach
    let real = islands_with(&topo, &I, &opts);
    assert!(
        real.len() == 1 && (real[0].z - 10.0 - res.offset.z).abs() < 1e-6,
        "only the bottom edge at 0.2 mm layers"
    );
}

// islands.test.js:119
#[test]
fn islands_a_peg_end_too_small_for_a_fin_amber_still_starts_in_midair() {
    // an arm off the pillar (its flat underside a fin-sized region) with a 2 x 2 mm peg hanging under
    // it: the peg's 4 mm2 end is an amber sliver, held by nothing
    let topo = topo_parts(&[
        pillar(),
        block(10.0, 30.0, 0.0, 10.0, 20.0, 24.0),
        block(20.0, 22.0, 4.0, 6.0, 16.0, 20.5),
    ]);
    let res = analyze(&topo, 45.0, &I);
    assert!(
        res.regions.len() == 1 && res.raw_region_count == 2,
        "the arm is a region, the peg end a sliver"
    );
    let isl = islands_of(&topo, &I);
    assert!(
        isl.len() == 1 && (isl[0].z - 16.0 - res.offset.z).abs() < 1e-6,
        "one island at the peg end, got {}",
        isl.len()
    );
}

// Full coverage holds them (fins/fill.js): the island starts are fill targets, held only by a wall
// top within FILL.islandReach under them.
fn held_by(built: &FinsBuilt, isl: &Island) -> bool {
    let reach = Config::default().fill.island_reach;
    isl.points.iter().any(|p| {
        built.fins.iter().any(|f| {
            f.line.iter().any(|q| {
                sf_engine::jsnum::hypot(&[p[0] - q[0], p[1] - q[1]]) <= reach
                    && p[2] - q[2] >= -0.05
                    && p[2] - q[2] <= 1.5
            })
        })
    })
}

fn bf(
    topo: &Arc<Topology>,
    mode: &str,
    tines: Option<bool>,
    islands: Option<Vec<Island>>,
) -> FinsBuilt {
    let res = analyze(topo, 45.0, &I);
    build_fins(
        topo,
        &res,
        &I,
        &FinsOpts {
            mode: Some(mode.into()),
            bed_pad: Some(true),
            tines,
            islands,
            ..Default::default()
        },
        &Config::default(),
    )
}

// islands.test.js:136
#[test]
fn islands_full_coverage_walls_a_start_in_midair_auto_does_not_full_only() {
    let topo = topo_parts(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let isl = islands_of(&topo, &I).remove(0);
    let full = bf(&topo, "full", None, None);
    let fill = full.fill.as_ref().expect("Full reports its fill");
    assert!(
        fill.islands == 1 && fill.islands_bare == 0,
        "Full: 1 island, none bare ({}/{})",
        fill.islands,
        fill.islands_bare
    );
    assert!(held_by(&full, &isl), "a wall top right under the corner");
    let auto = bf(&topo, "auto", None, None);
    assert!(
        !held_by(&auto, &isl),
        "Auto has nothing under it (islands are Full only)"
    );
}

// #257's outside corner: a quarter pyramid hanging point-down, its tip 0.5 mm in from under its top
// corner -- two sides drop near sheer to the tip, two climb away from it at ~48deg -- so the tip is at
// the part's edge and a line centred on it is half in air
fn edge_corner(x: f64, y: f64, z: f64, side: f64, rise: f64) -> Vec<f64> {
    let t = [x - 0.5, y - 0.5, z];
    let a = [x, y, z + rise];
    let b = [x - side, y, z + rise];
    let c = [x - side, y - side, z + rise];
    let d = [x, y - side, z + rise];
    let mid = [x - side / 2.0, y - side / 2.0, z + rise * 0.75];
    let mut out = Vec::new();
    for [p, q, r] in [
        [t, a, b],
        [t, b, c],
        [t, c, d],
        [t, d, a],
        [a, d, c],
        [a, c, b],
    ] {
        let mut n = [0.0; 3];
        for k in 0..3 {
            n[k] = (q[(k + 1) % 3] - p[(k + 1) % 3]) * (r[(k + 2) % 3] - p[(k + 2) % 3])
                - (q[(k + 2) % 3] - p[(k + 2) % 3]) * (r[(k + 1) % 3] - p[(k + 1) % 3]);
        }
        let away = n
            .iter()
            .enumerate()
            .fold(0.0, |t, (k, v)| t + v * (p[k] - mid[k]))
            > 0.0;
        out.extend(p);
        out.extend(if away { q } else { r });
        out.extend(if away { r } else { q });
    }
    f32s(&out)
}

// islands.test.js:162
#[test]
fn islands_full_walls_an_outside_corners_tip_on_the_parts_edge_257_from_the_side_that_runs_up_the_part()
 {
    let topo = topo_parts(&[pillar(), edge_corner(25.0, 5.0, 20.0, 9.5, 10.0)]);
    let res = analyze(&topo, 45.0, &I);
    let isl = islands_of(&topo, &I);
    assert!(
        isl.len() == 1 && (isl[0].z - 20.0).abs() < 0.01,
        "the tip is the one island ({})",
        isl.len()
    );
    let full = bf(&topo, "full", None, None);
    let fill = full.fill.as_ref().unwrap();
    assert!(
        fill.islands_bare == 0,
        "Full leaves the tip bare ({}/{})",
        fill.islands,
        fill.islands_bare
    );
    assert!(held_by(&full, &isl[0]), "a wall top right under the tip");
    let auto = bf(&topo, "auto", Some(false), None);
    let fulln = bf(&topo, "full", Some(false), None);
    let tail: Vec<[f64; 3]> = fulln.triangles[auto.triangles.len()..]
        .iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    let n = inside_count(&topo, &I, &res.offset, &tail);
    assert!(n == 0, "{n} fill vertices inside the part");
}

// islands.test.js:175 -- the JS also builds a copy of the topology without its vertex ids (what a
// Worker receives) and checks that no island targets are found without them; a Rust `Topology`
// always has its ids, so only the first half applies: the islands the page sends build the same walls.
#[test]
fn islands_the_pages_islands_opts_islands_build_the_same_walls() {
    let topo = topo_parts(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let isl = islands_of(&topo, &I);
    let here = bf(&topo, "full", None, None);
    let sent = bf(&topo, "full", None, Some(isl));
    assert!(
        sent.triangles.len() == here.triangles.len()
            && sent.fill.as_ref().unwrap().islands_bare == 0,
        "same build from the sent islands"
    );
}
