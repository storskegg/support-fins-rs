//! Port of upstream `tests/fins.test.js`: per-fin removal data model: every auto fin now carries its
//! triangle segment(s) (triRanges, vertex-indexed into built.triangles) so the UI can address and drop
//! an individual fin. These tests pin the invariants the click-to-remove feature relies on -- that the
//! ranges are well-formed, cover the whole fin mesh exactly once, and that filtering by them excludes
//! exactly the removed fin's triangles. (Model-free: a tilted plate with a bore yields >=2 flanking
//! fins, same fixture as perp_hole.test.js.)
mod builds;
use builds::*;
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt, FinsOpts};
use sf_engine::types::V4;
use sf_testkit::{holed_plate_topo, is_closed, rot_x};
use std::sync::Arc;

// The same build the UI runs in Auto mode.
fn build_plate() -> FinsBuilt {
    let topo = Arc::new(holed_plate_topo(40.0, 30.0, 4.0, 9.0, 9.0));
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        ..Default::default()
    };
    build(&topo, &rot_x(45.0), &opts).1
}

/// Flatten one fin's triRanges into a vertex list (mirrors ui/remove.js filteredTriangles for a
/// single record).
fn fin_verts(triangles: &[V4], fin: &FinRecord) -> Vec<V4> {
    let mut out = Vec::new();
    for [lo, hi] in &fin.tri_ranges {
        out.extend_from_slice(&triangles[*lo..*hi]);
    }
    out
}

// fins.test.js:28
#[test]
fn per_fin_every_auto_fin_carries_tri_ranges_id_kind_line() {
    let b = build_plate();
    assert!(b.fins.len() >= 2, "expected >=2 fins, got {}", b.fins.len());
    for f in &b.fins {
        assert!(!f.tri_ranges.is_empty(), "fin {} has no triRanges", f.id);
        assert!(
            f.kind == FinKind::Prop || f.kind == FinKind::Wedge,
            "fin {} bad kind {:?}",
            f.id,
            f.kind
        );
        assert!(!f.line.is_empty(), "fin {} missing line", f.id);
    }
}

// fins.test.js:39
#[test]
fn per_fin_tri_ranges_are_in_bounds_non_overlapping_and_cover_every_triangle() {
    let b = build_plate();
    let n = b.triangles.len();
    // vertex-indexed coverage: mark every vertex index claimed by some fin's range.
    let mut claimed = vec![0u8; n];
    let mut claimed_count = 0;
    for f in &b.fins {
        for [lo, hi] in &f.tri_ranges {
            assert!(
                *hi <= n && lo < hi,
                "fin {} range out of bounds: [{lo},{hi}) of {n}",
                f.id
            );
            for t in *lo..*hi {
                assert!(
                    claimed[t] == 0,
                    "vertex {t} claimed by two fins (overlapping ranges)"
                );
                claimed[t] = 1;
                claimed_count += 1;
            }
        }
    }
    // Every fin triangle is accounted for -- no orphan geometry outside any fin.
    assert!(
        claimed_count == n,
        "triRanges cover {claimed_count} of {n} vertices -- gap in coverage"
    );
}

// fins.test.js:58
#[test]
fn per_fin_each_fin_is_itself_a_watertight_closed_solid() {
    let b = build_plate();
    for f in &b.fins {
        let verts = fin_verts(&b.triangles, f);
        assert!(
            verts.len().is_multiple_of(3),
            "fin {} vertex count {} not a triangle multiple",
            f.id,
            verts.len()
        );
        assert!(
            is_closed(&xyz(&verts)),
            "fin {} ({:?}) is not a closed solid in isolation",
            f.id,
            f.kind
        );
    }
}

// fins.test.js:67
#[test]
fn per_fin_filtering_out_one_fin_excludes_exactly_its_triangles_and_no_others() {
    let b = build_plate();
    let n = b.triangles.len();
    // Pick a fin to "remove" (not the first, to avoid lucky-edge cases).
    let k = 1.min(b.fins.len() - 1);
    let removed = &b.fins[k];
    // Replicate ui/remove.js filteredTriangles: walk records, skip removed, keep the rest.
    let mut kept: Vec<V4> = Vec::new();
    let mut removed_verts = std::collections::HashSet::new();
    for f in &b.fins {
        if f.id == removed.id {
            for [lo, hi] in &f.tri_ranges {
                for t in *lo..*hi {
                    removed_verts.insert(t);
                }
            }
        } else {
            for [lo, hi] in &f.tri_ranges {
                kept.extend_from_slice(&b.triangles[*lo..*hi]);
            }
        }
    }
    // The kept set is exactly the vertices NOT in the removed fin's ranges.
    assert!(
        kept.len() == n - removed_verts.len(),
        "kept {} vs expected {}",
        kept.len(),
        n - removed_verts.len()
    );
    // Sanity: the kept geometry is still watertight (removing a disjoint closed solid doesn't open
    // edges on the rest).
    assert!(
        is_closed(&xyz(&kept)),
        "kept fins are not watertight after removing one fin"
    );
    // And re-adding the removed fin's vertices back makes the whole mesh closed again.
    let mut all_back = kept.clone();
    all_back.extend(fin_verts(&b.triangles, removed));
    assert!(
        is_closed(&xyz(&all_back)),
        "full mesh not watertight after restoring the removed fin"
    );
}
