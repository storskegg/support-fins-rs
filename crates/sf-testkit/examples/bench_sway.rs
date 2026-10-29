//! Sway click timing, `just bench-sway`: `sway_at_face` on a synthetic million-face tower (a tall box turned 30
//! degrees about Z, its sides finely tessellated, a ledge near the top so the rib's shrink loop runs).
//!
//! The tower stands in for the million-face scanned parts the web app's Sway click was slow on (GH #405 and the
//! lowestHit index / cross-rib crop commits). Reports the FIRST click (which builds the part's triangle index) and
//! the median of the rest, plus an FNV-1a hash of every click's output so an A/B run proves it built the same braces.
//! `BENCH_QUAD` (mm, default 0.1) sets the facet size, `BENCH_RUNS` the click count.
use std::time::Instant;

use sf_engine::config::Config;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::sway::{Avoid, SwayOpts, face_is_upright, sway_at_face};

/// Quad edge in mm on the tower's sides.
const QUAD: f64 = 0.1;

fn push_quad(pos: &mut Vec<f64>, a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) {
    for p in [a, b, c, a, c, d] {
        pos.extend(p);
    }
}

fn tower(w: f64, depth: f64, h: f64, quad: f64) -> Vec<f64> {
    let (s, c) = 30f64.to_radians().sin_cos();
    let turn = |x: f64, y: f64, z: f64| [x * c - y * s, x * s + y * c, z];
    // the shaft's outline at height z: a ledge sticks out over the last 8 mm
    let ledge = |z: f64| if z > h - 8.0 { 3.0 } else { 0.0 };
    let mut pos = vec![];
    let nz = (h / quad).ceil() as usize;
    let sides = [
        ([0.0, 0.0], [w, 0.0]),
        ([w, 0.0], [w, depth]),
        ([w, depth], [0.0, depth]),
        ([0.0, depth], [0.0, 0.0]),
    ];
    for (p, q) in sides {
        let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        let nu = (len / quad).ceil() as usize;
        let at = |i: usize, j: usize| {
            let (t, z) = (i as f64 / nu as f64, j as f64 / nz as f64 * h);
            let (dx, dy) = ((q[0] - p[0]) / len, (q[1] - p[1]) / len);
            // outward is to the right of the walk direction; a little ripple keeps the facets distinct
            let o = ledge(z) + 0.05 * (z * 0.9).sin();
            turn(
                p[0] + (q[0] - p[0]) * t + dy * o,
                p[1] + (q[1] - p[1]) * t - dx * o,
                z,
            )
        };
        for j in 0..nz {
            for i in 0..nu {
                push_quad(
                    &mut pos,
                    at(i, j),
                    at(i + 1, j),
                    at(i + 1, j + 1),
                    at(i, j + 1),
                );
            }
        }
    }
    let corners = |z: f64| {
        [
            turn(0.0, 0.0, z),
            turn(w, 0.0, z),
            turn(w, depth, z),
            turn(0.0, depth, z),
        ]
    };
    let (b, t) = (corners(0.0), corners(h));
    push_quad(&mut pos, b[0], b[3], b[2], b[1]);
    push_quad(&mut pos, t[0], t[1], t[2], t[3]);
    pos
}

fn main() {
    let clicks: usize = std::env::var("BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    let quad: f64 = std::env::var("BENCH_QUAD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(QUAD);
    let cfg = Config::default();
    let pos = tower(30.0, 20.0, 80.0, quad);
    let rot = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let topo = build_topology(pos);
    let res = analyze(&topo, 45.0, &rot);
    let n = topo.n_faces;
    // clicks: upright faces spread over the sides (stride), print-space centroid
    let upright: Vec<usize> = (0..n)
        .filter(|&f| face_is_upright(&topo, &rot, f, &cfg))
        .collect();
    let stride = (upright.len() / clicks).max(1);
    let mut times = vec![];
    let mut hash = 0xcbf29ce484222325u64;
    let mut ok = 0;
    for &f in upright.iter().step_by(stride).take(clicks) {
        let p = &topo.pos[f * 9..f * 9 + 9];
        let c = [
            (p[0] + p[3] + p[6]) / 3.0 + res.offset.x,
            (p[1] + p[4] + p[7]) / 3.0 + res.offset.y,
            (p[2] + p[5] + p[8]) / 3.0 + res.offset.z,
        ];
        let opts = SwayOpts {
            cross_above: Some(10.0),
            ..Default::default()
        };
        let t = Instant::now();
        let r = sway_at_face(&topo, &res, &rot, f, c, &opts, &Avoid::default(), &cfg);
        times.push(t.elapsed().as_secs_f64() * 1e3);
        let mut eat = |x: f64| hash = (hash ^ x.to_bits()).wrapping_mul(0x100000001b3);
        match &r {
            Ok(rib) => {
                ok += 1;
                eat(rib.height);
                eat(rib.cross as f64);
                rib.tris.iter().for_each(|v| v.iter().for_each(|&x| eat(x)));
            }
            Err(e) => e.bytes().for_each(|b| eat(b as f64)),
        }
    }
    let first = times[0];
    let mut rest = times[1..].to_vec();
    rest.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "sway_at_face on {n} triangles ({} upright): {} clicks ({ok} stood): first {first:.1} ms, median of the rest {:.1} ms, max {:.1} ms, out {hash:016x}",
        upright.len(),
        times.len(),
        rest[rest.len() / 2],
        rest[rest.len() - 1]
    );
}
