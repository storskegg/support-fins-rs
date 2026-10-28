//! Port of `web/prop/surface.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Reading the seated part: `seat` rotates a raw vertex into print space, and
//! `surface_z_at` / `surface_zs_at` answer "what part surface is directly above (x, y)" --
//! the lowest one, or all of them -- through a cached XY bucket grid.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::csr::build_csr;
use crate::gridsize::grid_dims;
use crate::jsnum;
use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `buildZGrid`'s result (web/prop/surface.js:45-79), cached per `Tris`: the JS grid object
/// `{ start, items, minX, minY, maxX, maxY, sx, sy, NX, NY, asked }` plus its lazily built
/// `low` (`lowFirst`).
#[derive(Debug)]
pub struct ZGrid {
    start: Vec<i32>,
    /// Triangle offsets (`f`, a multiple of 9) per cell, in triangle-index order.
    items: Vec<usize>,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    sx: f64,
    sy: f64,
    nx: usize,
    ny: usize,
    /// `g.asked`: queries so far (until `low` exists).
    asked: AtomicU64,
    /// `g.low`: built on first use by `low_first`.
    low: OnceLock<Low>,
}

/// `{ items, lb }` of `lowFirst` (web/prop/surface.js:123-125).
#[derive(Debug)]
struct Low {
    items: Vec<usize>,
    lb: Vec<f64>,
}

/// Port of `seat` (web/prop/surface.js:11-17): rotate + seat one raw vertex into print
/// space. JS fills and returns `out`; Rust returns it by value.
pub fn seat(pos: &[f64], i: usize, rot: &Rot3, off: &Xyz) -> [f64; 3] {
    let (x, y, z) = (pos[i], pos[i + 1], pos[i + 2]);
    [
        rot[0] * x + rot[3] * y + rot[6] * z + off.x,
        rot[1] * x + rot[4] * y + rot[7] * z + off.y,
        rot[2] * x + rot[5] * y + rot[8] * z + off.z,
    ]
}

/// A cell coordinate as JS computes it: `Math.min(N - 1, Math.max(0, Math.floor(v)))`.
/// NaN stays NaN (a NaN vertex lists nowhere and a NaN query finds nothing).
fn clamp_cell(v: f64, n: usize) -> f64 {
    jsnum::min((n - 1) as f64, jsnum::max(0.0, jsnum::floor(v)))
}

/// The inclusive range `a..=b` of a JS `for (a = a0; a <= a1; a++)` over cell coordinates:
/// empty when either end is NaN.
fn span_range(a: f64, b: f64) -> std::ops::RangeInclusive<usize> {
    if a.is_nan() || b.is_nan() || a > b {
        std::ops::RangeInclusive::new(1, 0)
    } else {
        (a as usize)..=(b as usize)
    }
}

/// The cell range a triangle at offset `f` covers (`span`, web/prop/surface.js:58-64),
/// as `(a0, a1, b0, b1)`.
fn tri_span(
    tris: &[f64],
    f: usize,
    cx: &dyn Fn(f64) -> f64,
    cy: &dyn Fn(f64) -> f64,
) -> (f64, f64, f64, f64) {
    let (ax, ay, bx, by, dx, dy) = (
        tris[f],
        tris[f + 1],
        tris[f + 3],
        tris[f + 4],
        tris[f + 6],
        tris[f + 7],
    );
    (
        cx(jsnum::min_of(&[ax, bx, dx])),
        cx(jsnum::max_of(&[ax, bx, dx])),
        cy(jsnum::min_of(&[ay, by, dy])),
        cy(jsnum::max_of(&[ay, by, dy])),
    )
}

