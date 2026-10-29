//! The pointer-ray and frame maths of `sf_engine::drawn` (three.js did these in the web). The geometry
//! of the hand-placed supports is checked against web/ui/walls.js in crates/sf-ffi/tests/draw.rs.
use std::collections::HashSet;

use sf_engine::drawn::{arc_points, auto_supports, ray_triangle, to_local, to_world};
use sf_engine::types::Xyz;

const A: [f64; 3] = [0.0, 0.0, 0.0];
const B: [f64; 3] = [10.0, 0.0, 0.0];
const C: [f64; 3] = [0.0, 10.0, 0.0];

#[test]
fn a_ray_meets_a_triangle_from_either_side() {
    let d = ray_triangle([2.0, 2.0, 5.0], [0.0, 0.0, -1.0], A, B, C).unwrap();
    assert!((d - 5.0).abs() < 1e-12);
    let d = ray_triangle([2.0, 2.0, -5.0], [0.0, 0.0, 1.0], A, B, C).unwrap();
    assert!((d - 5.0).abs() < 1e-12);
}

#[test]
fn a_ray_misses_outside_behind_or_parallel() {
    assert_eq!(
        ray_triangle([8.0, 8.0, 5.0], [0.0, 0.0, -1.0], A, B, C),
        None
    ); // outside the edge
    assert_eq!(
        ray_triangle([2.0, 2.0, 5.0], [0.0, 0.0, 1.0], A, B, C),
        None
    ); // pointing away
    assert_eq!(
        ray_triangle([2.0, 2.0, 5.0], [1.0, 0.0, 0.0], A, B, C),
        None
    ); // parallel to the plane
    assert_eq!(
        ray_triangle([-0.1, 2.0, 5.0], [0.0, 0.0, -1.0], A, B, C),
        None
    );
    // a degenerate triangle is never hit
    assert_eq!(
        ray_triangle([1.0, 0.0, 5.0], [0.0, 0.0, -1.0], A, B, [20.0, 0.0, 0.0]),
        None
    );
}

#[test]
fn local_and_world_are_inverses_for_a_rotation() {
    let (s, c) = 0.6f64.sin_cos();
    let rot = [c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0];
    let off = Xyz {
        x: 3.0,
        y: -4.0,
        z: 7.5,
    };
    let p = [1.5, -2.25, 9.0];
    let back = to_local(&rot, &off, to_world(&rot, &off, p));
    for i in 0..3 {
        assert!((back[i] - p[i]).abs() < 1e-12);
    }
    // the part's origin stands at the offset
    assert_eq!(to_world(&rot, &off, [0.0; 3]), [3.0, -4.0, 7.5]);
}

#[test]
fn no_auto_build_means_nothing_to_avoid() {
    let a = auto_supports(None, &HashSet::new());
    assert!(a.braces.is_empty() && a.walls.is_empty());
}

#[test]
fn the_band_of_three_in_line_is_the_straight_line() {
    let pts = arc_points([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 0.0, 0.0]);
    assert_eq!(pts, vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
    let arc = arc_points([0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [10.0, 8.0, 0.0]);
    assert!(arc.len() > 3 && arc.len() <= 129);
    assert_eq!(arc[0], [0.0, 0.0, 0.0]);
    assert_eq!(*arc.last().unwrap(), [20.0, 0.0, 0.0]);
}
