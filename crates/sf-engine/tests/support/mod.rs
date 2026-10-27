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
