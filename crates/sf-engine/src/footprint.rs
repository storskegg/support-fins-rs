//! Footprints on a plate (port of `web/ui/footprint.js`, local issue 052): the 1 mm cells of the bed a
//! model covers at any height -- the part, its fins and its pad -- for the overlap warning and Arrange.
//! Boxes would be too coarse: an L-shaped part, or long fins on one side, would clash with a model standing
//! clear in its corner.
//!
//! Every cell a triangle so much as touches is counted, so a footprint is never smaller than what prints: two
//! that share no cell cannot touch. A model off the millimetre grid widens by one cell along that axis for the
//! same reason.
//!
//! A grid is `{ x0, y0, w, h, bits }`: cell (i, j) is `[x0 + i, x0 + i + 1) x [y0 + j, ...)` in mm, set when
//! `bits[j * w + i]` is 1. Pure, no DOM.
//!
//! Past [`CELL_BUDGET`] cells (a part 2 m across; a 20 m one was a 400 MB grid and 7 s, a 1e7 mm one aborted the
//! process) the cells grow: a grid has a `cell` size in mm (1 unless it had to grow), its `x0`, `y0`, `w`, `h`
//! count cells of that size, and a cell is still set when anything touches it, so a coarse footprint is never
//! smaller than what prints. Such a part is far past any bed; the fine grids are untouched.
//!
//! Also here: `web/ui/platerow.js` (`plate_under`), the plates' row as numbers.

use crate::guards::JsSort;
use std::borrow::Cow;

use crate::jsnum;

/// The most cells a grid takes (4 MB). A grid that would take more is built from coarser cells.
pub const CELL_BUDGET: i64 = 4_000_000;

/// A vertex further out than this (1e9 m) is not a place a model stands: its triangle covers nothing.
const FAR: f64 = 1e12;

/// A footprint grid (`{ x0, y0, w, h, bits }`). `bx` / `by` are set only on a bed ([`bed_grid`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub x0: i64,
    pub y0: i64,
    pub w: i64,
    pub h: i64,
    pub bits: Vec<u8>,
    /// the bed's size in mm (`bedGrid` only)
    pub bed: Option<(f64, f64)>,
    /// the side of a cell in mm: 1, or more for a part too big for [`CELL_BUDGET`] 1 mm cells (`x0`, `y0`, `w`
    /// and `h` count these)
    pub cell: i64,
}

fn fdiv(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

fn cdiv(a: i64, b: i64) -> i64 {
    -(-a).div_euclid(b)
}

/// `len` zeroed bytes, or a panic (catchable, unlike the abort of a failed `vec!`) when they cannot be had.
fn zeros(len: usize) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    if v.try_reserve_exact(len).is_err() {
        panic!("footprint: out of memory for a {len}-cell grid");
    }
    v.resize(len, 0);
    v
}

/// The least cell size `>= from` (mm) at which an extent of `w_mm` x `h_mm` takes no more than
/// [`CELL_BUDGET`] cells. 1 for anything up to ~2 m square.
fn fit_cell(from: i64, w_mm: f64, h_mm: f64) -> i64 {
    let budget = CELL_BUDGET as f64;
    let cells = |c: f64| (jsnum::ceil(w_mm / c) + 2.0) * (jsnum::ceil(h_mm / c) + 2.0);
    let mut c = from.max(1) as f64;
    while cells(c) > budget {
        // jump near the answer, then creep: always strictly larger, so it ends
        let guess = jsnum::ceil(c * jsnum::sqrt(cells(c) / budget));
        c = if guess > c { guess } else { c + 1.0 };
    }
    c as i64
}

impl Grid {
    fn bit(&self, i: i64, j: i64) -> bool {
        self.bits[(j * self.w + i) as usize] != 0
    }

    /// The corner and size in mm.
    fn x0_mm(&self) -> i64 {
        self.x0 * self.cell
    }
    fn y0_mm(&self) -> i64 {
        self.y0 * self.cell
    }
    fn w_mm(&self) -> i64 {
        self.w * self.cell
    }
    fn h_mm(&self) -> i64 {
        self.h * self.cell
    }

