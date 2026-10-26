//! Port of `web/overhangs.js`: topology (weld + adjacency, once per file) and
//! `analyze` (orientation → overhang regions, per pose).

use std::sync::{Arc, Mutex, OnceLock};

use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `BED_EPS` (web/overhangs.js:17).
pub const BED_EPS: f64 = 0.35;
/// `MIN_REGION_AREA` (web/overhangs.js:18).
pub const MIN_REGION_AREA: f64 = 12.0;
/// `DEFAULT_THRESHOLD` (web/overhangs.js:19).
pub const DEFAULT_THRESHOLD: f64 = 45.0;
/// `ANGLE_EPS` (web/overhangs.js:29).
pub const ANGLE_EPS: f64 = 1e-4;

/// The object `buildTopology` returns (web/overhangs.js:144-156), minus the
/// `_zr/_parent/_over/_kept/_onBed` scratch arrays: `analyze` allocates its own,
/// so two analyses never alias (see `Analysis`).
///
/// JS also hangs lazily built caches on the topo object (`_insideGrid`,
/// `_adjStart/_adjNbr`, `_vgraph`) or keys `WeakMap`s by it (nearface `_index`,
/// trace `NEIGHBOURS`, seating `_seated`, sway `patchCache`). Those live in `cache`.
#[derive(Debug)]
pub struct Topology {
    /// The input positions, as given (f32-rounded values widened, when they came from a `Float32Array`).
    pub pos: Vec<f64>,
    pub n_faces: usize,
    pub nrm: Vec<f64>,
    pub area: Vec<f64>,
    /// `vid` (`Int32Array`): welded vertex id per face corner.
    pub vid: Vec<i32>,
    pub adj_a: Vec<i32>,
    pub adj_b: Vec<i32>,
    pub vertex_count: usize,
    pub edge_count: usize,
    #[allow(dead_code)] // read by the M1 ports; remove this allow when they land
    pub(crate) cache: TopoCache,
}

/// Per-topology lazy caches: one slot per JS cache. Each module owns its slot's type.
#[derive(Debug, Default)]
#[allow(dead_code)] // slots filled by the M1 ports; remove this allow when they land
pub(crate) struct TopoCache {
    /// `topo._insideGrid` (web/inside.js:41-59).
    pub(crate) inside_grid: OnceLock<crate::inside::InsideGrid>,
    /// `topo._adjStart` / `topo._adjNbr` (web/planes.js:195-208).
    pub(crate) face_adjacency: OnceLock<crate::planes::FaceAdjacency>,
    /// `topo._vgraph` (web/islands.js:57).
    pub(crate) vgraph: OnceLock<crate::islands::VGraph>,
    /// nearface `_index` WeakMap (web/prop/nearface.js:50-89).
    pub(crate) near_index: OnceLock<crate::prop::nearface::NearIndex>,
    /// trace `NEIGHBOURS` WeakMap (web/prop/trace.js:26-37).
    pub(crate) trace_neighbours: OnceLock<crate::prop::trace::Neighbours>,
    /// seating `_seated` WeakMap (web/fins/seating.js:22-34): the last pose's seated
    /// soup, keyed by the same string JS builds. `forget_seated` clears it.
    pub(crate) seated: Mutex<Option<(String, Arc<Tris>)>>,
}

/// One overhang region (web/overhangs.js:213-217): `{ faces, area }`.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub faces: Vec<usize>,
    pub area: f64,
}

/// The object `analyze` returns (web/overhangs.js:236-243).
///
/// **Aliasing divergence (deliberate):** JS returns `over`, `kept` and `onBed` as the
/// topo's scratch arrays, so the *next* `analyze` on the same topo overwrites an
/// earlier result's arrays. Rust returns owned arrays. Any JS caller that keeps
/// an old result across a new `analyze` must be flagged in review, because it sees the
/// newer pose's flags there and the Rust would not.
#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub over: Vec<u8>,
    pub kept: Vec<u8>,
    pub on_bed: Vec<u8>,
    pub regions: Vec<Region>,
    pub raw_region_count: usize,
    pub over_area: f64,
    pub bed_area: f64,
    pub over_face_count: usize,
    pub offset: Xyz,
    pub size: Xyz,
}

/// `buildTopology(geometry)` (web/overhangs.js:62-157). Takes the position array
/// (`geometry.getAttribute('position').array`).
pub fn build_topology(pos: Vec<f64>) -> Topology {
    let _ = pos;
    todo!("M1 track A")
}

/// `analyze(topo, thresholdDeg, rot)` (web/overhangs.js:171-244).
pub fn analyze(topo: &Topology, threshold_deg: f64, rot: &Rot3) -> Analysis {
    let _ = (topo, threshold_deg, rot);
    todo!("M1 track A")
}
