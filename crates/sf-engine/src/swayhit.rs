//! Port of `web/swayhit.js`. Sway's "does anything on the part cross the volume this rib would
//! occupy?" test.
//!
//! `lowest_hit` answers it through the XY grid in `triindex`: only the triangles near the rib are
//! looked at, so the click costs the same on a million-face part as on a small one.
//! `lowest_hit_brute` is the original full scan, kept as the reference the tests hold the indexed
//! version to; both run the same per-triangle test, so they can only differ in which triangles
//! they skip, and the grid skips only triangles that cannot touch the box.

use crate::jsnum;
use crate::sway::Frame;
use crate::triindex::{each_candidate, tri_index};
use crate::tris::Tris;

type V3 = [f64; 3];

/// Port of `clip` (web/swayhit.js:15-28, moved from web/sway.js): Sutherland-Hodgman: keep the
/// part of `poly` where `f(v) >= 0`.
pub fn clip(poly: &[V3], f: &dyn Fn(&V3) -> f64) -> Vec<V3> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let fa = f(&a);
        let fb = f(&b);
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push([
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]);
        }
    }
    out
}

const EPS: f64 = 1e-6;

/// Port of `triLow` (web/swayhit.js:36-65): the lowest z at which triangle `i` (an offset into
/// `tris`) crosses the rib volume, or Infinity. The triangle goes into the rib's frame (s out of
/// the face, u across it, z), is clipped to the rib's slab across the face, then against its
/// (s, z) outline.
fn tri_low(
    tris: &[f64],
    i: usize,
    fr: &Frame,
    u_lo: f64,
    u_hi: f64,
    outline: &[&dyn Fn(&V3) -> f64],
) -> f64 {
    let mut v = [[0.0; 3]; 3];
    let (mut umin, mut umax) = (f64::INFINITY, f64::NEG_INFINITY);
    for (k, vk) in v.iter_mut().enumerate() {
        let x = tris[i + k * 3];
        let y = tris[i + k * 3 + 1];
        let z = tris[i + k * 3 + 2];
        vk[0] = x * fr.nh.0 + y * fr.nh.1;
        vk[1] = x * fr.u.0 + y * fr.u.1;
        vk[2] = z;
        if vk[1] < umin {
            umin = vk[1];
        }
        if vk[1] > umax {
            umax = vk[1];
        }
    }
    if umax < u_lo || umin > u_hi {
        return f64::INFINITY;
    }
    // A triangle wholly outside one outline edge clips to nothing; say so before allocating.
    // (The edges are linear, so the margin only has to beat the rounding of the clips below --
    // except sway.js's outer edge, which kinks at z = 0 where depthAt clamps; it only matters
    // for part below the seated bed, ~0.025 mm at most.)
    for f in outline {
        if f(&v[0]) < -EPS && f(&v[1]) < -EPS && f(&v[2]) < -EPS {
            return f64::INFINITY;
        }
    }
    let mut poly: Vec<V3> = v.to_vec();
    poly = clip(&poly, &|q: &V3| q[1] - u_lo);
    poly = clip(&poly, &|q: &V3| u_hi - q[1]);
    for f in outline {
        if poly.len() < 2 {
            break;
        }
        poly = clip(&poly, *f);
    }
    let mut best = f64::INFINITY;
    if poly.len() < 2 {
        return best;
    }
    for q in &poly {
        if q[2] < best {
            best = q[2];
        }
    }
    best
}

/// Port of `lowestHitBrute` (web/swayhit.js:68-72): the full scan, every triangle. The reference
/// for `lowest_hit`.
pub fn lowest_hit_brute(
    tris: &[f64],
    fr: &Frame,
    u_lo: f64,
    u_hi: f64,
    outline: &[&dyn Fn(&V3) -> f64],
) -> f64 {
    let mut best = f64::INFINITY;
    let mut i = 0;
    while i < tris.len() {
        best = jsnum::min(best, tri_low(tris, i, fr, u_lo, u_hi, outline));
        i += 9;
    }
    best
}