    /// This grid on cells of `c` mm (`c >= self.cell`): a cell is set when any of the cells it covers is.
    /// Borrowed when the size is already `c`.
    fn at_cell(&self, c: i64) -> Cow<'_, Grid> {
        if c == self.cell {
            return Cow::Borrowed(self);
        }
        let s = self.cell;
        let x0 = fdiv(self.x0_mm(), c);
        let y0 = fdiv(self.y0_mm(), c);
        let w = cdiv(self.x0_mm() + self.w_mm(), c) - x0;
        let h = cdiv(self.y0_mm() + self.h_mm(), c) - y0;
        let mut bits = zeros((w * h) as usize);
        for j in 0..self.h {
            let (ya, yb) = (
                fdiv((self.y0 + j) * s, c) - y0,
                fdiv((self.y0 + j + 1) * s - 1, c) - y0,
            );
            for i in 0..self.w {
                if !self.bit(i, j) {
                    continue;
                }
                let (xa, xb) = (
                    fdiv((self.x0 + i) * s, c) - x0,
                    fdiv((self.x0 + i + 1) * s - 1, c) - x0,
                );
                for y in ya..=yb {
                    for x in xa..=xb {
                        bits[(y * w + x) as usize] = 1;
                    }
                }
            }
        }
        Cow::Owned(Grid {
            x0,
            y0,
            w,
            h,
            bits,
            bed: self.bed,
            cell: c,
        })
    }
}

/// Port of `rasterize` (web/ui/footprint.js:20-71).
///
/// The cells `tris` (flat float x,y,z triples, three vertices a triangle; `Float32Array` values held as
/// `f64`) cover, each vertex put through `m` (a column-major 4x4, as THREE's `Matrix4.elements`) when given.
/// `None` when there are none.
pub fn rasterize(tris: &[f64], m: Option<&[f64; 16]>) -> Option<Grid> {
    let n = jsnum::floor(tris.len() as f64 / 9.0) as usize;
    if n == 0 {
        return None;
    }
    let mut xs = vec![0.0f64; n * 3];
    let mut ys = vec![0.0f64; n * 3];
    // a triangle with a vertex that is not a place (NaN, infinite, past FAR) covers nothing
    let mut keep = vec![true; n];
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for v in 0..n * 3 {
        let mut x = tris[v * 3];
        let mut y = tris[v * 3 + 1];
        if let Some(m) = m {
            let z = tris[v * 3 + 2];
            (x, y) = (
                m[0] * x + m[4] * y + m[8] * z + m[12],
                m[1] * x + m[5] * y + m[9] * z + m[13],
            );
        }
        xs[v] = x;
        ys[v] = y;
        if !(x.abs() <= FAR && y.abs() <= FAR) {
            keep[v / 3] = false;
        }
    }
    for v in 0..n * 3 {
        if !keep[v / 3] {
            continue;
        }
        let (x, y) = (xs[v], ys[v]);
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
    }
    // (a mesh of nothing but triangles that are not places covers nothing)
    if min_x > max_x {
        return None;
    }
    // 1 mm cells unless they would take more than the budget (the box in mm, not the cells, decides)
    let cell = fit_cell(1, max_x - min_x, max_y - min_y);
    let cf = cell as f64;
    let x0 = jsnum::floor(min_x / cf) as i64;
    let y0 = jsnum::floor(min_y / cf) as i64;
    let w = jsnum::floor(max_x / cf) as i64 - x0 + 1;
    let h = jsnum::floor(max_y / cf) as i64 - y0 + 1;
    let mut bits = zeros((w * h) as usize);
    for t in 0..n {
        if !keep[t] {
            continue;
        }
        let a = t * 3;
        let (ax, ay, bx, by, cx, cy) = (xs[a], ys[a], xs[a + 1], ys[a + 1], xs[a + 2], ys[a + 2]);
        let i0 = jsnum::floor(jsnum::min_of(&[ax, bx, cx]) / cf) as i64 - x0;
        let i1 = jsnum::floor(jsnum::max_of(&[ax, bx, cx]) / cf) as i64 - x0;
        let j0 = jsnum::floor(jsnum::min_of(&[ay, by, cy]) / cf) as i64 - y0;
        let j1 = jsnum::floor(jsnum::max_of(&[ay, by, cy]) / cf) as i64 - y0;
        // one row or column of cells: the box is the triangle's cover already
        if i0 == i1 || j0 == j1 {
            for j in j0..=j1 {
                for i in i0..=i1 {
                    bits[(j * w + i) as usize] = 1;
                }
            }
            continue;
        }
        // else a cell is touched unless one of the edges' lines leaves it wholly outside
        // (separating axes: the box's own two are the bounds above)
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        let s = if area >= 0.0 { 1.0 } else { -1.0 };
        let edges = [[ax, ay, bx, by], [bx, by, cx, cy], [cx, cy, ax, ay]];
        for j in j0..=j1 {
            for i in i0..=i1 {
                if bits[(j * w + i) as usize] != 0 {
                    continue;
                }
                let qx = (x0 + i) as f64 * cf;
                let qy = (y0 + j) as f64 * cf;
                let mut inside = true;
                for [px, py, rx, ry] in edges {
                    // the edge's inward side, at the cell corner furthest along it
                    let nx = -(ry - py) * s;
                    let ny = (rx - px) * s;
                    let cxm = qx + if nx > 0.0 { cf } else { 0.0 };
                    let cym = qy + if ny > 0.0 { cf } else { 0.0 };
                    if (cxm - px) * nx + (cym - py) * ny < 0.0 {
                        inside = false;
                        break;
                    }
                }
                if inside {
                    bits[(j * w + i) as usize] = 1;
                }
            }
        }
    }
    Some(Grid {
        x0,
        y0,
        w,
        h,
        bits,
        bed: None,
        cell,
    })
}

