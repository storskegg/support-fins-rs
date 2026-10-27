//! Own tests for `sf_engine::solids` (no JS test file exists for it; the JS suite reaches it
//! through the wall builders, ported with M2/M3). Bit-for-bit replay against JS is
//! `tests/parity_solids.rs`.

use sf_engine::solids::{
    Radius, box_extrude, loft_extrude, ribbon, rounded_flange, rounded_polygon, stack_extrude,
};

type V3 = [f64; 3];

/// Signed volume of a triangle soup (positive when wound outward).
fn volume(tris: &[[f64; 4]]) -> f64 {
    tris.chunks(3)
        .map(|t| {
            let (a, b, c) = (t[0], t[1], t[2]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// Every undirected edge must be shared by exactly two triangles, in opposite directions.
fn is_closed(tris: &[[f64; 4]]) -> bool {
    use std::collections::HashMap;
    let key = |v: [f64; 4]| (v[0].to_bits(), v[1].to_bits(), v[2].to_bits());
    let mut e: HashMap<_, i32> = HashMap::new();
    for t in tris.chunks(3) {
        for i in 0..3 {
            let (a, b) = (key(t[i]), key(t[(i + 1) % 3]));
            *e.entry((a, b)).or_default() += 1;
            *e.entry((b, a)).or_default() -= 1;
        }
    }
    e.values().all(|&n| n == 0)
}

const SQUARE: [[f64; 2]; 4] = [[0.0, 0.0], [2.0, 0.0], [2.0, 3.0], [0.0, 3.0]];
const ID: fn(f64, f64, f64) -> V3 = |a, b, c| [a, b, c];

#[test]
fn box_extrude_is_a_closed_outward_box() {
    let mut out = Vec::new();
    box_extrude(&SQUARE, 0.0, 4.0, ID, &mut out);
    assert_eq!(out.len(), 3 * (2 * 4 + 2 * 2)); // 8 side + 4 cap triangles
    assert!(is_closed(&out));
    assert_eq!(volume(&out), 24.0);
}

#[test]
fn box_extrude_with_a_degenerate_polygon_does_not_panic() {
    let mut out = Vec::new();
    box_extrude(&[], 0.0, 1.0, ID, &mut out);
    box_extrude(&[[0.0, 0.0]], 0.0, 1.0, ID, &mut out);
    assert_eq!(out.len(), 6); // one point: a degenerate side pair, no caps
}

#[test]
fn loft_extrude_matches_box_extrude_for_equal_outlines() {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    box_extrude(&SQUARE, -1.0, 2.0, ID, &mut a);
    loft_extrude(&SQUARE, &SQUARE, -1.0, 2.0, ID, &mut b);
    assert_eq!(a, b);
}

#[test]
fn loft_extrude_leans_the_top() {
    let top: Vec<[f64; 2]> = SQUARE.iter().map(|p| [p[0] + 1.0, p[1]]).collect();
    let mut out = Vec::new();
    loft_extrude(&SQUARE, &top, 0.0, 1.0, ID, &mut out);
    assert!(is_closed(&out));
    assert!((volume(&out) - 6.0).abs() < 1e-12); // a sheared prism keeps its volume
}

#[test]
fn stack_extrude_counts_and_closes() {
    let polys: Vec<Vec<[f64; 2]>> = [1.0, 0.8, 0.5]
        .iter()
        .map(|k| SQUARE.iter().map(|p| [p[0] * k, p[1] * k]).collect())
        .collect();
    let mut out = Vec::new();
    stack_extrude(&polys, &[0.0, 1.0, 2.5], ID, &mut out);
    assert_eq!(out.len(), 3 * (2 * 4 * 2 + 2 * 2));
    assert!(is_closed(&out));
    assert!(volume(&out) > 0.0);
    // one ring is just the two caps (the JS loop over rings does not run)
    let mut one = Vec::new();
    stack_extrude(&polys[..1], &[0.0], ID, &mut one);
    assert_eq!(one.len(), 2 * 2 * 3); // two caps, two triangles each, no side rings
}

#[test]
fn ribbon_is_wound_outward() {
    // rings run clockwise seen from +z (the orientation the wall builders use: up one side,
    // down the other), which is what makes the shell face outward
    let ring = |z: f64| {
        SQUARE
            .iter()
            .rev()
            .map(|p| [p[0], p[1], z])
            .collect::<Vec<_>>()
    };
    let mut out = Vec::new();
    ribbon(&[ring(0.0), ring(1.0), ring(3.0)], &mut out);
    assert!(is_closed(&out));
    assert_eq!(volume(&out), 18.0);
}

#[test]
fn ribbon_of_rings_with_fewer_than_three_vertices_has_no_caps() {
    let mut out = Vec::new();
    ribbon(
        &[
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            vec![[0.0, 0.0, 1.0], [1.0, 0.0, 1.0]],
        ],
        &mut out,
    );
    assert_eq!(out.len(), 3 * 2 * 2);
}

#[test]
fn rounded_polygon_with_no_radius_keeps_the_corners() {
    for r in [0.0, -1.0, f64::NAN] {
        assert_eq!(
            rounded_polygon(&SQUARE, Radius::All(r), 8.0),
            SQUARE.to_vec(),
            "{r}"
        );
    }
    // radius past the array's end is undefined: that corner stays sharp
    let out = rounded_polygon(&SQUARE, Radius::Each(&[0.0, 0.0]), 8.0);
    assert_eq!(out, SQUARE.to_vec());
}

#[test]
fn rounded_polygon_rounds_inside_the_polygon() {
    let out = rounded_polygon(&SQUARE, Radius::All(0.5), 8.0);
    assert_eq!(out.len(), 4 * 9);
    for p in &out {
        assert!(p[0] >= -1e-12 && p[0] <= 2.0 + 1e-12 && p[1] >= -1e-12 && p[1] <= 3.0 + 1e-12);
    }
    // a huge radius is limited to 0.49 of the shorter neighbouring edge
    let big = rounded_polygon(&SQUARE, Radius::All(100.0), 8.0);
    assert!(big.iter().all(|p| p[0] > -1e-9 && p[0] < 2.0 + 1e-9));
    // only some corners rounded
    let some = rounded_polygon(&SQUARE, Radius::Each(&[0.5, 0.0, 0.5, 0.0]), 8.0);
    assert_eq!(some.len(), 2 * 9 + 2);
}

#[test]
fn rounded_polygon_skips_straight_and_repeated_corners() {
    let straight = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
    let out = rounded_polygon(&straight, Radius::All(0.2), 8.0);
    assert!(out.contains(&[1.0, 0.0])); // the straight-through corner (theta = pi) is kept as is
    let repeated = [[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
    let out = rounded_polygon(&repeated, Radius::All(0.2), 8.0);
    assert_eq!(out.iter().filter(|p| **p == [1.0, 0.0]).count(), 2); // da < 1e-7: pushed
}

#[test]
fn rounded_polygon_steps_set_the_arc_resolution() {
    let one = rounded_polygon(&SQUARE, Radius::All(0.5), 1.0);
    assert_eq!(one.len(), 4 * 2);
    let none = rounded_polygon(&SQUARE, Radius::All(0.5), 0.0);
    // steps = 0: j = 0 runs once, and turn * 0 / 0 is NaN; only the very first point is kept,
    // since a NaN distance is never > 1e-8
    assert_eq!(none.len(), 1);
    assert!(none[0][0].is_nan());
}

fn flange_section(x: f64, y: f64, sx: f64, sy: f64, half: f64) -> Vec<V3> {
    vec![
        [x + sx * half, y + sy * half, 0.0],
        [x + sx * half, y + sy * half, 0.6],
        [x - sx * half, y - sy * half, 0.6],
        [x - sx * half, y - sy * half, 0.0],
    ]
}

#[test]
fn rounded_flange_collapses_collinear_sections() {
    let line: Vec<_> = (0..6)
        .map(|i| flange_section(f64::from(i) * 2.0, 0.0, 0.0, 1.0, 3.0))
        .collect();
    let (mut all, mut ends) = (Vec::new(), Vec::new());
    rounded_flange(&line, &mut all);
    rounded_flange(&[line[0].clone(), line[5].clone()], &mut ends);
    assert_eq!(all, ends);
    assert!(!all.is_empty());
}

#[test]
fn rounded_flange_keeps_a_corner_and_adds_a_joint() {
    let mut bent: Vec<_> = (0..3)
        .map(|i| flange_section(f64::from(i) * 2.0, 0.0, 0.0, 1.0, 3.0))
        .collect();
    bent.extend((1..3).map(|i| flange_section(4.0, f64::from(i) * 2.0, 1.0, 0.0, 3.0)));
    let mut out = Vec::new();
    rounded_flange(&bent, &mut out);
    let mut straight = Vec::new();
    rounded_flange(&[bent[0].clone(), bent[2].clone()], &mut straight);
    // two segments + one joint ribbon is more than a single segment
    assert!(out.len() > straight.len());
}

#[test]
fn rounded_flange_with_one_section_emits_nothing() {
    let mut out = Vec::new();
    rounded_flange(&[flange_section(0.0, 0.0, 0.0, 1.0, 3.0)], &mut out);
    assert!(out.is_empty());
    rounded_flange(&[], &mut out);
    assert!(out.is_empty());
}
