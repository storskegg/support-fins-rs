//! Port of `web/overhangs.js`: topology (weld + adjacency, once per file) and
//! `analyze` (orientation → overhang regions, per pose).

use std::sync::{Arc, Mutex, OnceLock};

use indexmap::IndexMap;

use crate::jsnum;
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
    /// sway `patchCache` WeakMap (web/sway.js:763-773). Filled by M3 track A.
    pub(crate) sway_patches: OnceLock<crate::sway::SwayPatches>,
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

/// `NaN_KEY` (web/overhangs.js:38): never a real quantised coordinate.
const NAN_KEY: f64 = f64::MAX;

/// `q3(v)` (web/overhangs.js:39-42): quantise to 1e-3 mm so shared vertices weld. `+ 0`
/// turns -0 into 0 and every NaN welds to `NAN_KEY`.
fn q3(v: f64) -> f64 {
    let q = jsnum::round(v * 1000.0);
    if !q.is_nan() { q + 0.0 } else { NAN_KEY }
}

/// `mix(h, v)` (web/overhangs.js:50): `Math.imul(h ^ (v | 0) ^ ((v / 4294967296) | 0), 0x9e3779b1)`.
fn mix(h: f64, v: f64) -> i32 {
    let x = jsnum::to_int32(h) ^ jsnum::to_int32(v) ^ jsnum::to_int32(v / 4294967296.0);
    jsnum::imul(x as f64, 2654435761.0) // 0x9e3779b1
}

/// `slot(h, cap)` (web/overhangs.js:54): `(h ^ (h >>> 16)) & (cap - 1)`.
fn slot(h: i32, cap: usize) -> usize {
    let folded = h ^ jsnum::to_int32(jsnum::ushr(h as f64, 16.0));
    (folded & jsnum::to_int32((cap - 1) as f64)) as usize
}

/// `tableSize(n)` (web/overhangs.js:55-59): the power-of-two table size for `n` entries.
fn table_size(n: usize) -> usize {
    let mut cap = 16usize;
    while (cap as f64) < n as f64 * 2.0 {
        cap *= 2;
    }
    cap
}

/// `buildTopology(geometry)` (web/overhangs.js:62-157). Takes the position array
/// (`geometry.getAttribute('position').array`). Large meshes weld on all cores
/// (`weld::build_topology_par`); the result is identical to `build_topology_serial`.
pub fn build_topology(pos: Vec<f64>) -> Topology {
    if pos.len() / 9 >= crate::weld::PAR_MIN_FACES {
        crate::weld::build_topology_par(pos)
    } else {
        build_topology_serial(pos)
    }
}

