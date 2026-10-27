//! The test helpers of tests/_util.js that need `build_topology` / `inside_part` (M1/A):
//! `loadModel`, `blockTopo`, `tiltedBlockTopo`, `holedPlateTopo`, `insideCount` and
//! `nearestFaceInwardH`.

use sf_engine::inside::inside_part;
use sf_engine::jsnum;
use sf_engine::overhangs::{Topology, build_topology};
use sf_engine::planes::Xy;
use sf_engine::types::{Rot3, Xyz};

use crate::{block, holed_plate_soup, load_model_soup, pt_tri_dist2, tilted_block_soup};

/// `loadModel(name)` (tests/_util.js:27-30): `readSTL` of `<models_dir>/<name>.stl`, then
/// `buildTopology`.
pub fn load_model(name: &str) -> Topology {
    build_topology(load_model_soup(name))
}

/// The topology of a `tests/fixtures` STL (see [`load_fixture_soup`]).
pub fn load_fixture_topo(name: &str) -> Topology {
    build_topology(crate::load_fixture_soup(name))
}

/// `blockTopo(...args)` (tests/_util.js:44-47): the topology of `block(..)`.
pub fn block_topo(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Topology {
    build_topology(block(x0, x1, y0, y1, z0, z1))
}

/// `tiltedBlockTopo(x0, x1, y0, y1, z0, z1, degX)` (tests/_util.js:52-64): the topology of
/// `tilted_block_soup`.
pub fn tilted_block_topo(
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    z0: f64,
    z1: f64,
    deg_x: f64,
) -> Topology {
    build_topology(tilted_block_soup(x0, x1, y0, y1, z0, z1, deg_x))
}

/// `holedPlateTopo(X, Y, TH, hx, hy)` (tests/_util.js:70-91): the topology of `holed_plate_soup`.
pub fn holed_plate_topo(x: f64, y: f64, th: f64, hx: f64, hy: f64) -> Topology {
    build_topology(holed_plate_soup(x, y, th, hx, hy))
}

/// `insideCount(topo, rot, offset, tris)` (tests/_util.js:100-104): how many of these
/// `[x, y, z]` verts are strictly inside the seated part.
pub fn inside_count(topo: &Topology, rot: &Rot3, offset: &Xyz, tris: &[[f64; 3]]) -> usize {
    let mut c = 0;
    for v in tris {
        if inside_part(topo, rot, offset, v[0], v[1], v[2]) {
            c += 1;
        }
    }
    c
}

/// `nearestFaceInwardH(topo, rot, offset, p)` (tests/_util.js:168-185): the horizontal INWARD
/// unit normal of the seated part face nearest point `p`, or `None` if that face is
/// near-horizontal (no sideways grip).
pub fn nearest_face_inward_h(topo: &Topology, rot: &Rot3, offset: &Xyz, p: [f64; 3]) -> Option<Xy> {
    let pos = &topo.pos;
    let nrm = &topo.nrm;
    let seat = |o: usize| -> [f64; 3] {
        [
            rot[0] * pos[o] + rot[3] * pos[o + 1] + rot[6] * pos[o + 2] + offset.x,
            rot[1] * pos[o] + rot[4] * pos[o + 1] + rot[7] * pos[o + 2] + offset.y,
            rot[2] * pos[o] + rot[5] * pos[o + 1] + rot[8] * pos[o + 2] + offset.z,
        ]
    };
    let mut best = f64::INFINITY;
    let mut bf: Option<usize> = None;
    for f in 0..topo.n_faces {
        let o = f * 9;
        let d2 = pt_tri_dist2(p, seat(o), seat(o + 3), seat(o + 6));
        if d2 < best {
            best = d2;
            bf = Some(f);
        }
    }
    let bf = bf?;
    let (nx, ny, nz) = (nrm[bf * 3], nrm[bf * 3 + 1], nrm[bf * 3 + 2]);
    let sx = rot[0] * nx + rot[3] * ny + rot[6] * nz;
    let sy = rot[1] * nx + rot[4] * ny + rot[7] * nz;
    let (hx, hy) = (-sx, -sy);
    let hm = jsnum::hypot(&[hx, hy]);
    if hm < 0.2 {
        return None;
    }
    Some(Xy {
        x: hx / hm,
        y: hy / hm,
    })
}
