//! Port of `web/fins/seating.js`. Read `docs/AGENTS.md` before changing anything here.

use std::sync::Arc;

use crate::overhangs::Topology;
use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `seatedPartTris(topo, rot, offset)` (web/fins/seating.js:23-31): cached on the topology
/// under the same key string JS builds (`rot.join(',')|x,y,z`, via `jsnum::to_js_string`).
pub fn seated_part_tris(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Arc<Tris> {
    let _ = (topo, rot, offset);
    todo!("M1 track C")
}

/// `forgetSeated(topo)` (web/fins/seating.js:34).
pub fn forget_seated(topo: &Topology) {
    let _ = topo;
    todo!("M1 track C")
}