/// The single-threaded `buildTopology`: the port of the JS, and the reference the parallel weld
/// is tested against.
pub fn build_topology_serial(pos: Vec<f64>) -> Topology {
    let n_faces = pos.len() / 9;

    // float64: normals are compared against a threshold that real parts land
    // exactly on, and float32 rounding alone can flip a 45-degree chamfer.
    let mut nrm = vec![0.0f64; n_faces * 3];
    let mut area = vec![0.0f64; n_faces];

    // vertex weld: slot -> id, with the id's quantised key in vk
    let v_cap = table_size(n_faces * 3);
    let mut v_slot = vec![-1i32; v_cap];
    let mut vk = vec![0.0f64; n_faces * 9];
    let mut n_verts: i32 = 0;
    let mut vid = vec![0i32; n_faces * 3];

    for f in 0..n_faces {
        let o = f * 9;
        let (ax, ay, az) = (pos[o], pos[o + 1], pos[o + 2]);
        let (bx, by, bz) = (pos[o + 3], pos[o + 4], pos[o + 5]);
        let (cx, cy, cz) = (pos[o + 6], pos[o + 7], pos[o + 8]);

        // Face normal from the winding, NOT from the STL's stored normal attribute --
        // exported normals are routinely zero-length or inconsistent, and a wrong
        // normal here silently mislabels an overhang.
        let (ux, uy, uz) = (bx - ax, by - ay, bz - az);
        let (vx, vy, vz) = (cx - ax, cy - ay, cz - az);
        let px = uy * vz - uz * vy;
        let py = uz * vx - ux * vz;
        let pz = ux * vy - uy * vx;
        let len = jsnum::hypot(&[px, py, pz]);

        if len > 1e-12 {
            nrm[f * 3] = px / len;
            nrm[f * 3 + 1] = py / len;
            nrm[f * 3 + 2] = pz / len;
        }
        area[f] = 0.5 * len;

        for i in 0..3 {
            let qx = q3(pos[o + i * 3]);
            let qy = q3(pos[o + i * 3 + 1]);
            let qz = q3(pos[o + i * 3 + 2]);
            let mut h = slot(mix(mix(mix(2166136261.0, qx) as f64, qy) as f64, qz), v_cap);
            // 0x811c9dc5 == 2166136261
            let id;
            loop {
                let cur = v_slot[h];
                if cur < 0 {
                    id = n_verts;
                    v_slot[h] = id;
                    n_verts += 1;
                    let u = id as usize;
                    vk[u * 3] = qx;
                    vk[u * 3 + 1] = qy;
                    vk[u * 3 + 2] = qz;
                    break;
                }
                let u = cur as usize;
                if vk[u * 3] == qx && vk[u * 3 + 1] == qy && vk[u * 3 + 2] == qz {
                    id = cur;
                    break;
                }
                h = (h + 1) & (v_cap - 1);
            }
            vid[f * 3 + i] = id;
        }
    }

    // face adjacency via shared welded edges
    // edge -> the first face that had it: slot -> edge number, edge's ends in ek
    let e_cap = table_size(n_faces * 3);
    let mut e_slot = vec![-1i32; e_cap];
    let mut ek = vec![0i32; n_faces * 6];
    let mut e_face = vec![0i32; n_faces * 3];
    let mut n_edges: usize = 0;
    let mut adj_a: Vec<i32> = Vec::new();
    let mut adj_b: Vec<i32> = Vec::new();
    for f in 0..n_faces {
        for i in 0..3 {
            let u = vid[f * 3 + i];
            let v = vid[f * 3 + ((i + 1) % 3)];
            let a = if u < v { u } else { v };
            let b = if u < v { v } else { u };
            let mut h = slot(mix(mix(2166136261.0, a as f64) as f64, b as f64), e_cap);
            loop {
                let e = e_slot[h];
                if e < 0 {
                    e_slot[h] = n_edges as i32;
                    ek[n_edges * 2] = a;
                    ek[n_edges * 2 + 1] = b;
                    e_face[n_edges] = f as i32;
                    n_edges += 1;
                    break;
                }
                let eu = e as usize;
                if ek[eu * 2] == a && ek[eu * 2 + 1] == b {
                    adj_a.push(e_face[eu]);
                    adj_b.push(f as i32);
                    break;
                }
                h = (h + 1) & (e_cap - 1);
            }
        }
    }

    Topology {
        pos,
        n_faces,
        nrm,
        area,
        vid,
        adj_a,
        adj_b,
        vertex_count: n_verts as usize,
        edge_count: n_edges,
        cache: TopoCache::default(),
    }
}

