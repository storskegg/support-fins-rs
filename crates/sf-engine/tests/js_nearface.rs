//! Port of upstream `tests/nearface.test.js`: `nearestFaces` (web/prop/nearface.js) answers through a
//! bucket grid what tines.js used to answer by measuring every face: which faces are nearest a tine
//! seed, ties included, in face order. It has to be the SAME answer, bit for bit -- a tine's bite
//! heading comes from it, and a changed tie flips tines.

use sf_engine::jsnum;
use sf_engine::overhangs::Topology;
use sf_engine::prop::nearface::{nearest_faces, pt_tri_dist2};
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::load_model;

/// The full scan tines.js ran before the grid, verbatim in effect (nearface.test.js:9-27).
fn scan(topo: &Topology, rot: &Rot3, offset: &Xyz, px: f64, py: f64, pz: f64) -> Vec<usize> {
    let pos = &topo.pos;
    let n_f = topo.n_faces;
    let p = [px, py, pz];
    let seat = |o: usize| -> [f64; 3] {
        [
            rot[0] * pos[o] + rot[3] * pos[o + 1] + rot[6] * pos[o + 2] + offset.x,
            rot[1] * pos[o] + rot[4] * pos[o + 1] + rot[7] * pos[o + 2] + offset.y,
            rot[2] * pos[o] + rot[5] * pos[o + 1] + rot[8] * pos[o + 2] + offset.z,
        ]
    };
    let mut d2 = vec![0.0f64; n_f];
    let mut best = f64::INFINITY;
    for f in 0..n_f {
        let o = f * 9;
        d2[f] = pt_tri_dist2(p, seat(o), seat(o + 3), seat(o + 6));
        if d2[f] < best {
            best = d2[f];
        }
    }
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(best < f64::INFINITY) {
        return Vec::new();
    }
    let tol = best * 1e-9 + 1e-12;
    let mut out = Vec::new();
    for f in 0..n_f {
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(d2[f] > best + tol) {
            out.push(f);
        }
    }
    out
}

/// Deterministic pseudo-random (no Math.random: a failure must reproduce).
/// `() => ((s = Math.imul(s ^ (s >>> 15), 0x2c1b3c6d) + 0x9e3779b9 >>> 0) / 2 ** 32)`.
fn rng(seed: u32) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        let m = ((s ^ (s >> 15)) as i32).wrapping_mul(0x2c1b3c6d);
        s = (m as i64 + 0x9e3779b9) as u32;
        s as f64 / 4294967296.0
    }
}

/// column-major 3x3 (three.js Matrix3.elements), R = Ry * Rx
fn rot_xy(ax: f64, ay: f64) -> Rot3 {
    let (cx, sx, cy, sy) = (
        jsnum::cos(ax),
        jsnum::sin(ax),
        jsnum::cos(ay),
        jsnum::sin(ay),
    );
    [cy, 0.0, -sy, sy * sx, cx, cy * sx, sy * cx, -sx, cy * cx]
}

const MODELS: [&str; 9] = [
    "cube",
    "staircase",
    "torus",
    "sphere",
    "portal",
    "tube",
    "lbracket",
    "needle",
    "plate",
];

// nearface.test.js:50
#[test]
fn nearest_faces_matches_the_full_scan_exactly_ties_and_order_included() {
    let mut r = rng(215);
    let (mut checked, mut tied) = (0usize, 0usize);
    for name in MODELS {
        let topo = load_model(name);
        for rot in [
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            rot_xy(std::f64::consts::PI / 4.0, 0.0),
            rot_xy(0.6, 1.1),
        ] {
            let offset = Xyz {
                x: 3.25,
                y: -1.5,
                z: 12.0,
            };
            // seated bounds, to aim the probes at and around the part
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            let mut o = 0;
            while o < topo.pos.len() {
                let (x, y, z) = (topo.pos[o], topo.pos[o + 1], topo.pos[o + 2]);
                let p = [
                    rot[0] * x + rot[3] * y + rot[6] * z + offset.x,
                    rot[1] * x + rot[4] * y + rot[7] * z + offset.y,
                    rot[2] * x + rot[5] * y + rot[8] * z + offset.z,
                ];
                for k in 0..3 {
                    lo[k] = jsnum::min(lo[k], p[k]);
                    hi[k] = jsnum::max(hi[k], p[k]);
                }
                o += 3;
            }
            let mut probes: Vec<[f64; 3]> = Vec::new();
            // inside / around the box, and well outside it
            for _ in 0..60 {
                let mut p = [0.0; 3];
                for k in 0..3 {
                    p[k] = lo[k] + (hi[k] - lo[k]) * (r() * 1.4 - 0.2);
                }
                probes.push(p);
            }
            for _ in 0..10 {
                let mut p = [0.0; 3];
                for k in 0..3 {
                    p[k] = lo[k] + (hi[k] - lo[k]) * (r() * 6.0 - 2.5);
                }
                probes.push(p);
            }
            // on vertices and edge midpoints: where faces tie
            for _ in 0..30 {
                let f = jsnum::floor(r() * topo.n_faces as f64) as usize;
                let o = f * 9;
                let j = jsnum::floor(r() * 3.0) as usize * 3;
                let j2 = (j + 3) % 9;
                let v = |q: usize| -> [f64; 3] {
                    [
                        rot[0] * topo.pos[o + q]
                            + rot[3] * topo.pos[o + q + 1]
                            + rot[6] * topo.pos[o + q + 2]
                            + offset.x,
                        rot[1] * topo.pos[o + q]
                            + rot[4] * topo.pos[o + q + 1]
                            + rot[7] * topo.pos[o + q + 2]
                            + offset.y,
                        rot[2] * topo.pos[o + q]
                            + rot[5] * topo.pos[o + q + 1]
                            + rot[8] * topo.pos[o + q + 2]
                            + offset.z,
                    ]
                };
                let (a, b) = (v(j), v(j2));
                probes.push(a);
                probes.push([
                    (a[0] + b[0]) / 2.0,
                    (a[1] + b[1]) / 2.0,
                    (a[2] + b[2]) / 2.0,
                ]);
            }
            for [x, y, z] in probes {
                let want = scan(&topo, &rot, &offset, x, y, z);
                let got = nearest_faces(&topo, &rot, &offset, x, y, z);
                assert_eq!(
                    got,
                    want,
                    "{name} rot {rot:?} at {:?}: grid {got:?} vs scan {want:?}",
                    [x, y, z]
                );
                checked += 1;
                if want.len() > 1 {
                    tied += 1;
                }
            }
        }
    }
    assert!(
        tied > 50,
        "the probes should hit ties (corners, edges); only {tied} of {checked} did"
    );
}

// nearface.test.js:85
#[test]
fn nearest_faces_rebuilds_for_a_new_pose_of_the_same_part() {
    let topo = load_model("staircase");
    let a = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let b = rot_xy(0.9, 0.3);
    let off = Xyz {
        x: 0.0,
        y: 0.0,
        z: 5.0,
    };
    for rot in [a, b, a] {
        let want = scan(&topo, &rot, &off, 4.0, -2.0, 9.0);
        assert_eq!(
            nearest_faces(&topo, &rot, &off, 4.0, -2.0, 9.0),
            want,
            "stale grid"
        );
    }
}
