//! Port of `web/gridsize.js`. Read `docs/AGENTS.md` before changing anything here.

use crate::jsnum;

/// `FACES_PER_CELL` (web/gridsize.js:15).
const FACES_PER_CELL: f64 = 1.0;
/// `MAX_SIDE` (web/gridsize.js:16).
const MAX_SIDE: f64 = 2048.0;
/// `MIN_SIDE` (web/gridsize.js:17).
const MIN_SIDE: f64 = 64.0;
/// `MIN_CELL_MM` (web/gridsize.js:18).
const MIN_CELL_MM: f64 = 0.5;

/// `gridDims(ea, eb, n)` (web/gridsize.js:23-30): cells along each of two axes.
///
/// The sides are clamped to 64..=2048, so the `as usize` is exact (a NaN extent is the one
/// input where JS returns NaN; the signature has no room for it and it does not occur).
pub fn grid_dims(ea: f64, eb: f64, n: f64) -> [usize; 2] {
    let ea = jsnum::max(ea, 1e-6);
    let eb = jsnum::max(eb, 1e-6);
    // square cells of side h with about n / FACES_PER_CELL of them over ea x eb
    let mut h = jsnum::sqrt((ea * eb) / jsnum::max(1.0, n / FACES_PER_CELL));
    h = jsnum::max(h, MIN_CELL_MM);
    let side = |e: f64| jsnum::max(MIN_SIDE, jsnum::min(MAX_SIDE, jsnum::ceil(e / h)));
    [side(ea) as usize, side(eb) as usize]
}