/// Port of `buildZGrid` (web/prop/surface.js:45-79): XY bucket grid over a `tris` array, so
/// `surface_z_at` scans only the triangles whose footprint covers the query column instead of
/// the whole mesh. JS caches it on the array object; here it lives on the [`Tris`].
fn build_z_grid(tris: &Tris) -> &ZGrid {
    crate::lazy::get_or_build(&tris.z_grid, |par| {
        let t = &tris.v;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        let mut i = 0;
        while i < t.len() {
            let (x, y) = (t[i], t[i + 1]);
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
            i += 3;
        }
        // cells per axis sized to the mesh (gridsize.js), at least the old 64
        let [nx, ny] = grid_dims(max_x - min_x, max_y - min_y, t.len() as f64 / 9.0);
        let sx = nx as f64 / jsnum::max(1e-6, max_x - min_x);
        let sy = ny as f64 / jsnum::max(1e-6, max_y - min_y);
        let cx = |x: f64| clamp_cell((x - min_x) * sx, nx);
        let cy = |y: f64| clamp_cell((y - min_y) * sy, ny);
        let (start, ids) = build_csr(
            t.len() / 9,
            nx * ny,
            |i, push| {
                let (a0, a1, b0, b1) = tri_span(t, i * 9, &cx, &cy);
                for a in span_range(a0, a1) {
                    for b in span_range(b0, b1) {
                        push(a * ny + b);
                    }
                }
            },
            par,
        );
        // (offsets into `tris`, 9 floats a triangle)
        let items: Vec<usize> = ids.into_iter().map(|i| i as usize * 9).collect();
        ZGrid {
            start,
            items,
            min_x,
            min_y,
            max_x,
            max_y,
            sx,
            sy,
            nx,
            ny,
            asked: AtomicU64::new(0),
            low: OnceLock::new(),
        }
    })
}

/// Port of `cellOf` (web/prop/surface.js:82-88): the grid cell's index for column (x, y), or
/// `None` outside the grid. (JS also yields a NaN cell for a NaN query, whose lists are empty:
/// that is `None` here too, and every caller answers the same for both.)
fn cell_of(g: &ZGrid, x: f64, y: f64) -> Option<usize> {
    let e = 1e-9;
    if x < g.min_x - e || x > g.max_x + e || y < g.min_y - e || y > g.max_y + e {
        return None;
    }
    let a = clamp_cell((x - g.min_x) * g.sx, g.nx);
    let b = clamp_cell((y - g.min_y) * g.sy, g.ny);
    if a.is_nan() || b.is_nan() {
        return None;
    }
    Some(a as usize * g.ny + b as usize)
}

/// Port of `zOn` (web/prop/surface.js:91-103): height of the triangle at offset `i` at column
/// (x, y), or `None` if the column misses it.
fn z_on(tris: &[f64], i: usize, x: f64, y: f64) -> Option<f64> {
    let (ax, ay, az) = (tris[i], tris[i + 1], tris[i + 2]);
    let (bx, by, bz) = (tris[i + 3], tris[i + 4], tris[i + 5]);
    let (cx, cy, cz) = (tris[i + 6], tris[i + 7], tris[i + 8]);
    let den = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
    if jsnum::abs(den) < 1e-12 {
        return None;
    }
    let l1 = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / den;
    let l2 = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / den;
    let l3 = 1.0 - l1 - l2;
    if l1 < -1e-9 || l2 < -1e-9 || l3 < -1e-9 {
        return None;
    }
    Some(l1 * az + l2 * bz + l3 * cz)
}

/// `LOW_FIRST_AFTER` (web/prop/surface.js:119): a grid sorts after 2000 queries, plus 2000 per
/// 1000 triangles.
const LOW_FIRST_AFTER: f64 = 2000.0;

