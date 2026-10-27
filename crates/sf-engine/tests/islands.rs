//! Tests of `sf_engine::islands::find_islands` (web/islands.js): the scenarios of upstream
//! `tests/islands.test.js` that need only the topology and `analyze` (the Full-coverage ones
//! need fins, M3), plus option overrides and the knife-edge point sampling.

use sf_engine::config::Config;
use sf_engine::islands::{Island, IslandOpts, find_islands};
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::types::IDENTITY3;

/// `block(x0, x1, y0, y1, z0, z1)` (tests/_util.js) as f32-rounded positions.
fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<f64> {
    let p = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let t: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [3, 0, 4],
        [3, 4, 7],
    ];
    t.iter()
        .flat_map(|tri| tri.iter().flat_map(|&i| p[i]))
        .map(|v| v as f32 as f64)
        .collect()
}

/// tests/islands.test.js:10 -- a cube balanced on a corner (the (1,1,1) diagonal turned onto +Z).
fn corner_cube(cx: f64, cy: f64, cz: f64, side: f64) -> Vec<f64> {
    let raw = block(
        -side / 2.0,
        side / 2.0,
        -side / 2.0,
        side / 2.0,
        -side / 2.0,
        side / 2.0,
    );
    let r3 = 3.0f64.sqrt();
    let d = [1.0 / r3, 1.0 / r3, 1.0 / r3];
    let k = [d[1], -d[0], 0.0];
    let s = (k[0] * k[0] + k[1] * k[1] + k[2] * k[2]).sqrt();
    let c = d[2];
    let (kx, ky) = (k[0] / s, k[1] / s);
    let mut out = vec![0.0; raw.len()];
    for i in (0..raw.len()).step_by(3) {
        let p = [raw[i], raw[i + 1], raw[i + 2]];
        let kp = [ky * p[2], -kx * p[2], kx * p[1] - ky * p[0]];
        let kd = kx * p[0] + ky * p[1];
        let kv = [kx, ky, 0.0];
        for j in 0..3 {
            out[i + j] = (p[j] * c + kp[j] * s + kv[j] * kd * (1.0 - c)) as f32 as f64;
        }
        out[i] += cx;
        out[i + 1] += cy;
        out[i + 2] += cz;
    }
    out
}

fn topo_of(parts: &[Vec<f64>]) -> Topology {
    build_topology(parts.iter().flatten().map(|&v| v as f32 as f64).collect())
}

fn islands_with(topo: &Topology, opts: &IslandOpts) -> Vec<Island> {
    let res = analyze(topo, 45.0, &IDENTITY3);
    find_islands(topo, &res, &IDENTITY3, opts, &Config::default())
}

fn islands_of(topo: &Topology) -> Vec<Island> {
    islands_with(topo, &IslandOpts::default())
}

fn pillar() -> Vec<f64> {
    block(0.0, 10.0, 0.0, 10.0, 0.0, 30.0)
}

const LOW: f64 = 20.0 - 8.0 * 1.7320508075688772 / 2.0;

