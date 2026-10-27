//! `FinsBuilt` (web/fins.js): the result object `buildFins` returns and the passes mutate.

use sf_engine::fins::build::*;
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::planes::PatchStats;

fn empty() -> FinsBuilt {
    FinsBuilt::empty(Seating {
        kind: SeatingKind::Face,
        span: 0.0,
        bed_area: 0.0,
    })
}

#[test]
fn empty_is_a_part_that_got_no_walls() {
    let b = empty();
    assert_eq!(b.mode, "prop");
    assert!(b.triangles.is_empty() && b.pad_triangles.is_empty() && b.fins.is_empty());
    assert!(b.props.is_empty() && b.pad.is_none() && b.served_regions.is_empty());
    assert_eq!(
        (b.tines, b.patch_count, b.unserved, b.volume),
        (0, 0, 0.0, 0.0)
    );
    assert_eq!(b.rejected, Rejected::default());
    assert_eq!(b.patch_stats, PatchStats::default());
    assert!(!b.sag_risk && b.tip.is_none());
    // the keys only a later stage writes are still absent
    assert!(b.brace_count.is_none() && b.prop_count.is_none() && b.fill.is_none());
    assert!(b.sway.is_none() && b.tall_ribs.is_none() && b.bare.is_none() && !b.curved_dropped);
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
