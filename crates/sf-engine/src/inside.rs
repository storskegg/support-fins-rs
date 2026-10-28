//! Port of `web/inside.js`. Read `docs/AGENTS.md` before changing anything here.

use std::collections::HashSet;

use rayon::prelude::*;

use crate::gridsize::grid_dims;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::types::{Rot3, Xyz};

/// `EPS` (web/inside.js:27).
const EPS: f64 = 1e-9;
/// `COARSE_CELLS` (web/inside.js:29): solidClearance switches to the coarse grid past this many
/// fine cells a box.
const COARSE_CELLS: f64 = 64.0;

/// One YZ bucket grid in CSR form: the object `bucketYZ` returns (web/inside.js:64-97).
#[derive(Debug)]
pub(crate) struct Bucket {
    start: Vec<i32>,
    items: Vec<i32>,
    min_y: f64,
    min_z: f64,
    sy: f64,
    sz: f64,
    ny: usize,
    nz: usize,
}

/// `buildGrid`'s result (web/inside.js:40-62), cached per topology: the fine grid and, when it
/// is not already 64 x 64, the coarse 64 x 64 one (`grid.coarse`; `None` stands for JS's
/// `grid.coarse === grid`).
#[derive(Debug)]
pub struct InsideGrid {
    fine: Bucket,
    coarse: Option<Bucket>,
}

impl InsideGrid {
    fn coarse(&self) -> &Bucket {
        self.coarse.as_ref().unwrap_or(&self.fine)
    }
}

/// `grid.cell(y, z)` (web/inside.js:50-54).
fn cell(g: &Bucket, y: f64, z: f64) -> usize {
    let a = clamp_idx(jsnum::floor((y - g.min_y) * g.sy), g.ny);
    let b = clamp_idx(jsnum::floor((z - g.min_z) * g.sz), g.nz);
    a * g.nz + b
}

/// `Math.min(n - 1, Math.max(0, v))` as an index.
fn clamp_idx(v: f64, n: usize) -> usize {
    jsnum::min((n - 1) as f64, jsnum::max(0.0, v)) as usize
}

/// `buildGrid(topo)` (web/inside.js:40-62).
fn build_grid(topo: &Topology) -> &InsideGrid {
    topo.cache.inside_grid.get_or_init(|| {
        let pos = &topo.pos;
        let n_faces = topo.n_faces;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut min_z = f64::INFINITY;
        let mut max_z = f64::NEG_INFINITY;
        for v in 0..n_faces * 3 {
            let y = pos[v * 3 + 1];
            let z = pos[v * 3 + 2];
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
            if z < min_z {
                min_z = z;
            }
            if z > max_z {
                max_z = z;
            }
        }
        let [ny, nz] = grid_dims(max_y - min_y, max_z - min_z, n_faces as f64);
        let fine = bucket_yz(pos, n_faces, min_y, max_y, min_z, max_z, ny, nz);
        let coarse = if ny == 64 && nz == 64 {
            None
        } else {
            Some(bucket_yz(pos, n_faces, min_y, max_y, min_z, max_z, 64, 64))
        };
        InsideGrid { fine, coarse }
    })
}

