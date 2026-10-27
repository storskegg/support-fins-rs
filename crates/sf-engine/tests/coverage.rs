//! Tests of `sf_engine::fins::coverage`: `reach_of`, the HOLE / LOW / MUST classification,
//! `automatic_targets` (the augmented copy) and `touches_small_hole`.
mod support;
use sf_engine::fins::coverage::*;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;
use support::*;

fn topo_of(parts: &[Vec<[f64; 3]>]) -> Topology {
    build_topology(flat(parts).iter().map(|&v| v as f32 as f64).collect())
}

fn bore(d: f64) -> Topology {
    build_topology(circular_bore(d).iter().map(|&v| v as f32 as f64).collect())
}

#[test]
fn reach_of_needs_a_top_within_r_in_plan_and_under_the_face() {
    let reach = reach_of(&[[0.0, 0.0, 3.0]], 2.0);
    assert!(reach.near(0.0, 0.0, 3.0), "dz = 0 is in the window");
    assert!(
        reach.near(0.0, 0.0, 4.5),
        "dz = 1.5 is the top of the window"
    );
    assert!(!reach.near(0.0, 0.0, 4.6), "more than 1.5 below the face");
    assert!(
        reach.near(0.0, 0.0, 2.95),
        "0.05 above the face still counts"
    );
    assert!(
        !reach.near(0.0, 0.0, 2.9),
        "a top above the face does not hold it"
    );
    assert!(reach.near(2.0, 0.0, 3.0), "exactly R in plan");
    assert!(!reach.near(2.01, 0.0, 3.0));
    assert!(!reach.near(1.5, 1.5, 3.0), "hypot, not a box");
    // a top in the neighbouring grid cell is found; one two cells away is not even looked at
    let cells = reach_of(&[[5.9, 0.0, 3.0]], 6.0);
    assert!(cells.near(0.2, 0.0, 3.0));
    assert!(!cells.near(-0.2, 0.0, 3.0));
}

#[test]
fn reach_of_add_grows_the_set() {
    let mut reach = reach_of(&[], 3.0);
    assert!(!reach.near(1.0, 1.0, 2.0));
    reach.add([1.0, 1.0, 1.0]);
    assert!(reach.near(1.0, 1.0, 2.0));
    // the grid cell is at least 1 mm even for a tiny R
    let mut tiny = reach_of(&[], 0.2);
    tiny.add([-0.5, -0.5, 1.0]);
    assert!(tiny.near(-0.5, -0.5, 1.0));
}

#[test]
fn a_small_circular_bore_ceiling_is_a_hole_a_large_one_is_not() {
    for d in [3.0, 5.0] {
        let topo = bore(d);
        let res = analyze(&topo, 45.0, &IDENTITY3);
        let cls = classify(&topo, &res, &IDENTITY3);
        assert!(cls.contains(&HOLE), "diameter {d}");
        assert!(
            cls.iter()
                .enumerate()
                .all(|(f, &c)| res.over[f] != 0 || c == 0)
        );
    }
    let topo = bore(10.0);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    assert!(!classify(&topo, &res, &IDENTITY3).contains(&HOLE));
}

