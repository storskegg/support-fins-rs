//! Port of `web/swaycrop.js`. The part triangles a Sway brace's cross ribs can see.
//!
//! `brace_cross_rib` (prop/brace.rs) shears the whole part into the brace's lean and samples
//! part surface under the rib's footprint; on a million-face part that copy and its surface
//! grid were ~100 ms of every click, for a footprint a few mm wide. `crop_for_rib` hands it only
//! the triangles whose sheared footprint can meet the square of half-width `r` around the rib's
//! base point `o`, so the shear, the grid and the samples all run on a few thousand triangles.
//! The samples only ever look at columns inside that square, and a triangle whose sheared XY box
//! misses the square covers none of them, so the answers are the same as on the whole part.
//!
//! The scan is one allocation-free pass over the triangles (no grid: the shear moves every
//! vertex by the lean times its height, so an unsheared XY index can't answer it).

use crate::tris::Tris;

/// `PAD`: the column test accepts barycentrics a hair outside; this covers it.
const PAD: f64 = 1e-6;

/// Port of `cropForRib` (web/swaycrop.js:19-37): a copy of the triangles in `tris` (9 numbers
/// each) whose XY box, sheared by `lean` per mm of height above `z_bed`, meets `o` +- `r`, and
/// that reach below `z_top` (a surface height above it is never counted) and above the bed.
/// JS defaults: `zBed = 0`, `zTop = Infinity`.
pub fn crop_for_rib(
    tris: &[f64],
    o: [f64; 2],
    r: f64,
    lean: [f64; 2],
    z_bed: f64,
    z_top: f64,
) -> Tris {
    let (xa, xb) = (o[0] - r - PAD, o[0] + r + PAD);
    let (ya, yb) = (o[1] - r - PAD, o[1] + r + PAD);
    let ztop = z_top - z_bed + PAD;
    let zlow = -0.05; // rib samples count surface from zBed - 0.01 up
    let (lx, ly) = (lean[0], lean[1]);
    let mut out: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < tris.len() {
        let t = &tris[i..i + 9];
        i += 9;
        let (z0, z1, z2) = (t[2] - z_bed, t[5] - z_bed, t[8] - z_bed);
        if (z0 > ztop && z1 > ztop && z2 > ztop) || (z0 < zlow && z1 < zlow && z2 < zlow) {
            continue;
        }
        let (x0, x1, x2) = (t[0] - lx * z0, t[3] - lx * z1, t[6] - lx * z2);
        if (x0 < xa && x1 < xa && x2 < xa) || (x0 > xb && x1 > xb && x2 > xb) {
            continue;
        }
        let (y0, y1, y2) = (t[1] - ly * z0, t[4] - ly * z1, t[7] - ly * z2);
        if (y0 < ya && y1 < ya && y2 < ya) || (y0 > yb && y1 > yb && y2 > yb) {
            continue;
        }
        out.extend_from_slice(t);
    }
    Tris::new(out)
}