/// Port of `union` (web/ui/footprint.js:74-87). The union of grids (`None`s skipped); `None` when all are.
pub fn union(grids: &[Option<&Grid>]) -> Option<Grid> {
    let gs: Vec<&Grid> = grids.iter().flatten().copied().collect();
    if gs.is_empty() {
        return None;
    }
    if gs.len() == 1 {
        return Some(gs[0].clone());
    }
    // the cells of the coarsest grid, or coarser when the grids lie too far apart for the budget
    let x_lo = gs.iter().map(|g| g.x0_mm()).min().unwrap();
    let y_lo = gs.iter().map(|g| g.y0_mm()).min().unwrap();
    let x_hi = gs.iter().map(|g| g.x0_mm() + g.w_mm()).max().unwrap();
    let y_hi = gs.iter().map(|g| g.y0_mm() + g.h_mm()).max().unwrap();
    let from = gs.iter().map(|g| g.cell).max().unwrap();
    let cell = fit_cell(from, (x_hi - x_lo) as f64, (y_hi - y_lo) as f64);
    let gs: Vec<Cow<Grid>> = gs.iter().map(|g| g.at_cell(cell)).collect();
    let x0 = gs.iter().map(|g| g.x0).min().unwrap();
    let y0 = gs.iter().map(|g| g.y0).min().unwrap();
    let w = gs.iter().map(|g| g.x0 + g.w).max().unwrap() - x0;
    let h = gs.iter().map(|g| g.y0 + g.h).max().unwrap() - y0;
    let mut bits = zeros((w * h) as usize);
    for g in &gs {
        for j in 0..g.h {
            let row = (g.y0 - y0 + j) * w + g.x0 - x0;
            for i in 0..g.w {
                if g.bits[(j * g.w + i) as usize] != 0 {
                    bits[(row + i) as usize] = 1;
                }
            }
        }
    }
    Some(Grid {
        x0,
        y0,
        w,
        h,
        bits,
        bed: None,
        cell,
    })
}

