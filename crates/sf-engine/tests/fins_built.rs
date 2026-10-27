//! `FinsBuilt` (web/fins.js): the result object `buildFins` returns and the passes mutate.

use sf_engine::fins::build::*;
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::planes::PatchStats;
use sf_engine::prop::build::Skipped;

fn empty() -> FinsBuilt {
    FinsBuilt {
        triangles: vec![],
        pad_triangles: vec![],
        pad: None,
        mode: "prop".into(),
        fins: vec![],
        props: vec![],
        volume: 0.0,
        skipped: Skipped::default(),
        rejected: Rejected::default(),
        patch_count: 0,
        patch_stats: PatchStats::default(),
        tines: 0,
        served_regions: vec![],
        unserved: 0.0,
        sag_risk: false,
        seating: Seating {
            kind: SeatingKind::Face,
            span: 0.0,
            bed_area: 0.0,
        },
        tip: None,
        brace_count: None,
        prop_count: None,
        fill: None,
        curved_fill: None,
        curved_dropped: false,
        sway: None,
        small_hole_regions: 0,
        small_hole_mask: None,
        tall_ribs: None,
        floating: vec![],
        bare: None,
    }
}

#[test]
fn a_pass_mutates_the_built_object_in_place() {
    let mut b = empty();
    // what braceTallWalls does (fins/tall.js:104-134)
    b.tall_ribs = Some(0);
    b.triangles.push([0.0, 0.0, 0.0, 0.0]);
    b.tall_ribs = b.tall_ribs.map(|n| n + 2);
    assert_eq!((b.tall_ribs, b.triangles.len()), (Some(2), 1));
    assert_ne!(b, empty());
    assert_eq!(b.clone(), b);
}

#[test]
fn unserved_can_go_negative_as_in_js() {
    let mut b = empty();
    b.unserved = 0.0 - 2.0; // `(base.unserved ?? 0) - short.served.length`
    assert_eq!(b.unserved, -2.0);
}
