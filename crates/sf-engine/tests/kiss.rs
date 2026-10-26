//! Tests of `sf_engine::kiss::kiss_ends`: the three scenarios of upstream `tests/kiss.test.js`
//! that need only `build_topology` (the wedge scenario needs fins, M3), plus the option rules.

use sf_engine::kiss::{KissEnds, KissOpts, kiss_ends};
use sf_engine::overhangs::{Topology, build_topology};
use sf_engine::types::{IDENTITY3, Xyz};

const O: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

/// `block(x0, x1, y0, y1, z0, z1)` (tests/_util.js) as a flat soup, f32-rounded like the
/// Float32Array the JS stores it in.
fn block(b: [f64; 6]) -> Vec<f64> {
    let [x0, x1, y0, y1, z0, z1] = b;
    let p = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let t: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [3, 0, 4],
        [3, 4, 7],
    ];
    t.iter()
        .flat_map(|tri| tri.iter().flat_map(|&i| p[i]))
        .map(|v| v as f32 as f64)
        .collect()
}

fn topo_of(boxes: &[[f64; 6]]) -> Topology {
    build_topology(boxes.iter().flat_map(|b| block(*b)).collect())
}

fn ends(t: &Topology) -> KissEnds {
    // a 0.6-layer tine at the origin biting +Y: reach 1.5, so the old far probe sat at 3.0
    let opts = KissOpts {
        half: 0.25,
        back: 0.3,
        reach: 1.5,
        ..Default::default()
    };
    kiss_ends(t, &IDENTITY3, &O, 0.0, 0.0, 0.0, 1.0, 4.6, 5.2, &opts)
}

fn all(e: &KissEnds) -> [f64; 4] {
    [e.bot[0], e.bot[1], e.top[0], e.top[1]]
}

// tests/kiss.test.js:19
#[test]
fn a_part_thinner_than_the_far_probe_still_stops_the_tine_at_its_face() {
    // solid from y 0.5 to 1.3: both the old 3.0 and 1.5 probes land in air past it
    for d in all(&ends(&topo_of(&[[-5.0, 5.0, 0.5, 1.3, 0.0, 10.0]]))) {
        assert!((d - 0.51).abs() <= 1e-3, "end {d}");
    }
}

// tests/kiss.test.js:24
#[test]
fn a_thin_near_piece_stops_the_tine_not_a_block_beyond_it() {
    let t = topo_of(&[
        [-5.0, 5.0, 0.5, 0.7, 0.0, 10.0],
        [-5.0, 5.0, 2.0, 10.0, 0.0, 10.0],
    ]);
    for d in all(&ends(&t)) {
        assert!((d - 0.51).abs() <= 1e-3, "end {d}");
    }
}

// tests/kiss.test.js:29
#[test]
fn no_solid_within_reach_keeps_the_full_reach_and_solid_behind_the_seed_pulls_back() {
    for d in all(&ends(&topo_of(&[[-5.0, 5.0, 4.0, 10.0, 0.0, 10.0]]))) {
        assert!((d - 1.5).abs() <= 1e-9, "bare {d}");
    }
    for d in all(&ends(&topo_of(&[[-5.0, 5.0, -1.0, 10.0, 0.0, 10.0]]))) {
        assert!((d + 0.25).abs() <= 1e-9, "behind {d}");
    }
}

#[test]
fn contact_offset_moves_the_end_and_is_clamped() {
    let t = topo_of(&[[-5.0, 5.0, 0.5, 1.3, 0.0, 10.0]]);
    let at = |c: f64| {
        let opts = KissOpts {
            half: 0.25,
            back: 0.3,
            reach: 1.5,
            contact_offset: c,
        };
        kiss_ends(&t, &IDENTITY3, &O, 0.0, 0.0, 0.0, 1.0, 4.6, 5.2, &opts).bot[0]
    };
    let flush = at(0.0);
    assert!((flush - 0.5).abs() < 1e-3); // flush with the face
    assert!((at(0.1) - (flush - 0.1)).abs() < 1e-3); // positive leaves air
    assert!((at(-0.1) - (flush + 0.1)).abs() < 1e-3); // negative bites in
    assert_eq!(at(5.0), at(0.2)); // clamped to +-0.2
    assert_eq!(at(-5.0), at(-0.2));
    assert_eq!(at(f64::NAN), at(-0.01)); // not finite: the default
    assert_eq!(at(f64::INFINITY), at(-0.01));
    assert_eq!(KissOpts::default().contact_offset, -0.01);
}