/// Port of `placed` (web/ui/footprint.js:90-103). Grid `g` (a model's own, about its spot) standing at `xy`
/// on the plate. Off the millimetre grid it covers the cells both sides of each fractional shift.
pub fn placed(g: &Grid, [x, y]: [f64; 2]) -> Grid {
    // (in cells: a coarse grid not on its own lines covers both cells either side, as a fine one does)
    let (x, y) = (x / g.cell as f64, y / g.cell as f64);
    let fx = jsnum::floor(x + 1e-9);
    let fy = jsnum::floor(y + 1e-9);
    let ex = if x - fx > 1e-6 { 1 } else { 0 };
    let ey = if y - fy > 1e-6 { 1 } else { 0 };
    if ex == 0 && ey == 0 {
        return Grid {
            x0: g.x0 + fx as i64,
            y0: g.y0 + fy as i64,
            ..g.clone()
        };
    }
    let w = g.w + ex;
    let h = g.h + ey;
    let mut bits = zeros((w * h) as usize);
    for j in 0..g.h {
        for i in 0..g.w {
            if g.bits[(j * g.w + i) as usize] == 0 {
                continue;
            }
            for dj in 0..=ey {
                for di in 0..=ex {
                    bits[((j + dj) * w + i + di) as usize] = 1;
                }
            }
        }
    }
    Grid {
        x0: g.x0 + fx as i64,
        y0: g.y0 + fy as i64,
        w,
        h,
        bits,
        bed: None,
        cell: g.cell,
    }
}

/// Port of `shared` (web/ui/footprint.js:106-117). The cells two placed grids share, as plate cells `[x, y]`
/// (mm, a cell's corner), row by row.
pub fn shared(a: Option<&Grid>, b: Option<&Grid>) -> Vec<[i64; 2]> {
    let mut out = Vec::new();
    let (Some(a), Some(b)) = (a, b) else {
        return out;
    };
    // (two grids of different cells meet on the coarser; cells come out as their corners in mm)
    let c = a.cell.max(b.cell);
    let (a, b) = (a.at_cell(c), b.at_cell(c));
    let (a, b) = (&*a, &*b);
    let x0 = a.x0.max(b.x0);
    let x1 = (a.x0 + a.w).min(b.x0 + b.w);
    let y0 = a.y0.max(b.y0);
    let y1 = (a.y0 + a.h).min(b.y0 + b.h);
    for y in y0..y1 {
        for x in x0..x1 {
            if a.bit(x - a.x0, y - a.y0) && b.bit(x - b.x0, y - b.y0) {
                out.push([x * c, y * c]);
            }
        }
    }
    out
}

/// Port of `dilate` (web/ui/footprint.js:120-137). Grid `g` grown by `r` cells every way (a square reach:
/// never less than `r` mm).
pub fn dilate(g: &Grid, r: i64) -> Grid {
    if r <= 0 {
        return g.clone();
    }
    // r mm, in cells of the grid's own size, on bigger cells when the grown grid would pass the budget
    let r = r.min(1 << 40);
    let cell = fit_cell(g.cell, (g.w_mm() + 2 * r) as f64, (g.h_mm() + 2 * r) as f64);
    let g = &*g.at_cell(cell);
    let r = cdiv(r, cell);
    let w = g.w + 2 * r;
    let h = g.h + 2 * r;
    let mut rows = zeros((w * g.h) as usize);
    let mut bits = zeros((w * h) as usize);
    for j in 0..g.h {
        for i in 0..g.w {
            if g.bits[(j * g.w + i) as usize] == 0 {
                continue;
            }
            for k in (j * w + i)..(j * w + i + 2 * r + 1) {
                rows[k as usize] = 1;
            }
        }
    }
    for j in 0..g.h {
        for i in 0..w {
            if rows[(j * w + i) as usize] == 0 {
                continue;
            }
            for k in j..=j + 2 * r {
                bits[(k * w + i) as usize] = 1;
            }
        }
    }
    Grid {
        x0: g.x0 - r,
        y0: g.y0 - r,
        w,
        h,
        bits,
        bed: None,
        cell,
    }
}

/// Port of `bedGrid` (web/ui/footprint.js:140-145). A plate's taken cells: a bed of `bx` x `by` mm centred on
/// 0, and the grids put on it.
pub fn bed_grid(bx: f64, by: f64) -> Grid {
    let x0 = -(jsnum::ceil(bx / 2.0) as i64);
    let y0 = -(jsnum::ceil(by / 2.0) as i64);
    let w = jsnum::ceil(bx / 2.0) as i64 - x0;
    let h = jsnum::ceil(by / 2.0) as i64 - y0;
    Grid {
        x0,
        y0,
        w,
        h,
        bits: zeros((w * h) as usize),
        bed: Some((bx, by)),
        cell: 1,
    }
}

