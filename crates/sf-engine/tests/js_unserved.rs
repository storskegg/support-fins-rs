//! Port of upstream `tests/unserved.test.js`: UNSERVED must count dropped overhangs, not hide them.
//! Credit is spatial (fins/wedges.js `unservedAfterWedges`).
mod builds;
use builds::*;
use sf_engine::overhangs::analyze;
use sf_engine::types::Rot3;
use sf_testkit::{load_model, rot_x, rot_y};
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

fn build(name: &str, rot: &Rot3, coverage: f64) -> f64 {
    let topo = Arc::new(load_model(name));
    let res = analyze(&topo, 45.0, rot);
    let b = sf_engine::fins::build::build_fins(
        &topo,
        &res,
        rot,
        &auto_opts(Some(coverage)),
        &sf_engine::config::Config::default(),
    );
    b.unserved
}

// unserved.test.js:17
#[test]
fn unserved_regions_no_support_reaches_are_reported_lowledge_x30y60_dense() {
    // Both overhang regions have no support vertex within reach (0% covered); two wedges
    // elsewhere used to cancel them out -> "1 unserved".
    let u = build("lowledge", &mul(&rot_y(60.0), &rot_x(30.0)), 1.0);
    assert!(
        u == 2.0,
        "dropped overhangs hidden: unserved {u}, expected 2"
    );
}

// :24
#[test]
fn unserved_a_region_a_wedge_stands_under_counts_as_served_plate_x60() {
    // The wedge covers the plate's whole underside but shares no faces with the overhang region.
    let u = build("plate", &rot_x(60.0), 0.5);
    assert!(u == 0.0, "wedged region reported unserved: {u}");
}
