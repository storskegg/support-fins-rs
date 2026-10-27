//! Port of upstream `tests/orient.test.js` (web/orient.js): the layer-posture bucket, the
//! load-alignment verdict and the strength solver. The two `suggestOrientations` cases need
//! `build_fins` and are ported with it (PR 2).

use sf_engine::orient::*;
use sf_engine::overhangs::analyze;
use sf_engine::types::Xyz;
use sf_testkit::{assert_close, block_topo};

fn size(x: f64, y: f64, z: f64) -> Xyz {
    Xyz { x, y, z }
}

// orient.test.js:16
#[test]
fn layer_verdict_a_tall_part_reads_weak() {
    assert_eq!(layer_verdict(&size(10.0, 10.0, 100.0)), Posture::Weak);
}

// orient.test.js:20
#[test]
fn layer_verdict_a_flat_slab_reads_strong() {
    assert_eq!(layer_verdict(&size(100.0, 80.0, 5.0)), Posture::Strong);
}

// orient.test.js:24
#[test]
fn layer_verdict_an_on_its_side_part_reads_mixed() {
    assert_eq!(layer_verdict(&size(100.0, 50.0, 70.0)), Posture::Mixed);
}

// orient.test.js:29: the JS checks the object has only `posture` (no `note`); the Rust return
// is the posture alone, which the type enforces. Pinned by its strings.
#[test]
fn layer_verdict_returns_posture_only() {
    assert_eq!(layer_verdict(&size(100.0, 80.0, 5.0)).as_str(), "strong");
}

// orient.test.js:39
#[test]
fn load_alignment_a_pull_straight_up_the_build_axis_is_poor() {
    assert_eq!(
        load_alignment([0.0, 0.0, 1.0]).unwrap().quality,
        Quality::Poor
    );
}

// orient.test.js:43
#[test]
fn load_alignment_a_pull_in_the_layer_plane_is_good() {
    assert_eq!(
        load_alignment([1.0, 0.0, 0.0]).unwrap().quality,
        Quality::Good
    );
}

// orient.test.js:47
#[test]
fn load_alignment_a_45_degree_pull_is_mixed() {
    let al = load_alignment([1.0, 0.0, 1.0]).unwrap();
    assert_eq!(al.quality, Quality::Mixed);
    assert_close(
        al.cross,
        std::f64::consts::FRAC_1_SQRT_2,
        1e-9,
        "cross should be |z|/|d| = 1/sqrt(2)",
    );
}

// orient.test.js:53
#[test]
fn load_alignment_the_good_mixed_boundary_sits_at_60_degrees_from_the_plate() {
    // cross = 0.5 is exactly 60deg off in-plane and must still count as good (<= 0.5)
    assert_eq!(
        load_alignment([3f64.sqrt(), 0.0, 1.0]).unwrap().quality,
        Quality::Good
    );
}

// orient.test.js:58
#[test]
fn load_alignment_a_zero_length_direction_is_null() {
    assert!(load_alignment([0.0, 0.0, 0.0]).is_none());
}

// orient.test.js:97
#[test]
fn suggest_strength_pose_lays_the_load_into_the_layer_plane_on_a_seated_pose() {
    // A tall square post pulled along its own long axis.
    let topo = block_topo(0.0, 20.0, 0.0, 20.0, 0.0, 100.0);
    let pose = suggest_strength_pose(&topo, [0.0, 0.0, 1.0], 45.0)
        .expect("a strictly better printable pose should exist for an axial pull on a post");
    assert!(
        pose.bed_area >= 1.0,
        "the recommended pose must sit on the bed, not balance on a tip"
    );
    assert!(
        pose.cross < 0.5,
        "the load should end up in-plane (cross {})",
        pose.cross
    );
    assert_eq!(pose.quality, Quality::Good);
}

// orient.test.js:109
#[test]
fn suggest_strength_pose_an_already_in_plane_load_needs_no_turn() {
    let topo = block_topo(0.0, 100.0, 0.0, 80.0, 0.0, 8.0);
    let pose = suggest_strength_pose(&topo, [1.0, 0.0, 0.0], 45.0);
    assert!(
        pose.is_none() || pose.unwrap().cross < 0.5,
        "must not turn a flat slab into a worse-aligned pose"
    );
}

// --- own tests ---------------------------------------------------------------------------------

#[test]
fn rot_from_to_maps_a_onto_b_and_handles_the_degenerate_cases() {
    let down = [0.0, 0.0, -1.0];
    for a in [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.6, 0.0, 0.8],
    ] {
        let r = rot_from_to(a, down);
        let m = [
            r[0] * a[0] + r[3] * a[1] + r[6] * a[2],
            r[1] * a[0] + r[4] * a[1] + r[7] * a[2],
            r[2] * a[0] + r[5] * a[1] + r[8] * a[2],
        ];
        for k in 0..3 {
            assert_close(m[k], down[k], 1e-12, "a maps onto b");
        }
    }
    assert_eq!(
        rot_from_to(down, down),
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    );
    // antiparallel: a half turn
    let r = rot_from_to([0.0, 0.0, 1.0], down);
    assert_close(r[8], -1.0, 1e-12, "z flips");
}

#[test]
fn candidate_downs_always_has_the_axes_and_no_near_duplicates() {
    let topo = block_topo(0.0, 20.0, 0.0, 30.0, 0.0, 10.0);
    let d = candidate_downs(&topo, 12);
    assert_eq!(d.len(), 6, "a box: its six faces are the six axes");
    for (i, a) in d.iter().enumerate() {
        for b in &d[..i] {
            assert!(a[0] * b[0] + a[1] * b[1] + a[2] * b[2] <= 0.996);
        }
    }
    let _ = analyze(&topo, 45.0, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn layer_verdict_buckets_at_the_five_percent_band() {
    // eps = 0.05 * max(1, span)
    assert_eq!(layer_verdict(&size(100.0, 50.0, 95.0)), Posture::Weak);
    assert_eq!(layer_verdict(&size(100.0, 50.0, 94.9)), Posture::Mixed);
    assert_eq!(layer_verdict(&size(100.0, 50.0, 52.5)), Posture::Strong);
    assert_eq!(layer_verdict(&size(0.1, 0.1, 0.1)), Posture::Weak);
}