/// Port of `lowestHit` (web/swayhit.js:86-115): the same answer as `lowest_hit_brute`, from the
/// triangles near the box only. `s_lo`..`s_hi` must bound the outline's s (distance out of the
/// face) over the whole volume; the box in the plane is that range by [`u_lo`, `u_hi`] across.
/// `z_lo`..`z_hi` (JS defaults -Infinity / Infinity) must bound the outline's z the same way.
///
/// The grid hands over every triangle whose XY box meets the axis-aligned box around the rib's
/// footprint, which for a rib turned off-axis is much bigger than the rib. Each candidate is
/// first checked against the rib's own (s, u, z) box, cheaply and without allocating; one that
/// misses it cannot be clipped to anything, so skipping it is exact (the bounds are padded by
/// EPS so rounding never drops a touching one).
pub fn lowest_hit(
    tris: &Tris,
    fr: &Frame,
    u_lo: f64,
    u_hi: f64,
    outline: &[&dyn Fn(&V3) -> f64],
    s_lo: f64,
    s_hi: f64,
    z_lo: f64,
    z_hi: f64,
) -> f64 {
    // s = x*nh.x + y*nh.y and u = x*uDir.x + y*uDir.y; invert that 2x2 to put the box's corners in x, y.
    let (a, b, c, d) = (fr.nh.0, fr.nh.1, fr.u.0, fr.u.1);
    let det = a * d - b * c;
    if !(s_lo + s_hi).is_finite() || jsnum::abs(det) < 1e-9 {
        return lowest_hit_brute(&tris.v, fr, u_lo, u_hi, outline);
    }
    let (mut xa, mut xb, mut ya, mut yb) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for s in [s_lo, s_hi] {
        for u in [u_lo, u_hi] {
            let x = (d * s - b * u) / det;
            let y = (a * u - c * s) / det;
            if x < xa {
                xa = x;
            }
            if x > xb {
                xb = x;
            }
            if y < ya {
                ya = y;
            }
            if y > yb {
                yb = y;
            }
        }
    }
    let pad = 1e-6; // rounding in the inverse; a wider net only costs a few more candidates
    let (sa, sb) = (s_lo - EPS, s_hi + EPS);
    let (ua, ub) = (u_lo - EPS, u_hi + EPS);
    let (za, zb) = (z_lo - EPS, z_hi + EPS);
    let (nx, ny, ux, uy) = (fr.nh.0, fr.nh.1, fr.u.0, fr.u.1);
    let t = &tris.v;
    let mut best = f64::INFINITY;
    each_candidate(
        tri_index(tris),
        t,
        xa - pad,
        xb + pad,
        ya - pad,
        yb + pad,
        |id| {
            let i = id * 9;
            let (x0, y0, x1, y1, x2, y2) = (t[i], t[i + 1], t[i + 3], t[i + 4], t[i + 6], t[i + 7]);
            let (s0, s1, s2) = (x0 * nx + y0 * ny, x1 * nx + y1 * ny, x2 * nx + y2 * ny);
            if (s0 < sa && s1 < sa && s2 < sa) || (s0 > sb && s1 > sb && s2 > sb) {
                return;
            }
            let (u0, u1, u2) = (x0 * ux + y0 * uy, x1 * ux + y1 * uy, x2 * ux + y2 * uy);
            if (u0 < ua && u1 < ua && u2 < ua) || (u0 > ub && u1 > ub && u2 > ub) {
                return;
            }
            let (z0, z1, z2) = (t[i + 2], t[i + 5], t[i + 8]);
            if (z0 < za && z1 < za && z2 < za) || (z0 > zb && z1 > zb && z2 > zb) {
                return;
            }
            let z = tri_low(t, i, fr, u_lo, u_hi, outline);
            if z < best {
                best = z;
            }
        },
    );
    best
}
