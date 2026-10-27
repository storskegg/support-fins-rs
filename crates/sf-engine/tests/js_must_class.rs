//! Port of upstream `tests/must_class.test.js`: which red counts (web/fins/coverage.js) -- the rule
//! the coverage scoreboard and the Full coverage fill pass share. A tiny bore's ceiling (<= 6 mm,
//! "a model issue") is exempt as HOLE; a wider bore's is MUST, since bores DO get supported; (LOW,
//! the near-plate strip, is per face centroid under 0.82 mm.) And reachOf: a wall top holds a face
//! within R in plan and 0..1.5 mm under it.

use sf_engine::fins::coverage::{HOLE, LOW, MUST, classify, reach_of};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::types::Rot3;
use sf_testkit::{holed_plate_topo, rot_y, tilted_block_topo};

/// Area per class over the overhang faces whose centroid sits above zMin (must_class.test.js:12-25).
fn areas(topo: &Topology, res: &Analysis, rot: &Rot3, z_min: f64) -> [f64; 3] {
    let cls = classify(topo, res, rot);
    let mut by = [0.0f64; 3];
    for f in 0..topo.n_faces {
        if res.over[f] == 0 {
            continue;
        }
        let mut cz = 0.0f64;
        for i in 0..3 {
            let o = f * 9 + i * 3;
            cz += (rot[2] * topo.pos[o]
                + rot[5] * topo.pos[o + 1]
                + rot[8] * topo.pos[o + 2]
                + res.offset.z)
                / 3.0;
        }
        if cz > z_min {
            by[cls[f] as usize] += topo.area[f];
        }
    }
    by
}

/// A 40 x 40 block on its side (rotY 90), with a square bore along X. The bore's ceiling is the
/// only overhang well above the plate (must_class.test.js:29-33).
fn bore(half: f64) -> (Topology, Analysis, Rot3) {
    let topo = holed_plate_topo(20.0, 20.0, 30.0, half, half);
    let rot = rot_y(90.0);
    let res = analyze(&topo, 45.0, &rot);
    (topo, res, rot)
}

fn fmt(by: &[f64; 3]) -> String {
    by.iter()
        .map(|a| format!("{a:.0}"))
        .collect::<Vec<_>>()
        .join(",")
}

// IGNORED until classify's axis fit is fixed (its own PR): it fits a patch's axis to its
// triangles' corners, and on a 2-triangle roof the two shared corners count twice, tilting the axis
// -- this 4 x 30 slot reads 5.3 wide, past the 4.5 limit, so it scores MUST. Round bores (many
// even triangles) are unaffected.
// must_class.test.js:41 (`ignore: true` upstream, and so here: the JS test does not run)
#[test]
#[ignore = "ignored upstream too (must_class.test.js:41): classify's axis fit on a 2-triangle roof"]
fn must_class_a_4_mm_bore_ceiling_is_a_tiny_hole_exempt() {
    let (topo, res, rot) = bore(2.0);
    let by = areas(&topo, &res, &rot, 5.0);
    assert!(
        by[HOLE as usize] > 100.0,
        "bore ceiling not HOLE: {}",
        fmt(&by)
    );
    assert!(
        by[MUST as usize] < 1.0,
        "bore ceiling partly MUST: {}",
        fmt(&by)
    );
}

// must_class.test.js:47
#[test]
fn must_class_a_10_mm_bore_ceiling_is_must_bores_get_supported() {
    let (topo, res, rot) = bore(5.0);
    let by = areas(&topo, &res, &rot, 5.0);
    assert!(
        by[MUST as usize] > 250.0,
        "bore ceiling not MUST: {}",
        fmt(&by)
    );
    assert!(
        by[HOLE as usize] < 1.0,
        "wide bore read as a tiny hole: {}",
        fmt(&by)
    );
}

// must_class.test.js:53
#[test]
fn must_class_a_tilted_slab_underside_is_must() {
    let topo = tilted_block_topo(-20.0, 20.0, -20.0, 20.0, -3.0, 3.0, 55.0);
    let rot = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let res = analyze(&topo, 45.0, &rot);
    let by = areas(&topo, &res, &rot, f64::NEG_INFINITY);
    assert!(
        by[MUST as usize] > 100.0,
        "slab underside not MUST: {}",
        fmt(&by)
    );
    assert_eq!(by[HOLE as usize], 0.0, "a slab has no hole");
    assert_eq!(
        by[LOW as usize], 0.0,
        "two big faces, both centred far above the near-plate strip"
    );
}

// must_class.test.js:63
#[test]
fn reach_of_within_r_in_plan_and_0_to_1_5_mm_under_grows_with_add() {
    let mut near = reach_of(&[[0.0, 0.0, 10.0]], 6.0);
    assert!(
        near.near(5.0, 0.0, 11.0),
        "a face 1 mm over a top 5 mm away is held"
    );
    assert!(!near.near(7.0, 0.0, 11.0), "past R in plan");
    assert!(!near.near(0.0, 0.0, 12.0), "more than 1.5 mm over the top");
    assert!(!near.near(0.0, 0.0, 9.0), "below the top");
    assert!(!near.near(40.0, 40.0, 11.0));
    near.add([40.0, 40.0, 10.0]);
    assert!(near.near(40.0, 40.0, 11.0), "add() serves a new top");
}