/// `bucketYZ` (web/inside.js:64-97): NY x NZ cells over [minY, maxY] x [minZ, maxZ], as CSR,
/// each cell's faces ascending.
#[allow(clippy::too_many_arguments)]
fn bucket_yz(
    pos: &[f64],
    n_faces: usize,
    min_y: f64,
    max_y: f64,
    min_z: f64,
    max_z: f64,
    ny: usize,
    nz: usize,
) -> Bucket {
    let sy = ny as f64 / jsnum::max(1e-6, max_y - min_y);
    let sz = nz as f64 / jsnum::max(1e-6, max_z - min_z);
    let mut counts = vec![0i32; ny * nz + 1];
    let span = |f: usize| -> [usize; 4] {
        let o = f * 9;
        let y0 = jsnum::min_of(&[pos[o + 1], pos[o + 4], pos[o + 7]]);
        let y1 = jsnum::max_of(&[pos[o + 1], pos[o + 4], pos[o + 7]]);
        let z0 = jsnum::min_of(&[pos[o + 2], pos[o + 5], pos[o + 8]]);
        let z1 = jsnum::max_of(&[pos[o + 2], pos[o + 5], pos[o + 8]]);
        [
            clamp_idx(jsnum::floor((y0 - min_y) * sy), ny),
            clamp_idx(jsnum::floor((y1 - min_y) * sy), ny),
            clamp_idx(jsnum::floor((z0 - min_z) * sz), nz),
            clamp_idx(jsnum::floor((z1 - min_z) * sz), nz),
        ]
    };

    for f in 0..n_faces {
        let [a0, a1, b0, b1] = span(f);
        for a in a0..=a1 {
            for b in b0..=b1 {
                counts[a * nz + b + 1] += 1;
            }
        }
    }
    for i in 0..ny * nz {
        counts[i + 1] += counts[i];
    }
    let mut items = vec![0i32; counts[ny * nz] as usize];
    let mut cur: Vec<i32> = counts[..ny * nz].to_vec();
    for f in 0..n_faces {
        let [a0, a1, b0, b1] = span(f);
        for a in a0..=a1 {
            for b in b0..=b1 {
                let c = a * nz + b;
                items[cur[c] as usize] = f as i32;
                cur[c] += 1;
            }
        }
    }
    Bucket {
        start: counts,
        items,
        min_y,
        min_z,
        sy,
        sz,
        ny,
        nz,
    }
}

/// `triClosest(pos, f, px, py, pz)` (web/inside.js:99-143): closest point on triangle `f`,
/// all in the mesh's own frame.
fn tri_closest(pos: &[f64], f: usize, px: f64, py: f64, pz: f64) -> [f64; 3] {
    let o = f * 9;
    let (ax, ay, az) = (pos[o], pos[o + 1], pos[o + 2]);
    let (abx, aby, abz) = (pos[o + 3] - ax, pos[o + 4] - ay, pos[o + 5] - az);
    let (acx, acy, acz) = (pos[o + 6] - ax, pos[o + 7] - ay, pos[o + 8] - az);
    let (apx, apy, apz) = (px - ax, py - ay, pz - az);

    let d1 = abx * apx + aby * apy + abz * apz;
    let d2 = acx * apx + acy * apy + acz * apz;
    if d1 <= 0.0 && d2 <= 0.0 {
        return [ax, ay, az];
    }

    let (bpx, bpy, bpz) = (px - pos[o + 3], py - pos[o + 4], pz - pos[o + 5]);
    let d3 = abx * bpx + aby * bpy + abz * bpz;
    let d4 = acx * bpx + acy * bpy + acz * bpz;
    if d3 >= 0.0 && d4 <= d3 {
        return [pos[o + 3], pos[o + 4], pos[o + 5]];
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        return [ax + abx * t, ay + aby * t, az + abz * t];
    }

    let (cpx, cpy, cpz) = (px - pos[o + 6], py - pos[o + 7], pz - pos[o + 8]);
    let d5 = abx * cpx + aby * cpy + abz * cpz;
    let d6 = acx * cpx + acy * cpy + acz * cpz;
    if d6 >= 0.0 && d5 <= d6 {
        return [pos[o + 6], pos[o + 7], pos[o + 8]];
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        return [ax + acx * t, ay + acy * t, az + acz * t];
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [
            pos[o + 3] + (pos[o + 6] - pos[o + 3]) * t,
            pos[o + 4] + (pos[o + 7] - pos[o + 4]) * t,
            pos[o + 5] + (pos[o + 8] - pos[o + 5]) * t,
        ];
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [
        ax + abx * v + acx * w,
        ay + aby * v + acy * w,
        az + abz * v + acz * w,
    ]
}

/// The part's closest surface point within `rMax` of a PRINT-space query point, or `None`
/// if nothing is that close.
///
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
    let pos = &topo.pos;
    let grid = &build_grid(topo).fine;

    let x = qx - offset.x;
    let y = qy - offset.y;
    let z = qz - offset.z;
    let ox = rot[0] * x + rot[1] * y + rot[2] * z;
    let oy = rot[3] * x + rot[4] * y + rot[5] * z;
    let oz = rot[6] * x + rot[7] * y + rot[8] * z;

    let a0 = clamp_idx(jsnum::floor((oy - r_max - grid.min_y) * grid.sy), grid.ny);
    let a1 = clamp_idx(jsnum::floor((oy + r_max - grid.min_y) * grid.sy), grid.ny);
    let b0 = clamp_idx(jsnum::floor((oz - r_max - grid.min_z) * grid.sz), grid.nz);
    let b1 = clamp_idx(jsnum::floor((oz + r_max - grid.min_z) * grid.sz), grid.nz);

    let mut best = r_max * r_max;
    let (mut bx, mut by, mut bz) = (0.0, 0.0, 0.0);
    let mut found = false;
    for a in a0..=a1 {
        for b in b0..=b1 {
            let c = a * grid.nz + b;
            for i in grid.start[c]..grid.start[c + 1] {
                let q = tri_closest(pos, grid.items[i as usize] as usize, ox, oy, oz);
                let dx = q[0] - ox;
                let dy = q[1] - oy;
                let dz = q[2] - oz;
                let dd = dx * dx + dy * dy + dz * dz;
                if dd < best {
                    best = dd;
                    bx = dx;
                    by = dy;
                    bz = dz;
                    found = true;
                }
            }
        }
    }
    if !found {
        return None;
    }
    let d = jsnum::sqrt(best);
    // PRINT-space z of a mesh-frame direction: how far "up" the approach points
    let up_z = rot[2] * bx + rot[5] * by + rot[8] * bz;
    Some(Nearest {
        d,
        cos_up: if d > EPS { up_z / d } else { 0.0 },
    })
}

