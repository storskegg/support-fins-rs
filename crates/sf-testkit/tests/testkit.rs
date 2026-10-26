//! Unit tests for the `sf-testkit` helpers (Rust-side; the helpers' parity with the JS is in
//! `parity_testkit.rs`).

use sf_testkit::*;

fn verts(soup: &[f64]) -> Vec<[f64; 3]> {
    soup.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn volume(tris: &[[f64; 3]]) -> f64 {
    let mut v = 0.0;
    for t in tris.chunks(3) {
        let (p, q, r) = (t[0], t[1], t[2]);
        v += p[0] * (q[1] * r[2] - q[2] * r[1]) - p[1] * (q[0] * r[2] - q[2] * r[0])
            + p[2] * (q[0] * r[1] - q[1] * r[0]);
    }
    v / 6.0
}

#[test]
fn block_is_closed_and_oriented_with_positive_volume() {
    let b = verts(&block(-1.0, 2.0, 0.0, 3.0, 1.0, 5.0));
    assert_eq!(b.len(), 36);
    assert!(is_closed(&b));
    assert!(is_oriented(&b));
    assert_close(volume(&b), 3.0 * 3.0 * 4.0, 1e-9, "volume");
}

#[test]
fn block_values_round_to_f32() {
    let b = block(0.1, 1.0, 0.0, 1.0, 0.0, 1.0);
    assert_eq!(b[0], (0.1f32) as f64);
    assert_ne!(b[0], 0.1);
}

#[test]
fn flipped_face_is_closed_but_not_oriented() {
    let mut b = verts(&block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0));
    // reverse the winding of the first triangle
    b.swap(1, 2);
    assert!(is_closed(&b));
    assert!(!is_oriented(&b));
}

#[test]
fn inside_out_block_has_balanced_edges_but_negative_volume() {
    let mut b = verts(&block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0));
    for t in b.chunks_mut(3) {
        t.swap(1, 2);
    }
    assert!(is_closed(&b));
    assert!(!is_oriented(&b));
}

#[test]
fn missing_face_is_not_closed() {
    let b = verts(&block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0));
    assert!(!is_closed(&b[3..]));
    assert!(is_closed(&[]));
}

#[test]
fn degenerate_edges_are_skipped_by_is_oriented() {
    // a zero-length edge (a == b) is skipped, so a lone degenerate triangle has no edges left
    let p = [1.0, 2.0, 3.0];
    assert!(!is_oriented(&[p, p, p])); // volume 0 is not > 0
}

#[test]
fn vertex_keys_round_to_a_thousandth_and_merge_minus_zero() {
    let mut b = verts(&block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0));
    for v in b.iter_mut() {
        v[0] += 4e-4 * if v[0] > 0.5 { 1.0 } else { -1.0 }; // stays within a key
    }
    assert!(is_closed(&b));
    assert!(is_oriented(&b));
    // -0 and 0 key alike
    let mut c = verts(&block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0));
    for v in c.iter_mut() {
        if v[2] == 0.0 {
            v[2] = -0.0;
        }
    }
    assert!(is_closed(&c) && is_oriented(&c));
}

#[test]
fn edge_order_follows_string_comparison_not_numbers() {
    // keys "10,0,0" and "9,0,0" compare as "10.." < "9.." in JS strings (numerically 10 > 9).
    // A closed tetrahedron-like pair of triangles needs the same orientation either way, so
    // check the sign directly: the edge 9->10 walks "descending" as strings.
    let p = [0.009, 0.0, 0.0];
    let q = [0.010, 0.0, 0.0];
    let r = [0.0, 1.0, 0.0];
    let s = [0.0, 0.0, 1.0];
    // tetra p q r s with consistent outward winding (vol > 0 when oriented): faces
    // (p,r,q) (p,q,s) (q,r,s) (p,s,r)
    let t = [p, r, q, p, q, s, q, r, s, p, s, r];
    assert!(is_closed(&t));
    // Whatever the vertex order, orientation is decided by the string order consistently.
    assert_eq!(is_oriented(&t), volume(&t) > 0.0);
}

