//! Port of `web/triindex.js`. A uniform XY grid over a flat triangle array (9 numbers per
//! triangle), for the questions "which triangles could touch this box?" -- answered without
//! walking every triangle. Sway's `lowest_hit` asks it once per rib iteration, and on a
//! million-face part the brute-force scan was most of the click.
//!
//! Built once per array (cached on the `Tris`, where JS caches on the array's identity) in
//! O(n): triangles are binned by their XY bounding box into a CSR layout (one array of cell
//! starts, one of triangle ids). Z is not indexed: the boxes asked about are tall and thin.
//!
//! It only ever narrows the candidates. A triangle whose XY bounding box misses the query box
//! cannot touch the box, so skipping it changes nothing; callers still run their exact test on
//! every candidate, which is why the answers are identical to a full scan.

use crate::jsnum;
use crate::tris::Tris;

/// `RES`: cells per side; thin queries touch a few rows of them.
const RES: usize = 128;

/// The grid `triIndex` returns (`{ n, x0, y0, res, inv, start, ids, stamp, tick }`).
///
/// Divergence from JS: `stamp`/`tick` (the once-per-query visit marks) are not stored. A
/// triangle binned into several cells of the query is visited from the one cell holding the
/// lower-left corner of (its cell range clipped to the query's), which visits the same set
/// and needs no mutable state, so the index is shared between threads.
#[derive(Debug)]
pub struct TriIndex {
    pub n: usize,
    pub x0: f64,
    pub y0: f64,
    pub res: usize,
    pub inv: [f64; 2],
    pub start: Vec<i32>,
    pub ids: Vec<i32>,
}

/// Port of `triIndex` (web/triindex.js:22-26): the grid of `tris`, built on first use.
pub fn tri_index(tris: &Tris) -> &TriIndex {
    tris.tri_index.get_or_init(|| build(&tris.v))
}

/// `cellOf` (web/triindex.js:75-78): the cell column (axis 0) or row (axis 1) holding `v`,
/// clamped to the grid.
fn cell_of(g: &TriIndex, axis: usize, v: f64) -> usize {
    let c = jsnum::floor((v - if axis == 1 { g.y0 } else { g.x0 }) * g.inv[axis]);
    if c < 0.0 {
        0
    } else if c >= g.res as f64 {
        g.res - 1
    } else {
        c as usize
    }
}

/// The cells `[ca, cb] x [ra, rb]` triangle `i` is binned into (`range` in `build`).
fn range(g: &TriIndex, tris: &[f64], i: usize) -> (usize, usize, usize, usize) {
    let a = i * 9;
    let xa = jsnum::min(jsnum::min(tris[a], tris[a + 3]), tris[a + 6]);
    let xb = jsnum::max(jsnum::max(tris[a], tris[a + 3]), tris[a + 6]);
    let ya = jsnum::min(jsnum::min(tris[a + 1], tris[a + 4]), tris[a + 7]);
    let yb = jsnum::max(jsnum::max(tris[a + 1], tris[a + 4]), tris[a + 7]);
    (
        cell_of(g, 0, xa),
        cell_of(g, 0, xb),
        cell_of(g, 1, ya),
        cell_of(g, 1, yb),
    )
}

/// Port of `build` (web/triindex.js:28-73).
fn build(tris: &[f64]) -> TriIndex {
    let n = tris.len() / 9;
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut i = 0;
    while i < tris.len() {
        let (x, y) = (tris[i], tris[i + 1]);
        if x < x0 {
            x0 = x;
        }
        if x > x1 {
            x1 = x;
        }
        if y < y0 {
            y0 = y;
        }
        if y > y1 {
            y1 = y;
        }
        i += 3;
    }
    let mut g = TriIndex {
        n,
        x0,
        y0,
        res: RES,
        inv: [
            RES as f64 / jsnum::max(x1 - x0, 1e-9),
            RES as f64 / jsnum::max(y1 - y0, 1e-9),
        ],
        start: vec![0; RES * RES + 1],
        ids: Vec::new(),
    };
    for pass in 0..2 {
        let mut fill = Vec::new();
        if pass == 1 {
            for c in 0..RES * RES {
                g.start[c + 1] += g.start[c];
            }
            g.ids = vec![0; g.start[RES * RES] as usize];
            fill = g.start[..RES * RES].to_vec();
        }
        for i in 0..n {
            let (ca, cb, ra, rb) = range(&g, tris, i);
            for r in ra..=rb {
                for c in ca..=cb {
                    if pass == 0 {
                        g.start[r * RES + c + 1] += 1; // shifted by one: the prefix sum below makes it a start
                    } else {
                        g.ids[fill[r * RES + c] as usize] = i as i32;
                        fill[r * RES + c] += 1;
                    }
                }
            }
        }
    }
    g
}

/// Port of `eachCandidate` (web/triindex.js:80-95): call `visit(triangle index)` once for every
/// triangle whose XY box may overlap the given XY box. `tris` is the array `g` was built from.
pub fn each_candidate(
    g: &TriIndex,
    tris: &[f64],
    xa: f64,
    xb: f64,
    ya: f64,
    yb: f64,
    mut visit: impl FnMut(usize),
) {
    let ca = cell_of(g, 0, xa);
    let cb = cell_of(g, 0, xb);
    let ra = cell_of(g, 1, ya);
    let rb = cell_of(g, 1, yb);
    for r in ra..=rb {
        for c in ca..=cb {
            let cell = r * g.res + c;
            for k in g.start[cell]..g.start[cell + 1] {
                let id = g.ids[k as usize] as usize;
                // the once-per-query mark (JS `stamp`): only the cell at the lower-left corner of
                // the triangle's range inside the query's range visits it
                let (tca, _, tra, _) = range(g, tris, id);
                if r == tra.max(ra) && c == tca.max(ca) {
                    visit(id);
                }
            }
        }
    }
}
