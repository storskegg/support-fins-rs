//! Port of `web/inside.js`. Read `docs/AGENTS.md` before changing anything here.

/// `buildGrid`'s result (web/inside.js:39-95), cached per topology.
#[derive(Debug)]
pub struct InsideGrid {}

use crate::overhangs::Topology;
use crate::types::{Rot3, Xyz};

/// `nearestPart`'s `{ d, cosUp }` (web/inside.js:199).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearest {
    pub d: f64,
    pub cos_up: f64,
}

/// `solidClearance`'s `{ d, cosUp, x, y, z }` (web/inside.js:429).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clearance {
    pub d: f64,
    pub cos_up: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// `nearestPart(topo, rot, offset, qx, qy, qz, rMax = 0.45)` (web/inside.js:167-200). `None` = JS `null`.
pub fn nearest_part(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    qx: f64,
    qy: f64,
    qz: f64,
    r_max: f64,
) -> Option<Nearest> {
    let _ = (topo, rot, offset, qx, qy, qz, r_max);
    todo!("M1 track A")
}

/// `solidClearance(topo, rot, offset, verts, rMax = 0.45)` (web/inside.js:262-430). `verts` is
/// the JS array of `[x, y, z]` vertex triples. `None` = JS `null`.
pub fn solid_clearance(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    verts: &[[f64; 3]],
    r_max: f64,
) -> Option<Clearance> {
    let _ = (topo, rot, offset, verts, r_max);
    todo!("M1 track A")
}

/// `insidePart(topo, rot, offset, qx, qy, qz)` (web/inside.js:525-556).
pub fn inside_part(topo: &Topology, rot: &Rot3, offset: &Xyz, qx: f64, qy: f64, qz: f64) -> bool {
    let _ = (topo, rot, offset, qx, qy, qz);
    todo!("M1 track A")
}
