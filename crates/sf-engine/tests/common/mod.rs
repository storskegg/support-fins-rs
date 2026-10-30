//! Shared by the M2/A prop tests: block soups and the region inputs `buildProps` makes.
#![allow(dead_code)]
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Topology};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::{Rot3, V4};

pub fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Vec<[f64; 3]> {
    vec![a, b, c, a, c, d]
}

/// `block(x0, x1, y0, y1, z0, z1)` as a flat f32-rounded soup.
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

pub fn soup(parts: &[Vec<[f64; 3]>]) -> Vec<f64> {
    parts
        .iter()
        .flatten()
        .flatten()
        .map(|&v| v as f32 as f64)
        .collect()
}

/// A cylinder lying along x, `n` facets round (closed, outward winding).
pub fn pipe(len: f64, r: f64, n: usize, x0: f64, y0: f64, z0: f64) -> Vec<[f64; 3]> {
    let ring = |x: f64| -> Vec<[f64; 3]> {
        (0..n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                [x, y0 + r * a.cos(), z0 + r * a.sin()]
            })
            .collect()
    };
    let (a, b) = (ring(x0), ring(x0 + len));
    let mut t = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        t.extend(quad(a[i], b[i], b[j], a[j]));
        t.extend([a[i], a[j], [x0, y0, z0]]);
        t.extend([b[j], b[i], [x0 + len, y0, z0]]);
    }
    t
}

/// The seated faces and the vertices + centroids of region `gi`: what `buildProps` hands the
/// track functions as `regionTris` / `pts`.
pub fn region_inputs(
    topo: &Topology,
    res: &Analysis,
    rot: &Rot3,
    gi: usize,
) -> (Vec<usize>, Tris, Vec<[f64; 3]>) {
    let region = &res.regions[gi];
    let mut tris = vec![0.0; region.faces.len() * 9];
    let mut pts = Vec::new();
    for (k, &f) in region.faces.iter().enumerate() {
        let (mut gx, mut gy, mut gz) = (0.0f64, 0.0f64, 0.0f64);
        for q in 0..3 {
            let o = f * 9 + q * 3;
            let (x, y, z) = (topo.pos[o], topo.pos[o + 1], topo.pos[o + 2]);
            let sx = rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x;
            let sy = rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y;
            let sz = rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z;
            tris[k * 9 + q * 3] = sx;
            tris[k * 9 + q * 3 + 1] = sy;
            tris[k * 9 + q * 3 + 2] = sz;
            pts.push([sx, sy, sz]);
            gx += sx;
            gy += sy;
            gz += sz;
        }
        pts.push([gx / 3.0, gy / 3.0, gz / 3.0]);
    }
    (region.faces.clone(), Tris::new(tris), pts)
}

/// A build context with no crest cut, as the prop tests build with.
pub fn ctx(cfg: &Config) -> BuildCtx<'_> {
    BuildCtx::new(cfg, Crest::new(None, 0.0, cfg))
}

/// Tagged vertices without the tag.
pub fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

/// `new Float32Array(v)`: each value rounded to f32.
pub fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| (x as f32) as f64).collect()
}