/// Port of `lowFirst` (web/prop/surface.js:120-146): a second copy of the cell lists, ordered
/// by how low each triangle can reach (`lb` is a floor under any height `z_on` returns for it).
/// Built on first use.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(lb > -inf)` is true for NaN, as in JS
fn low_first<'a>(g: &'a ZGrid, tris: &[f64]) -> &'a Low {
    g.low.get_or_init(|| {
        let n = tris.len() / 9;
        let mut lb = vec![0.0f64; n];
        for t in 0..n {
            let o = t * 9;
            let (a, b, c) = (tris[o + 2], tris[o + 5], tris[o + 8]);
            let lo = jsnum::min_of(&[a, b, c]);
            let hi = jsnum::max_of(&[a, b, c]);
            lb[t] = lo - 3e-9 * (hi - lo) - 1e-12 * (1.0 + jsnum::abs(lo));
            if !(lb[t] > f64::NEG_INFINITY) {
                lb[t] = f64::NEG_INFINITY;
            }
        }
        // fill the cells in ascending-floor order, so each cell's list comes out sorted
        let mut order: Vec<usize> = (0..n).collect();
        // JS: order.sort((p, q) => lb[p] - lb[q] || p - q); a NaN difference (-inf - -inf)
        // falls through to the index, like a zero.
        order.sort_by(|&p, &q| {
            let d = lb[p] - lb[q];
            if d < 0.0 {
                std::cmp::Ordering::Less
            } else if d > 0.0 {
                std::cmp::Ordering::Greater
            } else {
                p.cmp(&q)
            }
        });
        let (nx, ny) = (g.nx, g.ny);
        let cx = |v: f64| clamp_cell((v - g.min_x) * g.sx, nx);
        let cy = |v: f64| clamp_cell((v - g.min_y) * g.sy, ny);
        let mut items = vec![0usize; g.items.len()];
        let mut cur: Vec<i32> = g.start[..nx * ny].to_vec();
        for &t in &order {
            let f = t * 9;
            let (a0, a1, b0, b1) = tri_span(tris, f, &cx, &cy);
            for a in span_range(a0, a1) {
                for b in span_range(b0, b1) {
                    let c = a * ny + b;
                    items[cur[c] as usize] = f;
                    cur[c] += 1;
                }
            }
        }
        Low { items, lb }
    })
}

/// Port of `surfaceZAt` (web/prop/surface.js:148-172): the lowest surface height of the region
/// directly above (x, y), or `None` (JS `null`) if the region does not cover that point.
pub fn surface_z_at(tris: &Tris, x: f64, y: f64) -> Option<f64> {
    if tris.is_empty() {
        return None;
    }
    let g = build_z_grid(tris);
    let c = cell_of(g, x, y)?;
    let mut best = f64::INFINITY;
    // Sorting the lists costs about a full pass of queries, so only a grid asked
    // enough to repay it (the whole part under a bed pad) gets them; a region's or a
    // patch's few queries scan the plain lists, as before.
    let sorted = g.low.get().is_some() || {
        let asked = g.asked.fetch_add(1, Ordering::Relaxed) + 1;
        asked as f64 > LOW_FIRST_AFTER * (1.0 + tris.len() as f64 / 9.0 / 1000.0)
    };
    if sorted {
        let low = low_first(g, &tris.v);
        for k in g.start[c] as usize..g.start[c + 1] as usize {
            let f = low.items[k];
            if low.lb[f / 9] > best {
                break; // nothing further down this list reaches lower
            }
            if let Some(z) = z_on(&tris.v, f, x, y)
                && z < best
            {
                best = z;
            }
        }
    } else {
        for k in g.start[c] as usize..g.start[c + 1] as usize {
            if let Some(z) = z_on(&tris.v, g.items[k], x, y)
                && z < best
            {
                best = z;
            }
        }
    }
    if best == f64::INFINITY {
        None
    } else {
        Some(best)
    }
}