// tests/islands.test.js:36
#[test]
fn a_corner_down_cube_hanging_off_a_pillar_starts_in_midair_no_face_red() {
    let topo = topo_of(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let isl = find_islands(
        &topo,
        &res,
        &IDENTITY3,
        &IslandOpts::default(),
        &Config::default(),
    );
    assert_eq!(isl.len(), 1);
    // (seated: analyze centres the part on the plate)
    assert!((isl[0].x - (15.0 + res.offset.x)).abs() <= 0.01, "x");
    assert!((isl[0].y - (5.0 + res.offset.y)).abs() <= 0.01, "y");
    assert!((isl[0].z - LOW).abs() <= 0.01, "z {}", isl[0].z);
    assert!(isl[0].area >= 0.03, "area {}", isl[0].area);
}

// tests/islands.test.js:51
#[test]
fn a_start_resting_within_a_layer_of_material_below_is_no_island() {
    let under = block(10.0, 25.0, 0.0, 10.0, 0.0, LOW - 0.1);
    assert!(
        islands_of(&topo_of(&[
            pillar(),
            corner_cube(15.0, 5.0, 20.0, 8.0),
            under
        ]))
        .is_empty()
    );
    // ... ending 0.5 mm short of it in plan, past the 0.3 mm grace: it hangs again
    let short = block(10.0, 14.5, 0.0, 10.0, 0.0, LOW - 0.1);
    assert_eq!(
        islands_of(&topo_of(&[
            pillar(),
            corner_cube(15.0, 5.0, 20.0, 8.0),
            short
        ]))
        .len(),
        1
    );
}

// tests/islands.test.js:60
#[test]
fn red_low_points_are_left_to_the_overhang_and_plate_starts_to_the_plate() {
    assert!(
        islands_of(&topo_of(&[
            pillar(),
            block(10.0, 18.0, 0.0, 8.0, 16.0, 24.0)
        ]))
        .is_empty()
    );
    assert!(
        islands_of(&topo_of(&[corner_cube(
            0.0,
            0.0,
            8.0 * 3.0f64.sqrt() / 2.0,
            8.0
        )]))
        .is_empty()
    );
}

// tests/islands.test.js:80
#[test]
fn starts_within_2mm_merge_into_one_island_farther_ones_stay_apart() {
    let two = |dy: f64| {
        islands_of(&topo_of(&[
            pillar(),
            corner_cube(15.0, 5.0, 20.0, 8.0),
            corner_cube(15.0, 5.0 + dy, 20.0, 8.0),
        ]))
    };
    let near = two(1.5);
    assert!(
        near.len() == 1 && near[0].points.len() == 2,
        "got {}",
        near.len()
    );
    assert_eq!(two(4.0).len(), 2);
}

// tests/islands.test.js:86
#[test]
fn a_start_under_the_area_floor_is_dropped() {
    let topo = topo_of(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let a = islands_of(&topo)[0].area;
    let opts = IslandOpts {
        min_area: Some(a + 0.01),
        ..Default::default()
    };
    assert!(islands_with(&topo, &opts).is_empty());
}

/// tests/islands.test.js:94 -- an open steep surface z = 2x on a 0.1 mm grid with one vertex
/// pushed 0.3 mm down.
fn dimpled_wall() -> Vec<f64> {
    let (n, h) = (20, 0.1);
    let z = |i: i32, j: i32| 10.0 + 2.0 * i as f64 * h - if i == 10 && j == 10 { 0.3 } else { 0.0 };
    let p = |i: i32, j: i32| [i as f64 * h, j as f64 * h, z(i, j)];
    let mut t = Vec::new();
    for i in 0..n {
        for j in 0..n {
            for q in [
                p(i, j),
                p(i, j + 1),
                p(i + 1, j + 1),
                p(i, j),
                p(i + 1, j + 1),
                p(i + 1, j),
            ] {
                t.extend(q);
            }
        }
    }
    t
}

// tests/islands.test.js:105
#[test]
fn a_dimple_in_a_steep_wall_prints_on_the_wall_below_it() {
    let topo = topo_of(&[dimpled_wall()]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let base = IslandOpts {
        plate_z: Some(-1.0),
        min_area: Some(0.0),
        ..Default::default()
    };
    // seen as a candidate when the layer is thinner than its drop to the row below...
    let thin = IslandOpts {
        layer: Some(0.05),
        grace: Some(0.0),
        ..base
    };
    let thin = find_islands(&topo, &res, &IDENTITY3, &thin, &Config::default());
    assert_eq!(thin.len(), 2, "the dimple is a downward low point");
    // ... and dropped at a real layer
    let real = find_islands(&topo, &res, &IDENTITY3, &base, &Config::default());
    assert!(real.len() == 1 && (real[0].z - 10.0 - res.offset.z).abs() < 1e-6);
}

// tests/islands.test.js:121
#[test]
fn a_peg_end_too_small_for_a_fin_amber_still_starts_in_midair() {
    let topo = topo_of(&[
        pillar(),
        block(10.0, 30.0, 0.0, 10.0, 20.0, 24.0),
        block(20.0, 22.0, 4.0, 6.0, 16.0, 20.5),
    ]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    assert!(
        res.regions.len() == 1 && res.raw_region_count == 2,
        "the arm is a region, the peg end a sliver"
    );
    let isl = islands_of(&topo);
    assert!(
        isl.len() == 1 && (isl[0].z - 16.0 - res.offset.z).abs() < 1e-6,
        "got {}",
        isl.len()
    );
}

#[test]
fn options_override_the_config_per_key() {
    let topo = topo_of(&[pillar(), corner_cube(15.0, 5.0, 20.0, 8.0)]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let mut cfg = Config::default();
    cfg.island.min_area = 1e9; // the config says nothing is big enough...
    assert!(find_islands(&topo, &res, &IDENTITY3, &IslandOpts::default(), &cfg).is_empty());
    // ... and an option beats the config
    let opts = IslandOpts {
        min_area: Some(0.0),
        ..Default::default()
    };
    assert_eq!(find_islands(&topo, &res, &IDENTITY3, &opts, &cfg).len(), 1);
    // a cap of two faces makes the start "big" (infinite area), still reported
    let big = IslandOpts {
        max_faces: Some(2.0),
        ..Default::default()
    };
    let r = find_islands(&topo, &res, &IDENTITY3, &big, &Config::default());
    assert_eq!(r.len(), 1);
    assert!(r[0].area.is_infinite());
}

#[test]
fn a_part_with_no_low_points_has_no_islands() {
    assert!(islands_of(&topo_of(&[block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0)])).is_empty());
    assert!(islands_of(&topo_of(&[])).is_empty());
}