#[test]
fn tilted_block_sits_on_z0_and_stays_closed() {
    let s = tilted_block_soup(-5.0, 5.0, -5.0, 5.0, 0.0, 10.0, 45.0);
    let v = verts(&s);
    let bb = bbox(&v);
    assert_eq!(bb.lo[2], 0.0);
    assert!(is_closed(&v));
    assert_eq!(s.len(), 108);
    // every stored value is an f32 value
    assert!(s.iter().all(|&x| (x as f32) as f64 == x));
}

#[test]
fn tilted_block_at_zero_degrees_is_the_block() {
    let t = tilted_block_soup(0.0, 1.0, 0.0, 2.0, 0.0, 3.0, 0.0);
    assert_eq!(t, block(0.0, 1.0, 0.0, 2.0, 0.0, 3.0));
}

#[test]
fn holed_plate_has_the_bore_volume_and_t_junctions() {
    let s = holed_plate_soup(20.0, 15.0, 4.0, 5.0, 3.0);
    let v = verts(&s);
    // The frame strips meet the outer and bore walls at T-junctions, so the edge-count test
    // in `isClosed` fails for this soup in JS too (checked against the original).
    assert!(!is_closed(&v));
    assert_close(
        volume(&v),
        40.0 * 30.0 * 4.0 - 10.0 * 6.0 * 4.0,
        1e-3,
        "volume",
    );
    // 4+4 strips * 2 tris + 8 walls * 2 tris = 32 tris
    assert_eq!(v.len(), (8 * 2 + 8 * 2) * 3);
}

#[test]
fn rotations_at_known_angles() {
    let r = rot_x(0.0);
    assert_eq!(r, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -0.0, 1.0]);
    assert_eq!(r[7].to_bits(), (-0.0f64).to_bits());
    let r = rot_x(90.0);
    assert_close(r[4], 0.0, 1e-15, "c");
    assert_eq!(r[5], 1.0);
    assert_eq!(r[7], -1.0);
    let r = rot_y(90.0);
    assert_eq!(r[2], -1.0);
    assert_eq!(r[6], 1.0);
    assert_eq!(rot_y(0.0)[2].to_bits(), (-0.0f64).to_bits());
}

#[test]
fn assert_close_passes_within_tolerance_and_on_nan() {
    assert_close(1.0, 1.0 + 1e-9, 1e-6, "x");
    assert_close(1.0, 1.0 + 1e-6, 1e-6, "x"); // |a-b| == tol is not > tol (1e-6 rounds equal or below)
    assert_close(f64::NAN, 1.0, 1e-6, "nan never fails, as in JS");
}

#[test]
#[should_panic(expected = "gap: 1 vs 2 (tol 0.5)")]
fn assert_close_message_format() {
    assert_close(1.0, 2.0, 0.5, "gap");
}

#[test]
#[should_panic(expected = "not close: 1 vs 1e-7 (tol 0.5)")]
fn assert_close_default_message() {
    assert_close(1.0, 1e-7, 0.5, "");
}

// ---- pt_tri_dist2: one input per branch -----------------------------------

const A: [f64; 3] = [0.0, 0.0, 0.0];
const B: [f64; 3] = [4.0, 0.0, 0.0];
const C: [f64; 3] = [0.0, 4.0, 0.0];

#[test]
fn pt_tri_dist2_vertex_a_region() {
    assert_eq!(pt_tri_dist2([-1.0, -1.0, 0.0], A, B, C), 2.0);
}

#[test]
fn pt_tri_dist2_vertex_b_region() {
    assert_eq!(pt_tri_dist2([5.0, -1.0, 0.0], A, B, C), 2.0);
}

#[test]
fn pt_tri_dist2_vertex_c_region() {
    assert_eq!(pt_tri_dist2([-1.0, 5.0, 2.0], A, B, C), 1.0 + 1.0 + 4.0);
}

#[test]
fn pt_tri_dist2_edge_ab_region() {
    assert_eq!(pt_tri_dist2([2.0, -3.0, 0.0], A, B, C), 9.0);
}

#[test]
fn pt_tri_dist2_edge_ac_region() {
    assert_eq!(pt_tri_dist2([-3.0, 2.0, 0.0], A, B, C), 9.0);
}