/// Port of `take` (web/ui/footprint.js:148-158). Mark placed grid `g` taken on `bed` (cells off the bed are
/// dropped).
pub fn take(bed: &mut Grid, g: Option<&Grid>) {
    let Some(g) = g else { return };
    if g.cell != 1 {
        // a coarse grid: every mm of the bed under one of its set cells (the bed is 1 mm cells)
        for y in bed.y0.max(g.y0_mm())..(bed.y0 + bed.h).min(g.y0_mm() + g.h_mm()) {
            for x in bed.x0.max(g.x0_mm())..(bed.x0 + bed.w).min(g.x0_mm() + g.w_mm()) {
                if g.bit(fdiv(x, g.cell) - g.x0, fdiv(y, g.cell) - g.y0) {
                    bed.bits[((y - bed.y0) * bed.w + x - bed.x0) as usize] = 1;
                }
            }
        }
        return;
    }
    for j in 0..g.h {
        let y = g.y0 + j - bed.y0;
        if y < 0 || y >= bed.h {
            continue;
        }
        for i in 0..g.w {
            let x = g.x0 + i - bed.x0;
            if x >= 0 && x < bed.w && g.bits[(j * g.w + i) as usize] != 0 {
                bed.bits[(y * bed.w + x) as usize] = 1;
            }
        }
    }
}

/// Port of `findSpot` (web/ui/footprint.js:167-197). Where on `bed` model grid `g` (about its own spot) can
/// stand, on whole mm, with `gap` mm to everything `bed` has taken: the free spot nearest the bed's centre
/// (its box's middle there), searched every `step` mm (the JS default is 5), then within a step of it for a
/// closer one. `None` when the bed has no room.
pub fn find_spot(bed: &Grid, g: &Grid, gap: i64, step: i64) -> Option<[i64; 2]> {
    let grown = dilate(g, gap);
    let cx = g.x0_mm() as f64 + g.w_mm() as f64 / 2.0;
    let cy = g.y0_mm() as f64 + g.h_mm() as f64 / 2.0;
    let (bx, by) = bed.bed.expect("find_spot needs a bed grid");
    let hx = bx / 2.0;
    let hy = by / 2.0;
    // the whole-mm shifts that keep it on the bed
    let dx0 = jsnum::ceil(-hx - g.x0_mm() as f64) as i64;
    let dx1 = jsnum::floor(hx - g.x0_mm() as f64 - g.w_mm() as f64) as i64;
    let dy0 = jsnum::ceil(-hy - g.y0_mm() as f64) as i64;
    let dy1 = jsnum::floor(hy - g.y0_mm() as f64 - g.h_mm() as f64) as i64;
    if dx0 > dx1 || dy0 > dy1 {
        return None;
    }
    let clear = |dx: i64, dy: i64| free(bed, &grown, dx, dy);
    let ix = jsnum::round(-cx) as i64;
    let iy = jsnum::round(-cy) as i64;
    let dist = |dx: i64, dy: i64| jsnum::pow(dx as f64 + cx, 2.0) + jsnum::pow(dy as f64 + cy, 2.0);
    let mut cand: Vec<(i64, i64, f64)> = Vec::new();
    let mut dy = iy - jsnum::ceil((iy - dy0) as f64 / step as f64) as i64 * step;
    while dy <= dy1 {
        let mut dx = ix - jsnum::ceil((ix - dx0) as f64 / step as f64) as i64 * step;
        while dx <= dx1 {
            if dx >= dx0 && dy >= dy0 {
                cand.push((dx, dy, dist(dx, dy)));
            }
            dx += step;
        }
        dy += step;
    }
    // Array.sort is stable
    cand.js_sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let first = cand.iter().find(|&&(dx, dy, _)| clear(dx, dy))?;
    // the grid found it; one step around, a nearer spot may be free too
    let mut best = *first;
    for dy in (first.1 - step + 1)..(first.1 + step) {
        for dx in (first.0 - step + 1)..(first.0 + step) {
            if dx < dx0 || dx > dx1 || dy < dy0 || dy > dy1 {
                continue;
            }
            let d = dist(dx, dy);
            if d < best.2 && clear(dx, dy) {
                best = (dx, dy, d);
            }
        }
    }
    Some([best.0, best.1])
}

