//! Soup builders shared by the M2/A circular and coverage tests.
#![allow(dead_code)]
use sf_engine::overhangs::{Analysis, Topology};
use sf_engine::types::Rot3;

pub fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Vec<[f64; 3]> {
    vec![a, b, c, a, c, d]
}

pub fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
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

pub fn flat(tris: &[Vec<[f64; 3]>]) -> Vec<f64> {
    tris.iter().flatten().flatten().copied().collect()
}

/// tests/_circular.js `recessedBody` with its defaults (segments 120, height 40, opening 25,
/// bore 15, depth 10, outer 35) and a segment count / translation.
pub fn recessed_body(segments: usize, translate: [f64; 3]) -> Vec<f64> {
    let (height, opening, bore, depth, outer) = (40.0, 25.0, 15.0, 10.0, 35.0);
    let profile = [
        [outer, 0.0],
        [outer, height],
        [bore, height],
        [bore, depth],
        [opening, 0.0],
    ];
    let point = |p: [f64; 2], i: usize| -> [f64; 3] {
        let a = i as f64 * 2.0 * std::f64::consts::PI / segments as f64;
        [
            p[0] * a.cos() + translate[0],
            p[0] * a.sin() + translate[1],
            p[1] + translate[2],
        ]
    };
    let mut out = Vec::new();
    for i in 0..segments {
        for j in 0..profile.len() {
            let (p, q) = (profile[j], profile[(j + 1) % profile.len()]);
            let (a, b, c, d) = (point(p, i), point(p, i + 1), point(q, i + 1), point(q, i));
            out.extend(a);
            out.extend(b);
            out.extend(c);
            out.extend(a);
            out.extend(c);
            out.extend(d);
        }
    }
    out
}

/// tests/_bore.js `circularBore({ diameter })` (length 30, 96 segments, half 10, one axial segment).
pub fn circular_bore(diameter: f64) -> Vec<f64> {
    let (length, segments, half) = (30.0, 96usize, 10.0);
    let mut t: Vec<[f64; 3]> = Vec::new();
    let mut quad_ = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| {
        t.extend([a, b, c, a, c, d]);
    };
    let ring = |x: f64, outer: bool| -> Vec<[f64; 3]> {
        (0..segments)
            .map(|i| {
                let a = i as f64 * 2.0 * std::f64::consts::PI / segments as f64;
                let (c, s) = (a.cos(), a.sin());
                let r = if outer {
                    half / c.abs().max(s.abs())
                } else {
                    diameter / 2.0
                };
                [x, r * c, half + r * s]
            })
            .collect()
    };
    let (lo, li) = (ring(-length / 2.0, true), ring(-length / 2.0, false));
    let (ro, ri) = (ring(length / 2.0, true), ring(length / 2.0, false));
    for i in 0..segments {
        let j = (i + 1) % segments;
        let at = |v: [f64; 3], x: f64| [x, v[1], v[2]];
        let (x0, x1) = (-length / 2.0, length / 2.0);
        quad_(at(lo[i], x0), at(lo[j], x0), at(lo[j], x1), at(lo[i], x1));
        quad_(at(li[i], x0), at(li[i], x1), at(li[j], x1), at(li[j], x0));
        quad_(li[i], li[j], lo[j], lo[i]);
        quad_(ri[i], ro[i], ro[j], ri[j]);
    }
    t.iter().flatten().copied().collect()
}

pub fn seated_region(topo: &Topology, res: &Analysis, rot: &Rot3, gi: usize) -> Vec<f64> {
    let region = &res.regions[gi];
    let mut out = Vec::new();
    for &f in &region.faces {
        for i in (0..9).step_by(3) {
            let o = f * 9 + i;
            let (x, y, z) = (topo.pos[o], topo.pos[o + 1], topo.pos[o + 2]);
            out.push(rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x);
            out.push(rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y);
            out.push(rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z);
        }
    }
    out
}

/// A welded "net": a plate on an `n` x `n` grid of `cell`-mm cells, the `solid(i, j)` cells
/// forming bars; `bottom(i, j)` is the underside height at a grid vertex; a foot clear of the net
/// sets the plate. (The generator `parity/oracle/lattice.js` uses.)
pub fn net(
    n: usize,
    cell: f64,
    solid: &dyn Fn(usize, usize) -> bool,
    bottom: &dyn Fn(usize, usize) -> f64,
    top: f64,
) -> Vec<f64> {
    let mut t: Vec<[f64; 3]> = Vec::new();
    let b = |i: usize, j: usize| [i as f64 * cell, j as f64 * cell, bottom(i, j)];
    let u = |i: usize, j: usize| [i as f64 * cell, j as f64 * cell, top];
    let on = |i: i64, j: i64| {
        i >= 0 && j >= 0 && (i as usize) < n && (j as usize) < n && solid(i as usize, j as usize)
    };
    for j in 0..n {
        for i in 0..n {
            if !on(i as i64, j as i64) {
                continue;
            }
            t.extend(quad(b(i, j), b(i, j + 1), b(i + 1, j + 1), b(i + 1, j)));
            t.extend(quad(u(i, j), u(i + 1, j), u(i + 1, j + 1), u(i, j + 1)));
            let (ii, jj) = (i as i64, j as i64);
            if !on(ii - 1, jj) {
                t.extend(quad(b(i, j), u(i, j), u(i, j + 1), b(i, j + 1)));
            }
            if !on(ii + 1, jj) {
                t.extend(quad(
                    b(i + 1, j),
                    b(i + 1, j + 1),
                    u(i + 1, j + 1),
                    u(i + 1, j),
                ));
            }
            if !on(ii, jj - 1) {
                t.extend(quad(b(i, j), b(i + 1, j), u(i + 1, j), u(i, j)));
            }
            if !on(ii, jj + 1) {
                t.extend(quad(
                    b(i + 1, j + 1),
                    b(i, j + 1),
                    u(i, j + 1),
                    u(i + 1, j + 1),
                ));
            }
        }
    }
    t.extend(block(-8.0, -5.0, -8.0, -5.0, 0.0, 22.0));
    t.iter().flatten().map(|&v| v as f32 as f64).collect()
}

pub fn flat_bottom(_: usize, _: usize) -> f64 {
    20.0
}
