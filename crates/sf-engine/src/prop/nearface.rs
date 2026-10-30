//! Port of `web/prop/nearface.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! The part faces nearest a point: `nearest_faces` returns every face tied for the smallest
//! distance, in face-index order -- the same answer as measuring every face, through a cached
//! 3D bucket grid over the seated part.
//!
//! Why a grid: tines.js aims each tine at the nearest face, and measuring every face for every
//! tine is tines x faces. A big part has many tines AND many faces, so that one loop was 60% of
//! an Auto build on a 512 mm figure (285 s).
//!
//! Exact, not approximate. A first pass over the cells around the point finds some face and its
//! distance; every face that could be as near (box within that distance, plus a margin far above
//! float rounding) is then measured with the same `pt_tri_dist2` on the same seated coordinates,
//! so the minimum, the tie tolerance and the face order all come out exactly as the full scan's did.

use std::sync::{Arc, Mutex};

use crate::fins::seating::pose_key;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::types::{Rot3, Xyz};

/// Port of `ptTriDist2` (web/prop/nearface.js:20-40): squared distance from point `p` to
/// triangle `(a, b, c)`. Ericson closest-point.
pub fn pt_tri_dist2(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let sub = |u: [f64; 3], v: [f64; 3]| [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return dot(ap, ap);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return dot(bp, bp);
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return dot(cp, cp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        let q = [a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]];
        let w = sub(p, q);
        return dot(w, w);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        let q = [a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]];
        let u = sub(p, q);
        return dot(u, u);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let q = [
            b[0] + w * (c[0] - b[0]),
            b[1] + w * (c[1] - b[1]),
            b[2] + w * (c[2] - b[2]),
        ];
        let u = sub(p, q);
        return dot(u, u);
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    let q = [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ];
    let u = sub(p, q);
    dot(u, u)
}

// Cells: about one per face, capped so a huge mesh can't allocate without bound.
// A face is listed in every cell its box overlaps; long slivers across a dense
// mesh can blow that up, so the grid coarsens until the lists stay near n.
/// `MAX_CELLS` (web/prop/nearface.js:44).
const MAX_CELLS: f64 = (1u32 << 21) as f64;
/// `MAX_LISTED` (web/prop/nearface.js:45): listings per face, on average, before coarsening.
const MAX_LISTED: f64 = 8.0;

/// The `_index` WeakMap entry (web/prop/nearface.js:50-89), cached per topology. JS keeps one
/// index per topology, rebuilt when the pose changes: `current` is that one slot.
#[derive(Debug, Default)]
pub struct NearIndex {
    current: Mutex<Option<Arc<Index>>>,
}

/// The per-pose grid `indexFor` returns: `{ key, S, lo, h, dims, start, items, stamp, gen, d2 }`.
#[derive(Debug)]
struct Index {
    key: String,
    s: Vec<f64>,
    lo: [f64; 3],
    h: f64,
    dims: [usize; 3],
    start: Vec<i32>,
    items: Vec<usize>,
    /// `stamp`, `gen`, `d2`: per-query scratch (JS mutates them on the index; a query holds the lock).
    scratch: Mutex<Scratch>,
}

#[derive(Debug)]
struct Scratch {
    stamp: Vec<u32>,
    gen_: u32,
    d2: Vec<f64>,
}

/// `Math.min(dims[k] - 1, Math.max(0, Math.floor((v - lo[k]) / h)))`: NaN stays NaN.
fn ci(v: f64, k: usize, lo: &[f64; 3], h: f64, dims: &[usize; 3]) -> f64 {
    jsnum::min(
        (dims[k] - 1) as f64,
        jsnum::max(0.0, jsnum::floor((v - lo[k]) / h)),
    )
}

