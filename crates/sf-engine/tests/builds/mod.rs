//! What the ported JS tests that call `buildFins` share: the options they pass, a build, a
//! topology from a soup. (Each is `fins.buildFins(topo, analyze(topo, 45, rot), rot, { mode:
//! 'auto', bedPad: true, tines: true, ... })` in the JS.)
#![allow(dead_code)]
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::types::{IDENTITY3, Rot3, V4};

pub const I: Rot3 = IDENTITY3;

/// `new Float32Array(..)` of a list of values.
pub fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| (x as f32) as f64).collect()
}

/// `buildTopology({ getAttribute: () => ({ array: pos }) })`.
pub fn topo_of(pos: Vec<f64>) -> Arc<Topology> {
    Arc::new(build_topology(pos))
}

/// `{ mode: 'auto', bedPad: true, tines: true, coverage }`.
pub fn auto_opts(coverage: Option<f64>) -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        coverage,
        ..Default::default()
    }
}

/// `analyze(topo, 45, rot)` then `buildFins`, with the default config.
pub fn build(topo: &Arc<Topology>, rot: &Rot3, opts: &FinsOpts) -> (Analysis, FinsBuilt) {
    let res = analyze(topo, 45.0, rot);
    let built = build_fins(topo, &res, rot, opts, &Config::default());
    (res, built)
}

pub fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

/// The part's triangles seated into print space as a vertex list (`topo.pos` through `rot` and
/// `offset`, as the pad tests spell out).
pub fn seated_verts(topo: &Topology, rot: &Rot3, o: &sf_engine::types::Xyz) -> Vec<[f64; 3]> {
    let mut part = Vec::new();
    for f in 0..topo.n_faces * 3 {
        let (x, y, z) = (topo.pos[f * 3], topo.pos[f * 3 + 1], topo.pos[f * 3 + 2]);
        part.push([
            rot[0] * x + rot[3] * y + rot[6] * z + o.x,
            rot[1] * x + rot[4] * y + rot[7] * z + o.y,
            rot[2] * x + rot[5] * y + rot[8] * z + o.z,
        ]);
    }
    part
}

/// `sectionSegs(tris, z)` (tests/pad.test.js:166): a triangle list's section at height z.
pub fn section_segs(tris: &[[f64; 3]], z: f64) -> Vec<[[f64; 2]; 2]> {
    let mut segs = Vec::new();
    let mut i = 0;
    while i + 2 < tris.len() {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for a in 0..3 {
            let p = tris[i + a];
            let q = tris[i + (a + 1) % 3];
            let (za, zb) = (p[2] - z, q[2] - z);
            if (za < 0.0) == (zb < 0.0) {
                continue;
            }
            let t = za / (za - zb);
            pts.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
        }
        if pts.len() == 2 {
            segs.push([pts[0], pts[1]]);
        }
        i += 3;
    }
    segs
}

/// `segDist` (tests/pad.test.js:181).
pub fn seg_dist([a, b]: [[f64; 2]; 2], [c, d]: [[f64; 2]; 2]) -> f64 {
    let pt = |p: [f64; 2], [u, v]: [[f64; 2]; 2]| {
        let dx = v[0] - u[0];
        let dy = v[1] - u[1];
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 0.0 {
            0.0f64.max(1.0f64.min(((p[0] - u[0]) * dx + (p[1] - u[1]) * dy) / l2))
        } else {
            0.0
        };
        (u[0] + t * dx - p[0]).hypot(u[1] + t * dy - p[1])
    };
    pt(a, [c, d])
        .min(pt(b, [c, d]))
        .min(pt(c, [a, b]))
        .min(pt(d, [a, b]))
}

/// `tests/fixtures/<name>.stl` of the upstream checkout as a topology (raster.test.js `example`).
pub fn fixture_topo(name: &str) -> Arc<Topology> {
    Arc::new(sf_testkit::load_fixture_topo(name))
}
