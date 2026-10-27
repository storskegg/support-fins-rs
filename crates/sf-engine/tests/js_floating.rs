//! Port of upstream `tests/floating.test.js`: PIECES IN MID-AIR (MorbidJ-hub, Sept 2026, from
//! 3DBenchy in the Fusion add-in): a piece that starts in mid-air (a cut clean through a wall, a
//! loose body) printed onto nothing with no word from the readout. floatingPieces finds it.
mod builds;
use builds::*;
use sf_engine::overhangs::analyze;
use sf_engine::pieces::floating_pieces;

type V3 = [f64; 3];
fn quad(a: V3, b: V3, c: V3, d: V3) -> Vec<V3> {
    vec![a, b, c, a, c, d]
}
fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<V3> {
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
fn topo_v(tris: &[V3]) -> std::sync::Arc<sf_engine::overhangs::Topology> {
    let flat: Vec<f64> = tris.iter().flatten().copied().collect();
    topo_of(f32s(&flat))
}
fn floating(
    topo: &std::sync::Arc<sf_engine::overhangs::Topology>,
) -> Vec<sf_engine::pieces::FloatingPiece> {
    floating_pieces(topo, &analyze(topo, 45.0, &I), &I)
}

// floating.test.js:22
#[test]
fn floating_a_piece_that_starts_in_mid_air_is_found_with_its_drop() {
    // a foot, and a bar hanging 5 mm over it with nothing joining them
    let topo = topo_v(
        &[
            block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0),
            block(-15.0, 15.0, -10.0, 10.0, 13.0, 20.0),
        ]
        .concat(),
    );
    let f = floating(&topo);
    assert!(f.len() == 1, "expected one floating piece, got {}", f.len());
    assert!(
        (f[0].drop - 5.0).abs() < 1e-6,
        "drop {}, expected 5",
        f[0].drop
    );
    let (_, b) = build(&topo, &I, &auto_opts(Some(0.5)));
    assert!(
        b.floating.len() == 1,
        "buildFins must carry floating pieces to the readout"
    );
}

// floating.test.js:32
#[test]
fn floating_over_bare_plate_the_drop_is_the_height_one_piece_or_a_stack_is_fine() {
    let beside = topo_v(
        &[
            block(-15.0, -5.0, -10.0, 10.0, 0.0, 8.0),
            block(5.0, 15.0, -10.0, 10.0, 6.0, 12.0),
        ]
        .concat(),
    );
    let f = floating(&beside);
    assert!(
        f.len() == 1 && (f[0].drop - 6.0).abs() < 1e-6,
        "beside: {f:?}"
    );

    let one = topo_v(&block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0));
    assert!(floating(&one).is_empty(), "a single piece never floats");

    let stack = topo_v(
        &[
            block(-15.0, 15.0, -10.0, 10.0, 0.0, 8.0),
            block(-10.0, 10.0, -5.0, 5.0, 8.1, 14.0),
        ]
        .concat(),
    );
    assert!(
        floating(&stack).is_empty(),
        "a piece 0.1 mm over another rests on it (print-in-place gap)"
    );
}

// the six faces of a block wound INWARD: the wall of a sealed cavity
fn cavity(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<V3> {
    let mut t = block(x0, x1, y0, y1, z0, z1);
    for i in (0..t.len()).step_by(3) {
        t.swap(i + 1, i + 2);
    }
    t
}

// floating.test.js:52
#[test]
fn floating_a_hollow_part_is_one_piece_its_sealed_void_is_not_flagged() {
    // a 30 mm box with a 20 mm void: two shells, but the inner one faces inward
    let hollow = topo_v(
        &[
            block(-15.0, 15.0, -15.0, 15.0, 0.0, 30.0),
            cavity(-10.0, 10.0, -10.0, 10.0, 5.0, 25.0),
        ]
        .concat(),
    );
    let f = floating(&hollow);
    assert!(f.is_empty(), "hollow box flagged as floating: {f:?}");
}

// floating.test.js:59
#[test]
fn floating_a_loose_piece_inside_a_void_is_still_found() {
    // the same hollow box with a block hanging 3 mm over the void's floor
    let topo = topo_v(
        &[
            block(-15.0, 15.0, -15.0, 15.0, 0.0, 30.0),
            cavity(-10.0, 10.0, -10.0, 10.0, 5.0, 25.0),
            block(-4.0, 4.0, -4.0, 4.0, 8.0, 14.0),
        ]
        .concat(),
    );
    let f = floating(&topo);
    assert!(
        f.len() == 1 && (f[0].drop - 3.0).abs() < 1e-6,
        "loose piece: {f:?}"
    );
}

// floating.test.js:67
#[test]
fn floating_1800_stacked_pieces_check_in_well_under_a_second() {
    // a 60 x 30 grid of posts, each carrying a cap 0.1 mm above it: 3,600 pieces, 1,800 of them
    // lifted, none floating. Testing every lifted piece against every triangle took ~1.5 s here
    // (8 s on an 800-piece, 1M-triangle part).
    let mut tris: Vec<V3> = Vec::new();
    for i in 0..60 {
        for j in 0..30 {
            let (x, y) = (i as f64 * 3.0, j as f64 * 3.0);
            tris.extend(block(x, x + 2.0, y, y + 2.0, 0.0, 4.0));
            tris.extend(block(x, x + 2.0, y, y + 2.0, 4.1, 5.0));
        }
    }
    let topo = topo_v(&tris);
    let res = analyze(&topo, 45.0, &I);
    let t0 = std::time::Instant::now();
    let f = floating_pieces(&topo, &res, &I);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(f.is_empty(), "{} caps flagged", f.len());
    assert!(ms < 500.0, "floatingPieces took {ms:.0} ms");
}