/// Port of `free` (web/ui/footprint.js:200-210). No cell of `g` moved by (dx, dy) is one `bed` took (cells
/// off the bed are free).
fn free(bed: &Grid, g: &Grid, dx: i64, dy: i64) -> bool {
    if g.cell != 1 {
        for y in bed.y0.max(g.y0_mm() + dy)..(bed.y0 + bed.h).min(g.y0_mm() + g.h_mm() + dy) {
            for x in bed.x0.max(g.x0_mm() + dx)..(bed.x0 + bed.w).min(g.x0_mm() + g.w_mm() + dx) {
                if g.bit(fdiv(x - dx, g.cell) - g.x0, fdiv(y - dy, g.cell) - g.y0)
                    && bed.bits[((y - bed.y0) * bed.w + x - bed.x0) as usize] != 0
                {
                    return false;
                }
            }
        }
        return true;
    }
    for j in 0..g.h {
        let y = g.y0 + j + dy - bed.y0;
        if y < 0 || y >= bed.h {
            continue;
        }
        for i in 0..g.w {
            let x = g.x0 + i + dx - bed.x0;
            if x >= 0
                && x < bed.w
                && g.bits[(j * g.w + i) as usize] != 0
                && bed.bits[(y * bed.w + x) as usize] != 0
            {
                return false;
            }
        }
    }
    true
}

/// Port of `area` (web/ui/footprint.js:213). How many cells a grid has set (its area in mm²).
pub fn area(g: Option<&Grid>) -> i64 {
    g.map_or(0, |g| {
        g.bits.iter().map(|&b| b as i64).sum::<i64>() * g.cell * g.cell
    })
}

/// Port of `pack` (web/ui/footprint.js:221-236). Arrange: a spot for each of `grids` (models' own, about their
/// spots) on a bed of `bx` x `by` mm, `gap` mm apart -- the biggest first, each at the free spot nearest the
/// centre. A model the bed has no room left for gets `None`; an empty grid stands at `[0, 0]`.
pub fn pack(grids: &[Option<&Grid>], bx: f64, by: f64, gap: i64) -> Vec<Option<[i64; 2]>> {
    let mut bed = bed_grid(bx, by);
    let mut order: Vec<usize> = (0..grids.len()).collect();
    // Array.sort is stable: the biggest first, ties keep their order
    order.sort_by(|&a, &b| area(grids[b]).cmp(&area(grids[a])));
    let mut out: Vec<Option<[i64; 2]>> = vec![None; grids.len()];
    for k in order {
        let Some(g) = grids[k] else {
            out[k] = Some([0, 0]);
            continue;
        };
        let Some(at) = find_spot(&bed, g, gap, 5) else {
            continue;
        };
        out[k] = Some(at);
        take(&mut bed, Some(&placed(g, [at[0] as f64, at[1] as f64])));
    }
    out
}

/// Port of `dragStep` (web/ui/footprint.js:248-265). A drag's step: where a model at `at` goes when the
/// pointer asks for `to`, given `clash(xy)`, the cells it would share with the others there. Never into a worse
/// clash than where it is (clear, or no worse while it is dragged out of one): the whole step when that holds
/// -- a jump to a clear spot past another model too, the way out of a ring's hole -- else as far along it as
/// holds, in half-millimetre steps, so a fast drag still stops at the other's edge; then the rest along x
/// alone or y alone, so it slides along that edge. Returns the spot (`at` itself when it stays).
pub fn drag_step(at: [f64; 2], to: [f64; 2], clash: impl Fn([f64; 2]) -> f64) -> [f64; 2] {
    let now = clash(at);
    if clash(to) <= now {
        return to;
    }
    let r = |v: f64| jsnum::round(v * 10.0) / 10.0;
    // (a drag across the plate row is a few thousand half-mm steps; capped so that a wild `to` cannot spin)
    let n = jsnum::ceil(jsnum::hypot(&[to[0] - at[0], to[1] - at[1]]) / 0.5);
    let n = if n > crate::guards::MAX_STEPS {
        crate::guards::MAX_STEPS
    } else {
        n
    };
    let mut best = at;
    let mut k = 1.0;
    while k < n {
        let p = [
            r(at[0] + (to[0] - at[0]) * k / n),
            r(at[1] + (to[1] - at[1]) * k / n),
        ];
        if clash(p) > now {
            break;
        }
        best = p;
        k += 1.0;
    }
    for xy in [[to[0], best[1]], [best[0], to[1]]] {
        if xy[0] == best[0] && xy[1] == best[1] {
            continue;
        }
        if clash(xy) <= now {
            return xy;
        }
    }
    best
}