/// `segSegClosest(p0, p1, q0, q1)` (web/inside.js:203-231): closest points between segments
/// p0-p1 and q0-q1; returns (pt on P, pt on Q).
fn seg_seg_closest(p0: [f64; 3], p1: [f64; 3], q0: [f64; 3], q1: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let (dx1, dy1, dz1) = (p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
    let (dx2, dy2, dz2) = (q1[0] - q0[0], q1[1] - q0[1], q1[2] - q0[2]);
    let (rx, ry, rz) = (p0[0] - q0[0], p0[1] - q0[1], p0[2] - q0[2]);
    let a = dx1 * dx1 + dy1 * dy1 + dz1 * dz1;
    let e = dx2 * dx2 + dy2 * dy2 + dz2 * dz2;
    let f = dx2 * rx + dy2 * ry + dz2 * rz;
    let (s, t);
    if a <= EPS && e <= EPS {
        s = 0.0;
        t = 0.0;
    } else if a <= EPS {
        s = 0.0;
        t = jsnum::min(1.0, jsnum::max(0.0, f / e));
    } else {
        let c = dx1 * rx + dy1 * ry + dz1 * rz;
        if e <= EPS {
            t = 0.0;
            s = jsnum::min(1.0, jsnum::max(0.0, -c / a));
        } else {
            let b = dx1 * dx2 + dy1 * dy2 + dz1 * dz2;
            let den = a * e - b * b;
            let mut s2 = if den > EPS {
                jsnum::min(1.0, jsnum::max(0.0, (b * f - c * e) / den))
            } else {
                0.0
            };
            let mut t2 = (b * s2 + f) / e;
            if t2 < 0.0 {
                t2 = 0.0;
                s2 = jsnum::min(1.0, jsnum::max(0.0, -c / a));
            } else if t2 > 1.0 {
                t2 = 1.0;
                s2 = jsnum::min(1.0, jsnum::max(0.0, (b - c) / a));
            }
            s = s2;
            t = t2;
        }
    }
    (
        [p0[0] + dx1 * s, p0[1] + dy1 * s, p0[2] + dz1 * s],
        [q0[0] + dx2 * t, q0[1] + dy2 * t, q0[2] + dz2 * t],
    )
}

/// `triVerts(pos, f)` (web/inside.js:230-235).
fn tri_verts(pos: &[f64], f: usize) -> [[f64; 3]; 3] {
    let o = f * 9;
    [
        [pos[o], pos[o + 1], pos[o + 2]],
        [pos[o + 3], pos[o + 4], pos[o + 5]],
        [pos[o + 6], pos[o + 7], pos[o + 8]],
    ]
}

/// `dist2(p, q)` (web/inside.js:237-240).
fn dist2(p: [f64; 3], q: [f64; 3]) -> f64 {
    let dx = p[0] - q[0];
    let dy = p[1] - q[1];
    let dz = p[2] - q[2];
    dx * dx + dy * dy + dz * dz
}

/// One chunk of `solid_clearance`'s walk: the best distance found (`r_max^2` if none), the
/// closest pair, and whether it ended on a pierce (then `best` is 0 and the pair is the pierce).
struct Chunk {
    best: f64,
    bp: Option<[f64; 3]>,
    bq: Option<[f64; 3]>,
    pierced: bool,
}

/// A solid with fewer triangles than this is walked on the calling thread.
const PAR_MIN_TRIS: usize = 48;
/// Chunks a larger solid is cut into (at least `PAR_MIN_CHUNK` triangles each).
const PAR_CHUNKS: usize = 32;
const PAR_MIN_CHUNK: usize = 8;

thread_local! {
    /// The "already measured against this solid triangle" stamps of `solid_clearance`, one per part
    /// triangle, kept per thread across calls: a stamp is unique to one solid triangle (the counter
    /// only grows), so stale entries never match and the array is not cleared per call.
    static SEEN: std::cell::RefCell<(Vec<i32>, i32)> = const { std::cell::RefCell::new((Vec::new(), 0)) };
}

/// Run `f` with this thread's stamp array (at least `n_faces` long) and a stamp counter with room
/// for `n_stamps` more. `f` must not call back into `with_seen`.
fn with_seen<R>(n_faces: usize, n_stamps: usize, f: impl FnOnce(&mut [i32], &mut i32) -> R) -> R {
    SEEN.with(|cell| {
        let (seen, stamp) = &mut *cell.borrow_mut();
        if seen.len() < n_faces {
            seen.resize(n_faces, 0);
        }
        if *stamp as i64 + n_stamps as i64 >= i32::MAX as i64 {
            seen.iter_mut().for_each(|s| *s = 0);
            *stamp = 0;
        }
        f(seen, stamp)
    })
}

/// The exact minimum clearance between an added solid and the part, or `None` if it is at
/// least `rMax` everywhere.
///
/// `solidClearance(topo, rot, offset, verts, rMax = 0.45)` (web/inside.js:262-430). `verts` is
/// the JS array of `[x, y, z]` vertex triples. `None` = JS `null`.
///
/// Divergence: JS stamps already-measured part triangles in `topo._scSeen`/`topo._scGen`
/// across calls; here the stamp array and generation are per call (same semantics: a pair is
/// measured once per wall triangle), so nothing is cached on the topology.
pub fn solid_clearance(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    verts: &[[f64; 3]],
    r_max: f64,
) -> Option<Clearance> {
    let pos = &topo.pos;
    let ig = build_grid(topo);
    let grid = &ig.fine;
    let coarse_is_fine = ig.coarse.is_none();

    // The first part triangle wall triangle A pierces, in the coarse grid's cell
    // order -- the walk below with the box rejects, on the old 64 x 64 layout.
    let coarse_pierce = |a_tri: &[[f64; 3]; 3],
                         x_lo: f64,
                         x_hi: f64,
                         y_lo: f64,
                         y_hi: f64,
                         z_lo: f64,
                         z_hi: f64|
     -> Option<[f64; 3]> {
        let g = ig.coarse();
        let mut done: HashSet<i32> = HashSet::new();
        let a0 = clamp_idx(jsnum::floor((y_lo - r_max - g.min_y) * g.sy), g.ny);
        let a1 = clamp_idx(jsnum::floor((y_hi + r_max - g.min_y) * g.sy), g.ny);
        let b0 = clamp_idx(jsnum::floor((z_lo - r_max - g.min_z) * g.sz), g.nz);
        let b1 = clamp_idx(jsnum::floor((z_hi + r_max - g.min_z) * g.sz), g.nz);
        for a in a0..=a1 {
            for b in b0..=b1 {
                let c = a * g.nz + b;
                for i in g.start[c]..g.start[c + 1] {
                    let f = g.items[i as usize];
                    if !done.insert(f) {
                        continue;
                    }
                    let o = f as usize * 9;
                    if jsnum::min_of(&[pos[o], pos[o + 3], pos[o + 6]]) > x_hi + r_max {
                        continue;
                    }
                    if jsnum::max_of(&[pos[o], pos[o + 3], pos[o + 6]]) < x_lo - r_max {
                        continue;
                    }
                    if jsnum::min_of(&[pos[o + 1], pos[o + 4], pos[o + 7]]) > y_hi + r_max {
                        continue;
                    }
                    if jsnum::max_of(&[pos[o + 1], pos[o + 4], pos[o + 7]]) < y_lo - r_max {
                        continue;
                    }
                    if jsnum::min_of(&[pos[o + 2], pos[o + 5], pos[o + 8]]) > z_hi + r_max {
                        continue;
                    }
                    if jsnum::max_of(&[pos[o + 2], pos[o + 5], pos[o + 8]]) < z_lo - r_max {
                        continue;
                    }
                    let hit = tri_tri_pierce(a_tri, &tri_verts(pos, f as usize));
                    if hit.is_some() {
                        return hit;
                    }
                }
            }
        }
        None // unreachable: the fine walk found a pierce among the same candidates
    };

    // un-rotate the solid's vertices into the mesh frame once
    let mv: Vec<[f64; 3]> = verts
        .iter()
        .map(|v| {
            let x = v[0] - offset.x;
            let y = v[1] - offset.y;
            let z = v[2] - offset.z;
            [
                rot[0] * x + rot[1] * y + rot[2] * z,
                rot[3] * x + rot[4] * y + rot[5] * z,
                rot[6] * x + rot[7] * y + rot[8] * z,
            ]
        })
        .collect();

    // One chunk of the solid's triangles `[lo, hi)` (vertex indices, a multiple of 3 apart): its
    // own best, with `best = r_max^2` to start. A pierce ends the chunk (nothing beats zero).
    let scan = |lo: usize, hi: usize, seen: &mut [i32], stamp: &mut i32| -> Chunk {
        let mut best = r_max * r_max;
        let mut bp: Option<[f64; 3]> = None;
        let mut bq: Option<[f64; 3]> = None;
        let mut t = lo;
        while t < hi {
            *stamp += 1;
            let a_tri = [mv[t], mv[t + 1], mv[t + 2]];
            let mut x_lo = f64::INFINITY;
            let mut x_hi = f64::NEG_INFINITY;
            let mut y_lo = f64::INFINITY;
            let mut y_hi = f64::NEG_INFINITY;
            let mut z_lo = f64::INFINITY;
            let mut z_hi = f64::NEG_INFINITY;
            for q in &a_tri {
                if q[0] < x_lo {
                    x_lo = q[0];
                }
                if q[0] > x_hi {
                    x_hi = q[0];
                }
                if q[1] < y_lo {
                    y_lo = q[1];
                }
                if q[1] > y_hi {
                    y_hi = q[1];
                }
                if q[2] < z_lo {
                    z_lo = q[2];
                }
                if q[2] > z_hi {
                    z_hi = q[2];
                }
            }
            // the fine grid unless this triangle's box spans more than COARSE_CELLS of
            // it; then the coarse one (fewer, fuller cells beat a walk of empty ones)
            let mut g = grid;
            let mut g_is_coarse = coarse_is_fine;
            let mut a0 = clamp_idx(jsnum::floor((y_lo - r_max - g.min_y) * g.sy), g.ny);
            let mut a1 = clamp_idx(jsnum::floor((y_hi + r_max - g.min_y) * g.sy), g.ny);
            let mut b0 = clamp_idx(jsnum::floor((z_lo - r_max - g.min_z) * g.sz), g.nz);
            let mut b1 = clamp_idx(jsnum::floor((z_hi + r_max - g.min_z) * g.sz), g.nz);
            if ((a1 - a0 + 1) * (b1 - b0 + 1)) as f64 > COARSE_CELLS {
                g = ig.coarse();
                g_is_coarse = true;
                a0 = clamp_idx(jsnum::floor((y_lo - r_max - g.min_y) * g.sy), g.ny);
                a1 = clamp_idx(jsnum::floor((y_hi + r_max - g.min_y) * g.sy), g.ny);
                b0 = clamp_idx(jsnum::floor((z_lo - r_max - g.min_z) * g.sz), g.nz);
                b1 = clamp_idx(jsnum::floor((z_hi + r_max - g.min_z) * g.sz), g.nz);
            }

            for a in a0..=a1 {
                for b in b0..=b1 {
                    let c = a * g.nz + b;
                    for i in g.start[c]..g.start[c + 1] {
                        let f = g.items[i as usize] as usize;
                        if seen[f] == *stamp {
                            continue; // this part tri already measured vs this wall tri
                        }
                        seen[f] = *stamp;
                        let o = f * 9;
                        // reject on the axis-aligned boxes before any real geometry
                        let bx0 = jsnum::min_of(&[pos[o], pos[o + 3], pos[o + 6]]);
                        if bx0 > x_hi + r_max {
                            continue;
                        }
                        let bx1 = jsnum::max_of(&[pos[o], pos[o + 3], pos[o + 6]]);
                        if bx1 < x_lo - r_max {
                            continue;
                        }
                        let by0 = jsnum::min_of(&[pos[o + 1], pos[o + 4], pos[o + 7]]);
                        if by0 > y_hi + r_max {
                            continue;
                        }
                        let by1 = jsnum::max_of(&[pos[o + 1], pos[o + 4], pos[o + 7]]);
                        if by1 < y_lo - r_max {
                            continue;
                        }
                        let bz0 = jsnum::min_of(&[pos[o + 2], pos[o + 5], pos[o + 8]]);
                        if bz0 > z_hi + r_max {
                            continue;
                        }
                        let bz1 = jsnum::max_of(&[pos[o + 2], pos[o + 5], pos[o + 8]]);
                        if bz1 < z_lo - r_max {
                            continue;
                        }

                        let b_tri = tri_verts(pos, f);

                        // INTERSECTION FIRST. Every distance primitive below is only valid
                        // for triangles that do not cross.
                        let mut pierce = tri_tri_pierce(&a_tri, &b_tri);
                        if pierce.is_some() {
                            // Which pierce is reported (callers trim the station nearest it)
                            // depends on the order the cells are walked. Report the one the old
                            // 64 x 64 walk met first, so the trim is the same at any grid size.
                            if !g_is_coarse {
                                pierce = coarse_pierce(&a_tri, x_lo, x_hi, y_lo, y_hi, z_lo, z_hi);
                            }
                            return Chunk {
                                best: 0.0,
                                bp: pierce,
                                bq: pierce,
                                pierced: true,
                            }; // nothing beats zero
                        }

                        // verts of the part tri against the solid tri and vice versa
                        for q in &b_tri {
                            let cp = tri_closest_p(&a_tri, *q);
                            let dd = dist2(cp, *q);
                            if dd < best {
                                best = dd;
                                bp = Some(cp);
                                bq = Some(*q);
                            }
                        }
                        for q in &a_tri {
                            let cp = tri_closest_p(&b_tri, *q);
                            let dd = dist2(*q, cp);
                            if dd < best {
                                best = dd;
                                bp = Some(*q);
                                bq = Some(cp);
                            }
                        }
                        // all nine edge pairs
                        for ea in 0..3 {
                            for eb in 0..3 {
                                let (p_a, p_b) = seg_seg_closest(
                                    a_tri[ea],
                                    a_tri[(ea + 1) % 3],
                                    b_tri[eb],
                                    b_tri[(eb + 1) % 3],
                                );
                                let dd = dist2(p_a, p_b);
                                if dd < best {
                                    best = dd;
                                    bp = Some(p_a);
                                    bq = Some(p_b);
                                }
                            }
                        }
                    }
                }
            }
            t += 3;
        }
        Chunk {
            best,
            bp,
            bq,
            pierced: false,
        }
    };

    // The chunks' results merge in the solid's triangle order, so the answer is the serial walk's:
    // the first pierce, else the smallest distance, the earliest one on a tie (a later chunk must
    // beat the best so far strictly, as the walk's `dd < best` does).
    let n_tri = mv.len() / 3;
    let chunks: Vec<Chunk> = if n_tri < PAR_MIN_TRIS {
        with_seen(topo.n_faces, n_tri, |seen, stamp| {
            vec![scan(0, mv.len(), seen, stamp)]
        })
    } else {
        let per = n_tri.div_ceil(PAR_CHUNKS).max(PAR_MIN_CHUNK);
        (0..n_tri.div_ceil(per))
            .into_par_iter()
            .map(|c| {
                let (lo, hi) = (c * per * 3, ((c + 1) * per * 3).min(mv.len()));
                with_seen(topo.n_faces, (hi - lo) / 3, |seen, stamp| {
                    scan(lo, hi, seen, stamp)
                })
            })
            .collect()
    };
    let mut best = r_max * r_max;
    let mut bp: Option<[f64; 3]> = None;
    let mut bq: Option<[f64; 3]> = None;
    for ch in chunks {
        if ch.pierced {
            best = 0.0;
            bp = ch.bp;
            bq = ch.bq;
            break;
        }
        if ch.best < best {
            best = ch.best;
            bp = ch.bp;
            bq = ch.bq;
        }
    }
    let bp = bp?;
    let bq = bq.expect("bq is set with bp");
    let d = jsnum::sqrt(best);
    let dx = bq[0] - bp[0];
    let dy = bq[1] - bp[1];
    let dz = bq[2] - bp[2];
    let up_z = rot[2] * dx + rot[5] * dy + rot[8] * dz;
    // the solid-side point, back in print space
    let px = rot[0] * bp[0] + rot[3] * bp[1] + rot[6] * bp[2] + offset.x;
    let py = rot[1] * bp[0] + rot[4] * bp[1] + rot[7] * bp[2] + offset.y;
    let pz = rot[2] * bp[0] + rot[5] * bp[1] + rot[8] * bp[2] + offset.z;
    Some(Clearance {
        d,
        cos_up: if d > EPS { up_z / d } else { 0.0 },
        x: px,
        y: py,
        z: pz,
    })
}

/// `segTriHit(p, q, A, B, C)` (web/inside.js:433-462): segment p-q against triangle A-B-C
/// (Moller-Trumbore); the hit point or `None`.
fn seg_tri_hit(
    p: [f64; 3],
    q: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Option<[f64; 3]> {
    let (dx, dy, dz) = (q[0] - p[0], q[1] - p[1], q[2] - p[2]);
    let (e1x, e1y, e1z) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let (e2x, e2y, e2z) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
    let hx = dy * e2z - dz * e2y;
    let hy = dz * e2x - dx * e2z;
    let hz = dx * e2y - dy * e2x;
    let det = e1x * hx + e1y * hy + e1z * hz;
    // parallel; grazing is distance work. Relative to the lengths: `a` is a triple
    // product (mm^3), and an edge lying IN the triangle's plane measures ~1e-12 of
    // rounding on mm-sized parts (local issue 023).
    let scale = jsnum::hypot(&[dx, dy, dz])
        * jsnum::hypot(&[e1x, e1y, e1z])
        * jsnum::hypot(&[e2x, e2y, e2z]);
    if jsnum::abs(det) <= 1e-9 * scale {
        return None;
    }
    let f = 1.0 / det;
    let (sx, sy, sz) = (p[0] - a[0], p[1] - a[1], p[2] - a[2]);
    let u = f * (sx * hx + sy * hy + sz * hz);
    if u < 0.0 || u > 1.0 {
        return None;
    }
    let qx = sy * e1z - sz * e1y;
    let qy = sz * e1x - sx * e1z;
    let qz = sx * e1y - sy * e1x;
    let v = f * (dx * qx + dy * qy + dz * qz);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * (e2x * qx + e2y * qy + e2z * qz);
    if t < 0.0 || t > 1.0 {
        return None;
    }
    Some([p[0] + dx * t, p[1] + dy * t, p[2] + dz * t])
}

/// `triTriPierce(A, B)` (web/inside.js:465-472): do triangles A and B cross? The piercing
/// point if so.
fn tri_tri_pierce(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> Option<[f64; 3]> {
    for e in 0..3 {
        let hit = seg_tri_hit(a[e], a[(e + 1) % 3], b[0], b[1], b[2])
            .or_else(|| seg_tri_hit(b[e], b[(e + 1) % 3], a[0], a[1], a[2]));
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// `triClosestP(T, p)` (web/inside.js:475-519): `triClosest` for a triangle given as three
/// vertex arrays.
fn tri_closest_p(tri: &[[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    let [a, b, c] = *tri;
    let (abx, aby, abz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let (acx, acy, acz) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
    let (apx, apy, apz) = (p[0] - a[0], p[1] - a[1], p[2] - a[2]);

    let d1 = abx * apx + aby * apy + abz * apz;
    let d2 = acx * apx + acy * apy + acz * apz;
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }

    let (bpx, bpy, bpz) = (p[0] - b[0], p[1] - b[1], p[2] - b[2]);
    let d3 = abx * bpx + aby * bpy + abz * bpz;
    let d4 = acx * bpx + acy * bpy + acz * bpz;
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        return [a[0] + abx * t, a[1] + aby * t, a[2] + abz * t];
    }

    let (cpx, cpy, cpz) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
    let d5 = abx * cpx + aby * cpy + abz * cpz;
    let d6 = acx * cpx + acy * cpy + acz * cpz;
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        return [a[0] + acx * t, a[1] + acy * t, a[2] + acz * t];
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [
            b[0] + (c[0] - b[0]) * t,
            b[1] + (c[1] - b[1]) * t,
            b[2] + (c[2] - b[2]) * t,
        ];
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [
        a[0] + abx * v + acx * w,
        a[1] + aby * v + acy * w,
        a[2] + abz * v + acz * w,
    ]
}

/// Containment by ray parity along +x, for a query point given in PRINT space.
///
/// `insidePart(topo, rot, offset, qx, qy, qz)` (web/inside.js:525-556).
pub fn inside_part(topo: &Topology, rot: &Rot3, offset: &Xyz, qx: f64, qy: f64, qz: f64) -> bool {
    let pos = &topo.pos;
    let grid = &build_grid(topo).fine;

    // inverse of an orthonormal rotation is its transpose
    let x = qx - offset.x;
    let y = qy - offset.y;
    let z = qz - offset.z;
    let ox = rot[0] * x + rot[1] * y + rot[2] * z;
    let oy = rot[3] * x + rot[4] * y + rot[5] * z;
    let oz = rot[6] * x + rot[7] * y + rot[8] * z;

    let c = cell(grid, oy, oz);
    let mut crossings = 0u32;
    for i in grid.start[c]..grid.start[c + 1] {
        let o = grid.items[i as usize] as usize * 9;
        let (ay, az) = (pos[o + 1], pos[o + 2]);
        let (by, bz) = (pos[o + 4], pos[o + 5]);
        let (cy, cz) = (pos[o + 7], pos[o + 8]);

        // barycentric of (oy, oz) within the triangle projected onto the y-z plane
        let d = (bz - cz) * (ay - cy) + (cy - by) * (az - cz);
        if jsnum::abs(d) < EPS {
            continue;
        }
        let l1 = ((bz - cz) * (oy - cy) + (cy - by) * (oz - cz)) / d;
        let l2 = ((cz - az) * (oy - cy) + (ay - cy) * (oz - cz)) / d;
        let l3 = 1.0 - l1 - l2;
        if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
            continue;
        }

        // x where the ray meets the triangle's plane; only forward hits count
        let hx = l1 * pos[o] + l2 * pos[o + 3] + l3 * pos[o + 6];
        if hx > ox {
            crossings += 1;
        }
    }
    (crossings & 1) == 1
}

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