#[test]
fn pt_tri_dist2_edge_bc_region() {
    // distance from (4,4,0) to the line x+y=4 is 4/sqrt2, squared 8
    assert_close(pt_tri_dist2([4.0, 4.0, 0.0], A, B, C), 8.0, 1e-12, "bc");
}

#[test]
fn pt_tri_dist2_face_interior() {
    assert_eq!(pt_tri_dist2([1.0, 1.0, 3.0], A, B, C), 9.0);
    assert_eq!(pt_tri_dist2([1.0, 1.0, 0.0], A, B, C), 0.0);
}

#[test]
fn pt_tri_dist2_degenerate_triangle() {
    // a == b == c: d1 = d2 = 0 hits the first branch only when p is "behind"; a coincident point is 0
    assert_eq!(pt_tri_dist2(A, A, A, A), 0.0);
}

// ---- bbox and tine_boxes --------------------------------------------------

#[test]
fn bbox_of_empty_is_infinite() {
    let b = bbox(&[]);
    assert_eq!(b.lo, [f64::INFINITY; 3]);
    assert_eq!(b.hi, [f64::NEG_INFINITY; 3]);
}

#[test]
fn bbox_uses_math_min_max_nan_and_signed_zero() {
    let b = bbox(&[[0.0, 1.0, 2.0], [-0.0, f64::NAN, 3.0]]);
    assert_eq!(b.lo[0].to_bits(), (-0.0f64).to_bits());
    assert_eq!(b.hi[0].to_bits(), 0.0f64.to_bits());
    assert!(b.lo[1].is_nan() && b.hi[1].is_nan());
    assert_eq!((b.lo[2], b.hi[2]), (2.0, 3.0));
}

/// A 36-vertex "box": the 12 triangles of `block`, so its bbox is exactly the arguments.
fn tbox(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
    verts(&block(x0, x1, y0, y1, z0, z1))
}

#[test]
fn tine_boxes_without_steps() {
    let mut out = tbox(0.0, 2.0, 0.0, 2.0, 5.0, 9.0);
    out.extend(tbox(3.0, 5.0, 0.0, 2.0, 5.0, 9.0));
    out.extend(tbox(6.0, 8.0, 0.0, 2.0, 5.0, 9.0));
    let r = tine_boxes(&out);
    assert_eq!(r.tines.len(), 3);
    assert!(r.steps.is_empty());
    assert_eq!(r.tines[1].lo, [3.0, 0.0, 5.0]);
    assert_eq!(r.tines[1].hi, [5.0, 2.0, 9.0]);
    assert_eq!(r.tines[1].verts.len(), 36);
}

#[test]
fn tine_boxes_with_steps() {
    // tine 0, its step (starts below, ends inside, inside footprint), tine 1 (no step), tine 2, step
    let mut out = tbox(0.0, 2.0, 0.0, 2.0, 5.0, 9.0);
    out.extend(tbox(0.5, 1.5, 0.5, 1.5, 0.0, 6.0));
    out.extend(tbox(3.0, 5.0, 0.0, 2.0, 5.0, 9.0));
    out.extend(tbox(6.0, 8.0, 0.0, 2.0, 5.0, 9.0));
    out.extend(tbox(6.0, 8.0, 0.0, 2.0, 1.0, 7.0));
    let r = tine_boxes(&out);
    assert_eq!(r.tines.len(), 3);
    assert_eq!(r.steps.len(), 2);
    assert_eq!(r.steps[0].tine, 0);
    assert_eq!(r.steps[0].b.hi, [1.5, 1.5, 6.0]);
    assert_eq!(r.steps[1].tine, 2);
    assert_eq!(r.tines[2].lo, [6.0, 0.0, 5.0]);
}

#[test]
fn tine_boxes_at_most_one_step_per_tine() {
    let mut out = tbox(0.0, 2.0, 0.0, 2.0, 5.0, 9.0);
    out.extend(tbox(0.5, 1.5, 0.5, 1.5, 0.0, 6.0));
    out.extend(tbox(0.5, 1.5, 0.5, 1.5, 0.0, 6.0)); // second look-alike is a tine, not a step
    let r = tine_boxes(&out);
    assert_eq!((r.tines.len(), r.steps.len()), (2, 1));
}

