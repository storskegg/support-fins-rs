//! Port of upstream `tests/bores.test.js`: bores get supported (Matthew's print tests, 2026-09-27,
//! reversing the old never-fin-a-bore rule). A part-attached wall stands on the bore's floor under
//! its ceiling, runs ALONG the bore so it pulls out an open end, and is flagged inBore for Suggest
//! orientation. A refusal is counted under its real reason -- there is no `bore` skip any more.
mod builds;
use builds::*;
use sf_engine::fins::build::FinsBuilt;
use sf_engine::overhangs::Topology;
use sf_engine::types::Rot3;
use sf_testkit::{holed_plate_topo, rot_y};
use std::sync::Arc;

// A 40 x 40 block, 30 thick, with a 10 x 10 bore through it; turned on its side (rotY 90) the
// bore runs along X, with a flat 30 x 10 ceiling 10 mm above its floor.
fn setup() -> (Arc<Topology>, Rot3, FinsBuilt) {
    let topo = Arc::new(holed_plate_topo(20.0, 20.0, 30.0, 5.0, 5.0));
    let rot = rot_y(90.0);
    let (_, built) = build(&topo, &rot, &auto_opts(None));
    (topo, rot, built)
}

// bores.test.js:16
#[test]
fn bores_the_bore_ceiling_gets_a_wall_on_the_bore_floor_flagged_in_bore() {
    let (_, _, built) = setup();
    let in_bore: Vec<_> = built
        .props
        .iter()
        .filter(|q| q.in_bore == Some(true))
        .collect();
    assert!(
        !in_bore.is_empty(),
        "no wall in the bore: {:?}",
        built.skipped
    );
    for w in in_bore {
        assert!(w.part_attached, "a bore wall must stand on the bore floor");
    }
}

// bores.test.js:22
#[test]
fn bores_the_wall_runs_along_the_bore_axis_so_it_slides_out_an_open_end() {
    let (_, _, built) = setup();
    for w in built.props.iter().filter(|q| q.in_bore == Some(true)) {
        let a = w.line[0];
        let b = w.line[w.line.len() - 1];
        let along = (b[0] - a[0]).abs();
        let across = (b[1] - a[1]).abs();
        assert!(
            along > 3.0 * across,
            "wall runs across the bore: dx {along:.1} dy {across:.1}"
        );
    }
}

// bores.test.js:30 -- the JS asserts `skipped` has no `bore` key; `Skipped` has no such field at all
// (a refusal is counted under its real reason), so this pins that the field set is the reasons.
#[test]
fn bores_no_bore_skip_reason_is_reported() {
    let (_, _, built) = setup();
    let s = format!("{:?}", built.skipped);
    assert!(
        !s.to_lowercase().contains("bore"),
        "skipped still carries bore: {s}"
    );
}
