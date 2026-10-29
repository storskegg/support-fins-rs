//! Port of upstream `tests/triindex.test.js`: Sway's `lowest_hit` goes through an XY triangle grid
//! (`triindex.rs`) instead of scanning every triangle. The grid may only skip triangles that cannot
//! touch the rib's box, so the answer must be bit-identical to the full scan, with and without the
//! rib's z bounds.

use sf_engine::sway::Frame;
use sf_engine::swayhit::{lowest_hit, lowest_hit_brute};
use sf_engine::tris::Tris;
use sf_testkit::{block_topo, load_model, tilted_block_topo};

type V3 = [f64; 3];

/// `rng(seed)` (tests/triindex.test.js:17-22): the same small deterministic generator.
fn rng(mut seed: u64) -> impl FnMut() -> f64 {
    move || {
        seed = (seed * 1664525 + 1013904223) % (1u64 << 32);
        seed as f64 / 4294967296.0
    }
}

struct Query {
    fr: Frame,
    u_lo: f64,
    u_hi: f64,
    s_lo: f64,
    s_hi: f64,
    z_lo: f64,
    z_hi: f64,
    // outline parameters: z0, z1, s0, slope, depth
    p: [f64; 5],
}

/// `randomQuery` (tests/triindex.test.js:25-41): a rib-shaped query.
fn random_query(rand: &mut impl FnMut() -> f64, lo: [f64; 3], hi: [f64; 3]) -> Query {
    let ang = rand() * std::f64::consts::PI * 2.0;
    let nh = (ang.cos(), ang.sin());
    let u = (-nh.1, nh.0);
    let cx = lo[0] + rand() * (hi[0] - lo[0]);
    let cy = lo[1] + rand() * (hi[1] - lo[1]);
    let uc = cx * u.0 + cy * u.1;
    let s0 = cx * nh.0 + cy * nh.1;
    let th = 1.0 + rand() * 4.0;
    let depth = 3.0 + rand() * 25.0;
    let slope = (rand() - 0.5) * 0.4;
    let z0 = rand() * (hi[2] - lo[2]) * 0.5;
    let z1 = z0 + 2.0 + rand() * (hi[2] - lo[2]);
    let s_in = |z: f64| s0 + slope * z;
    let s_out = |z: f64| s0 + slope * z + depth;
    Query {
        fr: Frame { nh, u },
        u_lo: uc - th / 2.0 - 0.3,
        u_hi: uc + th / 2.0 + 0.3,
        s_lo: s_in(z0).min(s_in(z1)) - 0.05,
        s_hi: s_out(z0).max(s_out(z1)) + 0.1,
        z_lo: z0,
        z_hi: z1,
        p: [z0, z1, s0, slope, depth],
    }
}

fn bounds(tris: &[f64]) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for t in tris.chunks(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(t[k]);
            hi[k] = hi[k].max(t[k]);
        }
    }
    (lo, hi)
}

fn check(name: &str, topo: sf_engine::overhangs::Topology) {
    let tris = Tris::new(topo.pos[..topo.n_faces * 9].to_vec());
    let (lo, hi) = bounds(&tris.v);
    // Boxes mostly over the part, some beside it, so both hits and misses are checked.
    let pad = [(hi[0] - lo[0]) * 0.2, (hi[1] - lo[1]) * 0.2, 0.0];
    let mut rand = rng(name.len() as u64 * 7919 + 1);
    let (mut hits, mut misses) = (0, 0);
    for n in 0..400 {
        let q = random_query(
            &mut rand,
            [lo[0] - pad[0], lo[1] - pad[1], lo[2] - pad[2]],
            [hi[0] + pad[0], hi[1] + pad[1], hi[2] + pad[2]],
        );
        let [z0, z1, s0, slope, depth] = q.p;
        let o0 = move |p: &V3| p[2] - z0;
        let o1 = move |p: &V3| z1 - p[2];
        let o2 = move |p: &V3| p[0] - ((s0 + slope * p[2]) - 0.05);
        let o3 = move |p: &V3| ((s0 + slope * p[2] + depth) + 0.1) - p[0];
        let outline: [&dyn Fn(&V3) -> f64; 4] = [&o0, &o1, &o2, &o3];
        let want = lowest_hit_brute(&tris.v, &q.fr, q.u_lo, q.u_hi, &outline);
        let got = lowest_hit(
            &tris,
            &q.fr,
            q.u_lo,
            q.u_hi,
            &outline,
            q.s_lo,
            q.s_hi,
            f64::NEG_INFINITY,
            f64::INFINITY,
        );
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "{name} query {n}: grid {got} vs scan {want}"
        );
        // with the z bounds sway.js passes: the per-triangle box test must not drop a crossing
        let got_z = lowest_hit(
            &tris, &q.fr, q.u_lo, q.u_hi, &outline, q.s_lo, q.s_hi, q.z_lo, q.z_hi,
        );
        assert_eq!(
            got_z.to_bits(),
            want.to_bits(),
            "{name} query {n}: grid+box {got_z} vs scan {want}"
        );
        if want == f64::INFINITY {
            misses += 1;
        } else {
            hits += 1;
        }
    }
    assert!(
        hits > 20 && misses > 20,
        "{name}: queries should include both hits ({hits}) and misses ({misses})"
    );
}

// triindex.test.js:63 (one JS test per scene)
#[test]
fn lowest_hit_through_the_grid_equals_the_full_scan() {
    for name in [
        "sphere",
        "torus",
        "lbracket",
        "staircase",
        "tshape",
        "lowledge",
    ] {
        check(name, load_model(name));
    }
    check("post", block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 150.0));
    check(
        "tilted",
        tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 100.0, 25.0),
    );
}
