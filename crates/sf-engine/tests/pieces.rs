//! Tests of `sf_engine::pieces::floating_pieces`: the scenarios of upstream `tests/floating.test.js`
//! that need only the topology and `analyze` (the `buildFins` assertions need fins, M3).

use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::pieces::floating_pieces;
use sf_engine::types::IDENTITY3;

fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Vec<[f64; 3]> {
    vec![a, b, c, a, c, d]
}

/// tests/floating.test.js:8
fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    [
        quad(v[0], v[3], v[2], v[1]),
        quad(v[4], v[5], v[6], v[7]),
        quad(v[0], v[1], v[5], v[4]),
        quad(v[2], v[3], v[7], v[6]),
        quad(v[1], v[2], v[6], v[5]),
        quad(v[0], v[4], v[7], v[3]),
    ]
    .concat()
}

/// tests/floating.test.js:47 -- the six faces of a block wound INWARD (each triangle's 2nd
/// and 3rd vertices swapped).
fn cavity(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
    let mut t = block(x0, x1, y0, y1, z0, z1);
    for i in (0..t.len()).step_by(3) {
        t.swap(i + 1, i + 2);
    }
    t
}

fn topo_of(tris: &[Vec<[f64; 3]>]) -> Topology {
    build_topology(
        tris.iter()
            .flatten()
            .flatten()
            .map(|&v| v as f32 as f64)
            .collect(),
    )
}

fn floating(topo: &Topology) -> Vec<sf_engine::pieces::FloatingPiece> {
    floating_pieces(topo, &analyze(topo, 45.0, &IDENTITY3), &IDENTITY3)
}

// tests/floating.test.js:22
#[test]
fn a_piece_that_starts_in_midair_is_found_with_its_drop() {
    let topo = topo_of(&[
        block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0),
        block(-15.0, 15.0, -10.0, 10.0, 13.0, 20.0),
    ]);
    let f = floating(&topo);
    assert_eq!(f.len(), 1);
    assert!((f[0].drop - 5.0).abs() < 1e-6, "drop {}", f[0].drop);
    assert_eq!(f[0].faces, 12);
    assert_eq!(f[0].lowest[2], 13.0);
}

// tests/floating.test.js:31
#[test]
fn over_bare_plate_the_drop_is_the_height_one_piece_or_a_stack_is_fine() {
    let beside = topo_of(&[
        block(-15.0, -5.0, -10.0, 10.0, 0.0, 8.0),
        block(5.0, 15.0, -10.0, 10.0, 6.0, 12.0),
    ]);
    let f = floating(&beside);
    assert!(f.len() == 1 && (f[0].drop - 6.0).abs() < 1e-6, "{f:?}");

    let one = topo_of(&[block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0)]);
    assert!(floating(&one).is_empty(), "a single piece never floats");

    // a piece 0.1 mm over another rests on it (print-in-place gap)
    let stack = topo_of(&[
        block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0),
        block(-10.0, 10.0, -5.0, 5.0, 8.1, 14.0),
    ]);
    assert!(floating(&stack).is_empty());
    // 0.5 mm over it does not
    let gap = topo_of(&[
        block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0),
        block(-10.0, 10.0, -5.0, 5.0, 8.5, 14.0),
    ]);
    let f = floating(&gap);
    assert!(f.len() == 1 && (f[0].drop - 0.5).abs() < 1e-5, "{f:?}");
}

// tests/floating.test.js:53
#[test]
fn a_hollow_part_is_one_piece_its_sealed_void_is_not_flagged() {
    let hollow = topo_of(&[
        block(-15.0, 15.0, -15.0, 15.0, 0.0, 30.0),
        cavity(-10.0, 10.0, -10.0, 10.0, 5.0, 25.0),
    ]);
    assert!(floating(&hollow).is_empty());
}

// tests/floating.test.js:59
#[test]
fn a_loose_piece_inside_a_void_is_still_found() {
    let topo = topo_of(&[
        block(-15.0, 15.0, -15.0, 15.0, 0.0, 30.0),
        cavity(-10.0, 10.0, -10.0, 10.0, 5.0, 25.0),
        block(-4.0, 4.0, -4.0, 4.0, 8.0, 14.0),
    ]);
    let f = floating(&topo);
    assert!(f.len() == 1 && (f[0].drop - 3.0).abs() < 1e-6, "{f:?}");
}

#[test]
fn two_pieces_on_the_plate_and_one_in_the_air() {
    // two pieces that both reach the plate never float; a third hanging over bare plate does
    let on_plate = topo_of(&[
        block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0),
        block(20.0, 30.0, 0.0, 10.0, 0.0, 10.0),
    ]);
    assert!(floating(&on_plate).is_empty());
    let with_air = topo_of(&[
        block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0),
        block(20.0, 30.0, 0.0, 10.0, 0.0, 10.0),
        block(40.0, 50.0, 0.0, 10.0, 5.0, 10.0),
    ]);
    let f = floating(&with_air);
    assert!(f.len() == 1 && (f[0].drop - 5.0).abs() < 1e-6);
}

#[test]
fn many_stacked_pieces_are_found_in_one_pass() {
    // tests/floating.test.js:67 at 12 x 8: posts each carrying a cap 0.1 mm above it
    let mut tris = Vec::new();
    for i in 0..12 {
        for j in 0..8 {
            let (x, y) = (i as f64 * 3.0, j as f64 * 3.0);
            tris.push(block(x, x + 2.0, y, y + 2.0, 0.0, 4.0));
            tris.push(block(x, x + 2.0, y, y + 2.0, 4.1, 5.0));
        }
    }
    assert!(floating(&topo_of(&tris)).is_empty());
}