/// The grid for this topology in this pose (`indexFor`, web/prop/nearface.js:51-89).
fn index_for(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Arc<Index> {
    let key = pose_key(rot, offset);
    let slot = topo.cache.near_index.get_or_init(NearIndex::default);
    let mut cur = slot.current.lock().unwrap();
    if let Some(hit) = cur.as_ref()
        && hit.key == key
    {
        return hit.clone();
    }
    let pos = &topo.pos;
    let n_f = topo.n_faces;
    // The seated coordinates, computed exactly as tines.js's full scan seated them
    // (same expression, same order), so every distance below is bit-identical.
    let mut s = vec![0.0f64; n_f * 9];
    let mut o = 0;
    while o < n_f * 9 {
        let (x, y, z) = (pos[o], pos[o + 1], pos[o + 2]);
        s[o] = rot[0] * x + rot[3] * y + rot[6] * z + offset.x;
        s[o + 1] = rot[1] * x + rot[4] * y + rot[7] * z + offset.y;
        s[o + 2] = rot[2] * x + rot[5] * y + rot[8] * z + offset.z;
        o += 3;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut o = 0;
    while o < s.len() {
        for k in 0..3 {
            if s[o + k] < lo[k] {
                lo[k] = s[o + k];
            }
            if s[o + k] > hi[k] {
                hi[k] = s[o + k];
            }
        }
        o += 3;
    }
    let ext: Vec<f64> = (0..3).map(|k| jsnum::max(hi[k] - lo[k], 1e-9)).collect();
    let dims_for = |h: f64| -> [usize; 3] {
        let d = |k: usize| jsnum::max(1.0, jsnum::min(1024.0, jsnum::ceil(ext[k] / h))) as usize;
        [d(0), d(1), d(2)]
    };
    let cells_at = |h: f64| -> f64 { dims_for(h).iter().fold(1.0, |p, &d| p * d as f64) };
    // Cell edge h: the largest that still gives ~one cell per face (bisect on h,
    // since a flat part's thin axis makes a closed form overshoot).
    let target = jsnum::min(MAX_CELLS, jsnum::max(1.0, n_f as f64));
    let mut h_lo = 1e-6;
    let mut h_hi = jsnum::max_of(&ext);
    for _ in 0..60 {
        let h = jsnum::sqrt(h_lo * h_hi);
        if cells_at(h) > target {
            h_lo = h;
        } else {
            h_hi = h;
        }
    }
    // Coarsen until the lists fit; at one cell every face is listed once, so it ends.
    let mut h = h_hi;
    let (dims, start, items) = loop {
        let dims = dims_for(h);
        if let Some((start, items)) = bucket(&s, n_f, &lo, h, &dims) {
            break (dims, start, items);
        }
        h *= 1.5;
    };
    let idx = Arc::new(Index {
        key,
        s,
        lo,
        h,
        dims,
        start,
        items,
        scratch: Mutex::new(Scratch {
            stamp: vec![0; n_f],
            gen_: 0,
            d2: vec![0.0; n_f],
        }),
    });
    *cur = Some(idx.clone());
    idx
}

/// Port of `bucket` (web/prop/nearface.js:93-118): bucket every face into the cells its box
/// overlaps (CSR), or `None` when the lists run past `MAX_LISTED` per face.
fn bucket(
    s: &[f64],
    n_f: usize,
    lo: &[f64; 3],
    h: f64,
    dims: &[usize; 3],
) -> Option<(Vec<i32>, Vec<usize>)> {
    let [nx, ny, nz] = *dims;
    // `spans` is an Int32Array: a NaN cell (a NaN vertex) is stored as 0.
    let mut spans = vec![0i32; n_f * 6];
    let mut listed = 0.0f64;
    for f in 0..n_f {
        let o = f * 9;
        for k in 0..3 {
            let (a, b, c) = (s[o + k], s[o + 3 + k], s[o + 6 + k]);
            spans[f * 6 + k * 2] = jsnum::to_int32(ci(jsnum::min_of(&[a, b, c]), k, lo, h, dims));
            spans[f * 6 + k * 2 + 1] =
                jsnum::to_int32(ci(jsnum::max_of(&[a, b, c]), k, lo, h, dims));
        }
        listed += (spans[f * 6 + 1] - spans[f * 6] + 1) as f64
            * (spans[f * 6 + 3] - spans[f * 6 + 2] + 1) as f64
            * (spans[f * 6 + 5] - spans[f * 6 + 4] + 1) as f64;
        if listed > MAX_LISTED * n_f as f64 + (1u32 << 20) as f64 {
            return None;
        }
    }
    let n_cells = nx * ny * nz;
    let mut start = vec![0i32; n_cells + 1];
    let each = |f: usize, fun: &mut dyn FnMut(usize)| {
        let sp = f * 6;
        for a in spans[sp]..=spans[sp + 1] {
            for b in spans[sp + 2]..=spans[sp + 3] {
                for c in spans[sp + 4]..=spans[sp + 5] {
                    fun((a as usize * ny + b as usize) * nz + c as usize);
                }
            }
        }
    };
    for f in 0..n_f {
        each(f, &mut |cell| start[cell + 1] += 1);
    }
    for i in 0..n_cells {
        start[i + 1] += start[i];
    }
    let mut items = vec![0usize; start[n_cells] as usize];
    let mut cur: Vec<i32> = start[..n_cells].to_vec();
    for f in 0..n_f {
        // ascending f per cell
        each(f, &mut |cell| {
            items[cur[cell] as usize] = f;
            cur[cell] += 1;
        });
    }
    Some((start, items))
}

/// Port of `boxDist2` (web/prop/nearface.js:122-132): squared distance from `(px, py, pz)` to
/// face `f`'s bounding box: never more than the distance to the face itself.
fn box_dist2(s: &[f64], f: usize, px: f64, py: f64, pz: f64) -> f64 {
    let o = f * 9;
    let mut sum = 0.0;
    let p = [px, py, pz];
    for k in 0..3 {
        let (a, b, c) = (s[o + k], s[o + 3 + k], s[o + 6 + k]);
        let mn = jsnum::min_of(&[a, b, c]);
        let mx = jsnum::max_of(&[a, b, c]);
        let d = if p[k] < mn {
            mn - p[k]
        } else if p[k] > mx {
            p[k] - mx
        } else {
            0.0
        };
        sum += d * d;
    }
    sum
}

/// Port of `nearestFaces` (web/prop/nearface.js:134-216): every face of the seated part tied for
/// nearest to `(px, py, pz)`, ascending by face index, with the full scan's tie rule (within
/// `best * 1e-9 + 1e-12` of the minimum squared distance). Empty when no distance is finite.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(x < Infinity)` is true for NaN, as in JS
pub fn nearest_faces(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    px: f64,
    py: f64,
    pz: f64,
) -> Vec<usize> {
    let n_f = topo.n_faces;
    if n_f == 0 {
        return Vec::new();
    }
    let g = index_for(topo, rot, offset);
    let (s, lo, h, dims, start, items) = (&g.s, &g.lo, g.h, &g.dims, &g.start, &g.items);
    let [nx, ny, nz] = *dims;
    let mut sc = g.scratch.lock().unwrap();
    // A fresh stamp per query marks the faces already measured (wraps after 4e9).
    sc.gen_ += 1;
    if sc.gen_ == 0xffffffff {
        sc.stamp.fill(0);
        sc.gen_ = 1;
    }
    let gen_ = sc.gen_;
    let p = [px, py, pz];
    let mut seen: Vec<usize> = Vec::new();
    let measure = |sc: &mut Scratch, seen: &mut Vec<usize>, f: usize| {
        if sc.stamp[f] == gen_ {
            return;
        }
        sc.stamp[f] = gen_;
        let o = f * 9;
        sc.d2[f] = pt_tri_dist2(
            p,
            [s[o], s[o + 1], s[o + 2]],
            [s[o + 3], s[o + 4], s[o + 5]],
            [s[o + 6], s[o + 7], s[o + 8]],
        );
        seen.push(f);
    };
    let scan_cell = |a: usize, b: usize, c: usize, fun: &mut dyn FnMut(usize)| {
        let cell = (a * ny + b) * nz + c;
        for k in start[cell] as usize..start[cell + 1] as usize {
            fun(items[k]);
        }
    };

    // Pass 1: an upper bound. Rings of cells out from the point's cell (clamped
    // into the grid) until one holds a face.
    let c0f = [
        ci(px, 0, lo, h, dims),
        ci(py, 1, lo, h, dims),
        ci(pz, 2, lo, h, dims),
    ];
    let r_max = nx.max(ny).max(nz) as i64;
    // A NaN cell coordinate makes every JS loop below run zero times.
    if !c0f.iter().any(|v| v.is_nan()) {
        let c0 = [c0f[0] as i64, c0f[1] as i64, c0f[2] as i64];
        let mut r = 0i64;
        while r <= r_max && seen.is_empty() {
            for a in 0.max(c0[0] - r)..=(nx as i64 - 1).min(c0[0] + r) {
                for b in 0.max(c0[1] - r)..=(ny as i64 - 1).min(c0[1] + r) {
                    // only the ring's shell: a full column on its sides, just the two caps inside
                    let side = (a - c0[0]).abs() == r || (b - c0[1]).abs() == r;
                    if side {
                        for c in 0.max(c0[2] - r)..=(nz as i64 - 1).min(c0[2] + r) {
                            scan_cell(a as usize, b as usize, c as usize, &mut |f| {
                                measure(&mut sc, &mut seen, f);
                            });
                        }
                    } else {
                        if c0[2] - r >= 0 {
                            scan_cell(a as usize, b as usize, (c0[2] - r) as usize, &mut |f| {
                                measure(&mut sc, &mut seen, f);
                            });
                        }
                        if c0[2] + r < nz as i64 {
                            scan_cell(a as usize, b as usize, (c0[2] + r) as usize, &mut |f| {
                                measure(&mut sc, &mut seen, f);
                            });
                        }
                    }
                }
            }
            r += 1;
        }
    }
    let mut ub = f64::INFINITY;
    for &f in &seen {
        if sc.d2[f] < ub {
            ub = sc.d2[f];
        }
    }
    if !(ub < f64::INFINITY) {
        // Nothing finite near the point (degenerate coordinates): fall back to the scan.
        for f in 0..n_f {
            measure(&mut sc, &mut seen, f);
        }
    } else {
        // Pass 2: every face whose box is within the bound (plus its tie tolerance and
        // a margin that dwarfs rounding) could be the nearest or tied with it.
        let bound = (ub + ub * 1e-9 + 1e-12) * (1.0 + 1e-6) + 1e-9;
        let r = jsnum::sqrt(bound);
        let range = |v0: f64, v1: f64, k: usize| {
            let (a, b) = (ci(v0, k, lo, h, dims), ci(v1, k, lo, h, dims));
            if a.is_nan() || b.is_nan() || a > b {
                std::ops::RangeInclusive::new(1, 0)
            } else {
                (a as usize)..=(b as usize)
            }
        };
        for a in range(px - r, px + r, 0) {
            for b in range(py - r, py + r, 1) {
                for c in range(pz - r, pz + r, 2) {
                    scan_cell(a, b, c, &mut |f| {
                        if sc.stamp[f] != gen_ && box_dist2(s, f, px, py, pz) <= bound {
                            measure(&mut sc, &mut seen, f);
                        }
                    });
                }
            }
        }
    }
    let mut best = f64::INFINITY;
    for &f in &seen {
        if sc.d2[f] < best {
            best = sc.d2[f];
        }
    }
    if !(best < f64::INFINITY) {
        return Vec::new();
    }
    let tol = best * 1e-9 + 1e-12;
    let mut out: Vec<usize> = Vec::new();
    for &f in &seen {
        if sc.d2[f] <= best + tol {
            out.push(f);
        }
    }
    out.sort_unstable(); // distinct integers: stable and unstable sorts agree
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fins::seating::tests::topo_of;
    use crate::types::IDENTITY3;

    const A: [f64; 3] = [0.0, 0.0, 0.0];
    const B: [f64; 3] = [4.0, 0.0, 0.0];
    const C: [f64; 3] = [0.0, 3.0, 0.0];

    #[test]
    fn pt_tri_dist2_reaches_every_region() {
        let d = |p| pt_tri_dist2(p, A, B, C);
        assert_eq!(d([-1.0, -1.0, 1.0]), 3.0); // vertex a
        assert_eq!(d([6.0, -1.0, 0.0]), 5.0); // vertex b
        assert_eq!(d([-1.0, 5.0, 0.0]), 5.0); // vertex c
        assert_eq!(d([2.0, -2.0, 1.0]), 5.0); // edge ab
        assert_eq!(d([-2.0, 1.5, 0.0]), 4.0); // edge ac
        assert_eq!(d([4.0, 3.0, 0.0]), 5.76); // edge bc: 2.4^2
        assert_eq!(d([1.0, 1.0, 2.0]), 4.0); // face
        assert_eq!(d([1.0, 1.0, 0.0]), 0.0); // on the face
        assert_eq!(d(A), 0.0);
        assert_eq!(d([2.0, 0.0, 0.0]), 0.0); // on an edge
    }

    #[test]
    fn pt_tri_dist2_non_finite_inputs() {
        assert!(pt_tri_dist2([f64::NAN, 0.0, 0.0], A, B, C).is_nan());
        assert!(!pt_tri_dist2([f64::INFINITY, 0.0, 0.0], A, B, C).is_finite()); // the exact value is pinned by the parity case 'inf'
        // a degenerate triangle (all three corners equal) is the distance to that point
        assert_eq!(pt_tri_dist2([1.0, 2.0, 2.0], A, A, A), 9.0);
    }

    /// The full scan nearestFaces replaces: every face tied for nearest, ascending.
    fn brute(pos: &[f64], p: [f64; 3]) -> Vec<usize> {
        let n = pos.len() / 9;
        let d2: Vec<f64> = (0..n)
            .map(|f| {
                let o = f * 9;
                pt_tri_dist2(
                    p,
                    [pos[o], pos[o + 1], pos[o + 2]],
                    [pos[o + 3], pos[o + 4], pos[o + 5]],
                    [pos[o + 6], pos[o + 7], pos[o + 8]],
                )
            })
            .collect();
        let best = d2
            .iter()
            .copied()
            .fold(f64::INFINITY, |b, d| if d < b { d } else { b });
        if best == f64::INFINITY {
            return vec![];
        }
        let tol = best * 1e-9 + 1e-12;
        (0..n).filter(|&f| d2[f] <= best + tol).collect()
    }

    fn lcg(seed: &mut u32) -> f64 {
        *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        *seed as f64 / 4294967296.0 * 20.0 - 10.0
    }

    #[test]
    fn matches_the_full_scan_on_a_scatter() {
        let mut seed = 7;
        let pos: Vec<f64> = (0..300 * 9).map(|_| lcg(&mut seed)).collect();
        let t = topo_of(pos.clone());
        let off = Xyz::default();
        for _ in 0..200 {
            let p = [
                lcg(&mut seed) * 1.5,
                lcg(&mut seed) * 1.5,
                lcg(&mut seed) * 1.5,
            ];
            assert_eq!(
                nearest_faces(&t, &IDENTITY3, &off, p[0], p[1], p[2]),
                brute(&pos, p)
            );
        }
    }

    #[test]
    fn ties_come_back_in_face_index_order() {
        // two faces share the edge (0,0,0)-(4,0,0); a point on that edge is nearest to both
        let pos = vec![
            0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 3.0, 0.0, //
            0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 4.0, 0.0, 0.0,
        ];
        let t = topo_of(pos);
        assert_eq!(
            nearest_faces(&t, &IDENTITY3, &Xyz::default(), 2.0, 0.0, 0.0),
            vec![0, 1]
        );
        assert_eq!(
            nearest_faces(&t, &IDENTITY3, &Xyz::default(), 1.0, 1.0, 0.0),
            vec![0]
        );
    }

    #[test]
    fn no_faces_or_no_finite_distance_is_empty() {
        let none = topo_of(vec![]);
        assert!(nearest_faces(&none, &IDENTITY3, &Xyz::default(), 0.0, 0.0, 0.0).is_empty());
        let t = topo_of(vec![0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 3.0, 0.0]);
        let off = Xyz::default();
        assert!(nearest_faces(&t, &IDENTITY3, &off, f64::NAN, 0.0, 0.0).is_empty());
        // far away is still finite
        assert_eq!(nearest_faces(&t, &IDENTITY3, &off, 1e9, 1e9, 1e9), vec![0]);
    }

    #[test]
    fn the_pose_decides_the_seated_coordinates_and_rebuilds_the_index() {
        let t = topo_of(vec![0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 3.0, 0.0]);
        let up = Xyz {
            x: 0.0,
            y: 0.0,
            z: 100.0,
        };
        // a point near the seated face at z = 100 only finds it through the new offset
        assert_eq!(nearest_faces(&t, &IDENTITY3, &up, 1.0, 1.0, 100.0), vec![0]);
        assert_eq!(
            nearest_faces(&t, &IDENTITY3, &Xyz::default(), 1.0, 1.0, 0.0),
            vec![0]
        );
        let g = t
            .cache
            .near_index
            .get()
            .unwrap()
            .current
            .lock()
            .unwrap()
            .clone()
            .unwrap();
        assert_eq!(g.key, "1,0,0,0,1,0,0,0,1|0,0,0");
    }

    #[test]
    fn coarsens_the_grid_when_long_slivers_list_too_many_cells() {
        // 1500 long thin faces that each cross the whole box: the first grid lists each face in
        // ~every cell, past MAX_LISTED, so the cell edge grows by 1.5 until the lists fit.
        let mut seed = 99;
        let mut pos = Vec::new();
        for _ in 0..1500 {
            let (y, z) = (lcg(&mut seed), lcg(&mut seed));
            pos.extend([-50.0, y, z, 50.0, y + 0.01, z, 50.0, y, z + 0.01]);
        }
        let t = topo_of(pos.clone());
        for p in [[0.0, 0.0, 0.0], [10.0, 3.0, -2.0], [-60.0, 1.0, 1.0]] {
            assert_eq!(
                nearest_faces(&t, &IDENTITY3, &Xyz::default(), p[0], p[1], p[2]),
                brute(&pos, p)
            );
        }
    }
}