/// `analyze(topo, thresholdDeg, rot)` (web/overhangs.js:171-244). JS defaults:
/// `thresholdDeg = DEFAULT_THRESHOLD`, `rot = IDENTITY3`; callers pass them explicitly.
pub fn analyze(topo: &Topology, threshold_deg: f64, rot: &Rot3) -> Analysis {
    let pos = &topo.pos;
    let n_faces = topo.n_faces;
    let nrm = &topo.nrm;
    let area = &topo.area;
    let adj_a = &topo.adj_a;
    let adj_b = &topo.adj_b;
    let cut = -(jsnum::cos((threshold_deg * std::f64::consts::PI) / 180.0) + ANGLE_EPS);

    let mut zr = vec![0.0f64; n_faces * 3];
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut min_z = f64::INFINITY;
    let mut max_z = f64::NEG_INFINITY;

    let mut p = 0;
    for v in 0..n_faces * 3 {
        let (x, y, z) = (pos[p], pos[p + 1], pos[p + 2]);
        let xr = rot[0] * x + rot[3] * y + rot[6] * z;
        let yr = rot[1] * x + rot[4] * y + rot[7] * z;
        let zz = rot[2] * x + rot[5] * y + rot[8] * z;
        zr[v] = zz;
        if xr < min_x {
            min_x = xr;
        }
        if xr > max_x {
            max_x = xr;
        }
        if yr < min_y {
            min_y = yr;
        }
        if yr > max_y {
            max_y = yr;
        }
        if zz < min_z {
            min_z = zz;
        }
        if zz > max_z {
            max_z = zz;
        }
        p += 3;
    }

    let mut on_bed = vec![0u8; n_faces];
    let mut over = vec![0u8; n_faces];
    let mut bed_area = 0.0f64;
    let mut over_area = 0.0f64;
    let mut over_face_count = 0usize;

    for f in 0..n_faces {
        // face height above the plate, after re-seating on the lowest point
        let h = jsnum::max_of(&[zr[f * 3], zr[f * 3 + 1], zr[f * 3 + 2]]) - min_z;
        if h < BED_EPS {
            on_bed[f] = 1;
            bed_area += area[f];
            continue;
        }

        let nzr = rot[2] * nrm[f * 3] + rot[5] * nrm[f * 3 + 1] + rot[8] * nrm[f * 3 + 2];
        if nzr < cut {
            over[f] = 1;
            over_area += area[f];
            over_face_count += 1;
        }
    }

    // union-find over adjacent overhang faces
    let mut parent: Vec<i32> = (0..n_faces as i32).collect();
    // `find(x)`: path halving, `x = parent[x] = parent[parent[x]]`
    fn find(parent: &mut [i32], mut x: i32) -> i32 {
        while parent[x as usize] != x {
            let g = parent[parent[x as usize] as usize];
            parent[x as usize] = g;
            x = g;
        }
        x
    }
    for e in 0..adj_a.len() {
        let a = adj_a[e];
        let b = adj_b[e];
        if over[a as usize] == 0 || over[b as usize] == 0 {
            continue;
        }
        let ra = find(&mut parent, a);
        let rb = find(&mut parent, b);
        if ra != rb {
            parent[ra as usize] = rb;
        }
    }

    let mut by_root: IndexMap<i32, Region> = IndexMap::new();
    for f in 0..n_faces {
        if over[f] == 0 {
            continue;
        }
        let r = find(&mut parent, f as i32);
        if let Some(g) = by_root.get_mut(&r) {
            g.faces.push(f);
            g.area += area[f];
        } else {
            by_root.insert(
                r,
                Region {
                    faces: vec![f],
                    area: area[f],
                },
            );
        }
    }

    let raw: Vec<Region> = by_root.into_values().collect();
    let raw_region_count = raw.len();
    let mut regions: Vec<Region> = raw
        .into_iter()
        .filter(|g| g.area >= MIN_REGION_AREA)
        .collect();
    // `.sort((a, b) => b.area - a.area)`: stable; a NaN difference counts as equal
    regions.sort_by(|a, b| {
        let d = b.area - a.area;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });

    // faces that survived the region-area filter, for shading
    let mut kept = vec![0u8; n_faces];
    for g in &regions {
        for &f in &g.faces {
            kept[f] = 1;
        }
    }

    Analysis {
        over,
        kept,
        on_bed,
        regions,
        raw_region_count,
        over_area,
        bed_area,
        over_face_count,
        // where the rotated part sits, so the caller can drop it onto the plate
        offset: Xyz {
            x: -(min_x + max_x) / 2.0,
            y: -(min_y + max_y) / 2.0,
            z: -min_z,
        },
        size: Xyz {
            x: max_x - min_x,
            y: max_y - min_y,
            z: max_z - min_z,
        },
    }
}