#[test]
fn tine_boxes_step_conditions_are_each_required() {
    let tine = tbox(0.0, 2.0, 0.0, 2.0, 5.0, 9.0);
    // not below the tine
    let not_below = tbox(0.5, 1.5, 0.5, 1.5, 5.0, 6.0);
    // does not end inside it (ends above the tine's top)
    let too_tall = tbox(0.5, 1.5, 0.5, 1.5, 0.0, 10.0);
    // ends at/below the tine's bottom
    let too_short = tbox(0.5, 1.5, 0.5, 1.5, 0.0, 5.0);
    // outside the tine footprint
    let outside = tbox(1.5, 2.5, 0.5, 1.5, 0.0, 6.0);
    for other in [not_below, too_tall, too_short, outside] {
        let mut out = tine.clone();
        out.extend(other);
        let r = tine_boxes(&out);
        assert_eq!((r.tines.len(), r.steps.len()), (2, 0));
    }
    // footprint tolerance is 1e-6
    let mut out = tine.clone();
    out.extend(tbox(0.0, 2.0 + 5e-7, 0.0, 2.0, 0.0, 6.0));
    // f32 storage in block() rounds 2+5e-7 to a nearby f32, still within 1e-6
    assert_eq!(tine_boxes(&out).steps.len(), 1);
}

#[test]
fn tine_boxes_ignores_a_trailing_partial_box() {
    let mut out = tbox(0.0, 2.0, 0.0, 2.0, 5.0, 9.0);
    out.extend(tbox(0.0, 2.0, 0.0, 2.0, 5.0, 9.0).into_iter().take(35));
    assert_eq!(tine_boxes(&out).tines.len(), 1);
    assert_eq!(tine_boxes(&[]).tines.len(), 0);
}

// ---- procedural solids ----------------------------------------------------

#[test]
fn circular_bore_defaults_match_bore_js() {
    let d = CircularBore::default();
    assert_eq!(
        (d.diameter, d.length, d.segments, d.half, d.axial_segments),
        (5.0, 30.0, 96, 10.0, 1)
    );
    let s = circular_bore(&d);
    assert_eq!(s.len(), 96 * 4 * 6 * 3);
    let v = verts(&s);
    assert!(is_closed(&v));
    assert!(is_oriented(&v));
}

#[test]
fn circular_bore_axial_segments_multiply_the_side_faces() {
    let s = circular_bore(&CircularBore {
        axial_segments: 3,
        ..Default::default()
    });
    assert_eq!(s.len(), 96 * (3 * 2 + 2) * 6 * 3);
    let v = verts(&s);
    assert!(is_closed(&v));
}

#[test]
fn recessed_body_defaults_match_circular_js() {
    let d = RecessedBody::default();
    assert_eq!(
        (d.segments, d.height, d.opening, d.bore, d.depth, d.outer),
        (120, 40.0, 25.0, 15.0, 10.0, 35.0)
    );
    let s = recessed_body(&d);
    assert_eq!(s.len(), 120 * 5 * 6 * 3);
    let v = verts(&s);
    assert!(is_closed(&v));
}

#[test]
fn recessed_body_yaw_and_translate() {
    let base = recessed_body(&RecessedBody {
        segments: 8,
        ..Default::default()
    });
    let moved = recessed_body(&RecessedBody {
        segments: 8,
        translate: [1.0, 2.0, 3.0],
        ..Default::default()
    });
    assert_eq!(base.len(), moved.len());
    for (a, b) in base.chunks(3).zip(moved.chunks(3)) {
        assert_close(b[0] - a[0], 1.0, 1e-12, "x");
        assert_close(b[1] - a[1], 2.0, 1e-12, "y");
        assert_close(b[2] - a[2], 3.0, 1e-12, "z");
    }
    let yawed = recessed_body(&RecessedBody {
        segments: 8,
        yaw: 45.0,
        ..Default::default()
    });
    // yaw of one segment (360/8 = 45 deg) maps the ring onto itself
    assert_close(
        yawed[0],
        base[5 * 6 * 3],
        1e-9,
        "segment 0 yawed 45 deg is segment 1",
    );
}

#[test]
fn model_paths_come_from_upstream() {
    let root = upstream_root();
    assert!(root.is_absolute(), "{}", root.display());
    assert_eq!(models_dir(), root.join("prototype/stress/models"));
}