/// Port of `plateUnder` (web/ui/platerow.js:11-18). The other plate a model dragged to `xy` (the edited
/// plate's coordinates) stands on -- its middle over that plate's `bx` x `by` bed -- as `(plate, xy)` (its
/// spot on that plate); `None` over the edited plate itself, a gap or past the row's end. `active` and `count`
/// are plate indices / counts.
pub fn plate_under(
    [x, y]: [f64; 2],
    active: f64,
    count: f64,
    pitch: f64,
    bx: f64,
    by: f64,
) -> Option<(usize, [f64; 2])> {
    let i = active + jsnum::round(x / pitch);
    if i == active || i < 0.0 || i >= count {
        return None;
    }
    let lx = x - (i - active) * pitch;
    if jsnum::abs(lx) > bx / 2.0 || jsnum::abs(y) > by / 2.0 {
        return None;
    }
    Some((i as usize, [jsnum::round(lx * 10.0) / 10.0, y]))
}

/// Port of `spot` (web/ui/models.js:168-179). Where a model of footprint `grid` (about its own spot) goes on a
/// plate whose bed is `bx` x `by` mm, `gap` mm clear of `others` (the plate's parked models' footprints, each
/// already placed): the free spot nearest the bed's centre, or past the others' right edge when the bed has
/// none. No `grid` (an empty slot) goes to the right of the others as well, at `y` 0.
pub fn spot(others: &[Option<&Grid>], grid: Option<&Grid>, bx: f64, by: f64, gap: i64) -> [f64; 2] {
    let mut bed = bed_grid(bx, by);
    for g in others {
        take(&mut bed, *g);
    }
    if let Some(at) = grid.and_then(|g| find_spot(&bed, g, gap, 5)) {
        return [at[0] as f64, at[1] as f64];
    }
    let mut right = bx / 2.0;
    for g in others.iter().flatten() {
        right = jsnum::max(right, (g.x0_mm() + g.w_mm()) as f64);
    }
    match grid {
        Some(g) => [
            jsnum::ceil(right + gap as f64 - g.x0_mm() as f64),
            jsnum::round(-(g.y0_mm() as f64 + g.h_mm() as f64 / 2.0)),
        ],
        None => [right + gap as f64, 0.0],
    }
}

/// Port of the overlap scan in `checkOverlap` (web/ui/arrange.js:118-121). The cells any two of the placed
/// `grids` share (a pair once, the earlier model first), as plate cells.
pub fn clash_cells(grids: &[Option<&Grid>]) -> Vec<[i64; 2]> {
    let mut cells = Vec::new();
    for a in 0..grids.len() {
        for b in (a + 1)..grids.len() {
            cells.extend(shared(grids[a], grids[b]));
        }
    }
    cells
}

/// Port of the placement in `arrange` (web/ui/arrange.js:153-165). Where each of `grids` (models' own footprints,
/// about their spots, none empty) goes on a `bx` x `by` bed, `gap` mm apart: [`pack`]'s spots, and a model the bed
/// has no room left for goes in a row past the bed's right edge, in the order they came.
pub fn arrange(grids: &[&Grid], bx: f64, by: f64, gap: i64) -> Vec<[f64; 2]> {
    let refs: Vec<Option<&Grid>> = grids.iter().map(|g| Some(*g)).collect();
    let at = pack(&refs, bx, by, gap);
    let mut x = bx / 2.0 + gap as f64;
    grids
        .iter()
        .zip(at)
        .map(|(g, spot)| match spot {
            Some(s) => [s[0] as f64, s[1] as f64],
            None => {
                let out = [
                    jsnum::ceil(x - g.x0_mm() as f64),
                    jsnum::round(-(g.y0_mm() as f64 + g.h_mm() as f64 / 2.0)),
                ];
                x += g.w_mm() as f64 + gap as f64;
                out
            }
        })
        .collect()
}