/// Port of `surfaceHitsAt` (web/prop/surface.js:182-198): with each height, its triangle's unit
/// normal z: `[z, nz]`, in triangle-index order.
pub fn surface_hits_at(tris: &Tris, x: f64, y: f64) -> Vec<[f64; 2]> {
    let mut hits = Vec::new();
    if tris.is_empty() {
        return hits;
    }
    let g = build_z_grid(tris);
    let Some(c) = cell_of(g, x, y) else {
        return hits;
    };
    let t = &tris.v;
    for k in g.start[c] as usize..g.start[c + 1] as usize {
        let i = g.items[k];
        let Some(z) = z_on(t, i, x, y) else { continue };
        let (ux, uy, uz) = (t[i + 3] - t[i], t[i + 4] - t[i + 1], t[i + 5] - t[i + 2]);
        let (vx, vy, vz) = (t[i + 6] - t[i], t[i + 7] - t[i + 1], t[i + 8] - t[i + 2]);
        let (nx, ny, nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
        // `|| 1`: a zero or NaN hypot becomes 1
        let h = jsnum::hypot(&[nx, ny, nz]);
        let h = if h == 0.0 || h.is_nan() { 1.0 } else { h };
        hits.push([z, nz / h]);
    }
    hits
}

/// Port of `surfaceZsAt` (web/prop/surface.js:208-222): every part-surface height directly
/// above (x, y), through the same XY grid as `surface_z_at`.
pub fn surface_zs_at(tris: &Tris, x: f64, y: f64) -> Vec<f64> {
    let mut zs = Vec::new();
    if tris.is_empty() {
        return zs;
    }
    let g = build_z_grid(tris);
    let Some(c) = cell_of(g, x, y) else {
        return zs;
    };
    for k in g.start[c] as usize..g.start[c + 1] as usize {
        if let Some(z) = z_on(&tris.v, g.items[k], x, y) {
            zs.push(z);
        }
    }
    zs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IDENTITY3;

    // Every test but `seat_*` needs `gridsize::grid_dims`, which is track A's (still `todo!()`).
    const NEEDS_A: &str = "needs gridsize::grid_dims (M1 track A)";

    fn tris(v: &[f64]) -> Tris {
        Tris::new(v.to_vec())
    }

    /// A flat triangle (0,0)-(4,0)-(0,4) at height `z`, CCW seen from above.
    fn flat(z: f64) -> Vec<f64> {
        vec![0.0, 0.0, z, 4.0, 0.0, z, 0.0, 4.0, z]
    }

    #[test]
    fn seat_rotates_then_offsets_one_vertex() {
        let rot = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let pos = [9.0, 1.0, 2.0, 3.0];
        let off = Xyz {
            x: 10.0,
            y: 20.0,
            z: 30.0,
        };
        assert_eq!(seat(&pos, 1, &rot, &off), [8.0, 21.0, 33.0]); // x' = -y + 10, y' = x + 20, z' = z + 30
        assert_eq!(seat(&pos, 0, &IDENTITY3, &Xyz::default()), [9.0, 1.0, 2.0]);
    }

    #[test]
    fn an_empty_soup_has_no_surface() {
        let _ = NEEDS_A;
        let t = Tris::new(vec![]);
        assert_eq!(surface_z_at(&t, 0.0, 0.0), None);
        assert!(surface_hits_at(&t, 0.0, 0.0).is_empty());
        assert!(surface_zs_at(&t, 0.0, 0.0).is_empty());
    }

    #[test]
    fn height_interpolates_inside_and_misses_outside() {
        // plane z = x + 2y over the triangle
        let t = tris(&[0.0, 0.0, 0.0, 4.0, 0.0, 4.0, 0.0, 4.0, 8.0]);
        assert_eq!(surface_z_at(&t, 1.0, 1.0), Some(3.0));
        assert_eq!(surface_z_at(&t, 3.0, 3.0), None); // inside the bbox, outside the triangle
        assert_eq!(surface_z_at(&t, -1.0, 1.0), None); // outside the grid
        assert_eq!(surface_z_at(&t, 1.0, 5.0), None);
        // a column on the edge, and within the 1e-9 barycentric slack
        assert_eq!(surface_z_at(&t, 2.0, 0.0), Some(2.0));
        assert!(surface_z_at(&t, 2.0, -1e-10).is_some());
        assert_eq!(surface_z_at(&t, f64::NAN, 1.0), None);
    }

    #[test]
    fn the_grid_edge_has_a_1e_minus_9_margin() {
        let t = tris(&flat(1.0));
        // 4 + 1e-9 is still on the grid and inside the barycentric slack; 4 + 2e-9 is off the grid.
        assert_eq!(surface_z_at(&t, 4.0 + 1e-9, 0.0), Some(1.0));
        assert_eq!(surface_z_at(&t, 4.0, 0.0), Some(1.0));
        assert_eq!(surface_z_at(&t, 4.0 + 2e-9, 0.0), None);
    }

    #[test]
    fn the_lowest_of_several_stacked_surfaces_wins() {
        let mut v = flat(5.0);
        v.extend(flat(2.0));
        v.extend(flat(9.0));
        let t = tris(&v);
        assert_eq!(surface_z_at(&t, 1.0, 1.0), Some(2.0));
        // zs: every surface, in triangle-index order
        assert_eq!(surface_zs_at(&t, 1.0, 1.0), vec![5.0, 2.0, 9.0]);
    }

    #[test]
    fn hits_carry_the_unit_normal_z_in_index_order() {
        let mut v = flat(5.0); // faces up: nz = +1
        v.extend([0.0, 0.0, 2.0, 0.0, 4.0, 2.0, 4.0, 0.0, 2.0]); // wound the other way: nz = -1
        let t = tris(&v);
        assert_eq!(surface_hits_at(&t, 1.0, 1.0), vec![[5.0, 1.0], [2.0, -1.0]]);
        assert!(surface_hits_at(&t, 3.9, 3.9).is_empty());
        // a tilted face: nz = cos of the slope
        let t = tris(&[0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 4.0, 4.0]);
        let h = surface_hits_at(&t, 1.0, 1.0);
        assert_eq!(h.len(), 1);
        assert!((h[0][1] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-15);
    }

    #[test]
    fn a_degenerate_triangle_never_answers() {
        // zero area (collinear), and a NaN corner: neither is a surface
        let mut v = vec![0.0, 0.0, 1.0, 2.0, 0.0, 1.0, 4.0, 0.0, 1.0];
        v.extend([0.0, 0.0, 7.0, 4.0, 0.0, 7.0, f64::NAN, 4.0, 7.0]);
        v.extend(flat(3.0));
        let t = tris(&v);
        assert_eq!(surface_z_at(&t, 1.0, 1.0), Some(3.0));
        assert_eq!(surface_zs_at(&t, 1.0, 1.0), vec![3.0]);
        assert_eq!(surface_hits_at(&t, 1.0, 1.0), vec![[3.0, 1.0]]);
    }

    #[test]
    fn a_grid_asked_often_enough_sorts_its_cells_without_changing_an_answer() {
        // 30 stacked layers; the floor-ordered lists stop early, the plain scan reads every one
        let mut v = Vec::new();
        for k in (0..30).rev() {
            v.extend(flat(k as f64 * 0.5));
        }
        let t = tris(&v);
        let plain = |x: f64, y: f64| {
            let g = build_z_grid(&t);
            assert!(g.low.get().is_none());
            surface_zs_at(&t, x, y)
                .into_iter()
                .fold(f64::INFINITY, f64::min)
        };
        let want = plain(1.0, 1.0);
        assert_eq!(want, 0.0);
        for i in 0..2100 {
            let x = 0.1 + (i % 30) as f64 * 0.1;
            assert!(surface_z_at(&t, x, 0.5).is_some());
        }
        // 2100 > LOW_FIRST_AFTER * (1 + 30/1000): the sorted lists exist now
        assert!(build_z_grid(&t).low.get().is_some());
        assert_eq!(surface_z_at(&t, 1.0, 1.0), Some(want));
        assert_eq!(surface_z_at(&t, 3.5, 0.25), Some(0.0));
    }
}