#[test]
fn a_flat_collar_over_a_post_is_not_a_hole() {
    let topo = topo_of(&[
        block(-1.0, 1.0, -1.0, 1.0, 0.0, 20.0),
        block(-5.0, 5.0, -5.0, 5.0, 10.0, 11.0),
    ]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let cls = classify(&topo, &res, &IDENTITY3);
    assert!(res.over_face_count > 0);
    assert!(!cls.contains(&HOLE));
    assert!(cls.contains(&MUST));
}

#[test]
fn faces_under_the_squat_floor_are_low_and_the_rest_must() {
    // a slab whose underside is 0.7 up (the 1 mm foot block sets the plate): centroid z < 0.82
    let topo = topo_of(&[
        block(0.0, 1.0, 0.0, 1.0, 0.0, 5.0),
        block(2.0, 12.0, 0.0, 10.0, 0.7, 5.0),
    ]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let cls = classify(&topo, &res, &IDENTITY3);
    let low = cls.iter().filter(|&&c| c == LOW).count();
    assert_eq!(low, 2, "the slab's two underside triangles");
    // a higher slab is MUST
    let topo = topo_of(&[
        block(0.0, 1.0, 0.0, 1.0, 0.0, 5.0),
        block(2.0, 12.0, 0.0, 10.0, 2.0, 5.0),
    ]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let cls = classify(&topo, &res, &IDENTITY3);
    assert!(cls.contains(&MUST) && !cls.contains(&LOW));
}

#[test]
fn automatic_targets_drop_hole_regions_and_keep_the_callers_result() {
    let topo = bore(3.0);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let before = res.clone();
    let t = automatic_targets(&topo, &res, &IDENTITY3);
    assert_eq!(res, before, "the caller's analyze result is untouched");
    assert_eq!(t.small_hole_regions, 1);
    assert!(t.analysis.regions.is_empty());
    assert_eq!(t.analysis.over, res.over, "the red mask survives");
    assert_eq!(t.analysis.kept, res.kept);
    assert_eq!(
        t.coverage_classes.iter().filter(|&&c| c == HOLE).count(),
        t.excluded_support_faces.iter().filter(|&&e| e != 0).count()
    );
    assert_eq!(t.excluded_support_tris.v.len() % 9, 0);
    assert!(!t.excluded_support_tris.is_empty());
    assert_eq!(t.excluded_support_volumes.len() % 24, 0);
    assert!(
        !t.excluded_support_volumes.is_empty(),
        "each ceiling reserves a prism down to the bore floor"
    );
    assert_eq!(classify_targets(&t), t.coverage_classes);
}

#[test]
fn automatic_targets_leave_an_ordinary_part_alone() {
    let topo = topo_of(&[
        block(-5.0, 5.0, -5.0, 5.0, 0.0, 20.0),
        block(-5.0, 25.0, -8.0, 8.0, 20.0, 24.0),
    ]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let t = automatic_targets(&topo, &res, &IDENTITY3);
    assert_eq!(t.analysis, res);
    assert_eq!(t.small_hole_regions, 0);
    assert!(t.excluded_support_faces.iter().all(|&e| e == 0));
    assert!(t.excluded_support_tris.is_empty() && t.excluded_support_volumes.is_empty());
}

fn stub(tris: Vec<f64>) -> Targets {
    let topo = topo_of(&[block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0)]);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let mut t = automatic_targets(&topo, &res, &IDENTITY3);
    t.excluded_support_tris = Tris::new(tris);
    t
}

#[test]
fn touches_small_hole_tests_stations_against_the_excluded_surfaces() {
    // one excluded triangle at z = 5 covering the origin
    let t = stub(vec![-2.0, -2.0, 5.0, 2.0, -2.0, 5.0, 0.0, 2.0, 5.0]);
    assert!(touches_small_hole(&t, &[[0.0, 0.0, 5.0]]));
    assert!(
        touches_small_hole(&t, &[[0.0, 0.0, 5.5]]),
        "0.5 mm is inside the settling allowance"
    );
    assert!(!touches_small_hole(&t, &[[0.0, 0.0, 5.51]]));
    assert!(
        !touches_small_hole(&t, &[[10.0, 0.0, 5.0]]),
        "not under the triangle"
    );
    assert!(
        touches_small_hole(&t, &[[10.0, 0.0, 5.0], [0.0, 0.0, 5.2]]),
        "any station will do"
    );
    assert!(!touches_small_hole(&t, &[]));
    assert!(
        !touches_small_hole(&stub(vec![]), &[[0.0, 0.0, 5.0]]),
        "no excluded surface at all"
    );
}
