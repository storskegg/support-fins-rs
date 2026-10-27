//! Port of `web/solids.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Closed-solid emitters shared by the wall builders (web/prop/) and the wall
//! cutouts (cutout.js). Pure mesh math: each takes cross-sections or a polygon
//! and pushes outward-wound triangles, as vertex triples, onto `out`.
//!
//! A JS vertex `[x, y, z]` is `[f64; 3]` and a 2D point `[f64; 2]`. A *section* (a ring of
//! vertices) is a `Vec<[f64; 3]>` (anything `Pt3`); the triangle list `out` is a flat
//! `Vec<V4>` (`[x, y, z, tag]`, tag `0.0` for what these emitters push), three vertices per
//! triangle.

use crate::jsnum;
use crate::types::{Pt3, V4};

/// The `radius` argument of `roundedPolygon`: a number for every corner, or one per corner
/// (`Array.isArray(radius) ? radius[i] : radius`).
#[derive(Clone, Copy, Debug)]
pub enum Radius<'a> {
    All(f64),
    Each(&'a [f64]),
}

impl Radius<'_> {
    /// `radius[i]`, `undefined` (NaN here: `!(r > 0)` is true for both) past the array's end.
    fn at(&self, i: usize) -> f64 {
        match self {
            Radius::All(r) => *r,
            Radius::Each(rs) => rs.get(i).copied().unwrap_or(f64::NAN),
        }
    }
}

/// Port of `roundedPolygon` (web/solids.js:7-31): inscribed corner arcs of a convex CCW
/// polygon; radii may be per corner. JS default `steps = 8`: callers pass it explicitly.
pub fn rounded_polygon(poly: &[[f64; 2]], radius: Radius, steps: f64) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let p = poly[i];
        let a = poly[(i + n - 1) % n];
        let b = poly[(i + 1) % n];
        let da = jsnum::hypot(&[a[0] - p[0], a[1] - p[1]]);
        let db = jsnum::hypot(&[b[0] - p[0], b[1] - p[1]]);
        let r = radius.at(i);
        // `!(r > 0)`: true for NaN / undefined too, which `r <= 0` would not be
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let skip = !(r > 0.0);
        if skip || da < 1e-7 || db < 1e-7 {
            out.push(p);
            continue;
        }
        let u = [(a[0] - p[0]) / da, (a[1] - p[1]) / da];
        let v = [(b[0] - p[0]) / db, (b[1] - p[1]) / db];
        let theta = jsnum::acos(jsnum::max(-1.0, jsnum::min(1.0, u[0] * v[0] + u[1] * v[1])));
        if theta < 1e-6 || std::f64::consts::PI - theta < 1e-6 {
            out.push(p);
            continue;
        }
        let d = jsnum::min_of(&[r / jsnum::tan(theta / 2.0), da * 0.49, db * 0.49]);
        let rr = d * jsnum::tan(theta / 2.0);
        let bis = jsnum::hypot(&[u[0] + v[0], u[1] + v[1]]);
        let c = [
            p[0] + (u[0] + v[0]) / bis * rr / jsnum::sin(theta / 2.0),
            p[1] + (u[1] + v[1]) / bis * rr / jsnum::sin(theta / 2.0),
        ];
        let start = jsnum::atan2(p[1] + u[1] * d - c[1], p[0] + u[0] * d - c[0]);
        let turn = std::f64::consts::PI - theta;
        let mut j = 0.0;
        while j <= steps {
            let ang = start + turn * j / steps;
            let q = [c[0] + rr * jsnum::cos(ang), c[1] + rr * jsnum::sin(ang)];
            let keep = match out.last() {
                None => true,
                Some(l) => jsnum::hypot(&[q[0] - l[0], q[1] - l[1]]) > 1e-8,
            };
            if keep {
                out.push(q);
            }
            j += 1.0;
        }
    }
    out
}

/// Port of `roundedFlange` (web/solids.js:34-87): round only the ends of a swept flange,
/// keeping the intervening contour. Each section is a ring of at least 4 vertices.
pub fn rounded_flange(sections: &[Vec<[f64; 3]>], out: &mut Vec<V4>) {
    let mut secs: Vec<Vec<[f64; 3]>> = Vec::new();
    for sec in sections {
        secs.push(sec.clone());
        while secs.len() > 2 {
            let n = secs.len();
            let (a, b, c) = (&secs[n - 3], &secs[n - 2], &secs[n - 1]);
            let d: Vec<f64> = (0..3).map(|k| c[0][k] - a[0][k]).collect();
            // `d.reduce((s, v) => s + v * v, 0)`
            let len2 = d.iter().fold(0.0, |s, v| s + v * v);
            let t = if len2 != 0.0 && !len2.is_nan() {
                d.iter()
                    .enumerate()
                    .fold(0.0, |s, (k, v)| s + v * (b[0][k] - a[0][k]))
                    / len2
            } else {
                -1.0
            };
            let on_line = b.iter().enumerate().all(|(j, p)| {
                p.iter()
                    .enumerate()
                    .all(|(k, v)| jsnum::abs(v - (a[j][k] + t * (c[j][k] - a[j][k]))) < 1e-6)
            });
            if t <= 0.0 || t >= 1.0 || !on_line {
                break;
            }
            secs.remove(n - 2);
        }
    }
    for i in 0..secs.len().saturating_sub(1) {
        let inset = |sec: &Vec<[f64; 3]>, other: &Vec<[f64; 3]>| -> Vec<[f64; 3]> {
            let len = jsnum::hypot(&[other[0][0] - sec[0][0], other[0][1] - sec[0][1]]);
            let t = jsnum::min(0.005 / jsnum::max(len, 1e-8), 0.005);
            sec.iter()
                .enumerate()
                .map(|(j, p)| {
                    let mut q = *p;
                    for k in 0..3 {
                        if k != 2 {
                            q[k] = p[k] + (other[j][k] - p[k]) * t;
                        }
                    }
                    q
                })
                .collect()
        };
        // Pull internal caps apart; the longer joint below overlaps each side.
        // Thus hosts never receive two segment solids with coincident end faces.
        let a = if i == 0 {
            secs[i].clone()
        } else {
            inset(&secs[i], &secs[i + 1])
        };
        let b = if i == secs.len() - 2 {
            secs[i + 1].clone()
        } else {
            inset(&secs[i + 1], &secs[i])
        };
        let w = jsnum::min(
            jsnum::hypot(&[a[0][0] - a[3][0], a[0][1] - a[3][1]]),
            jsnum::hypot(&[b[0][0] - b[3][0], b[0][1] - b[3][1]]),
        ) / 2.0;
        let poly = [
            [a[0][0], a[0][1]],
            [b[0][0], b[0][1]],
            [b[3][0], b[3][1]],
            [a[3][0], a[3][1]],
        ];
        let last = i == secs.len() - 2;
        let radii = [
            if i == 0 { w } else { 0.0 },
            if last { w } else { 0.0 },
            if last { w } else { 0.0 },
            if i == 0 { w } else { 0.0 },
        ];
        box_extrude(
            &rounded_polygon(&poly, Radius::Each(&radii), 8.0),
            a[0][2],
            jsnum::min(a[1][2], b[1][2]),
            |x, y, z| [x, y, z],
            out,
        );
    }
    // Short overlapping joint solids connect face-to-face segments without
    // expanding the checked contour. Follow the original sections on both sides
    // of each joint, staying below the lower of the adjoining flange roofs.
    for i in 1..secs.len().saturating_sub(1) {
        let (a, b, c) = (&secs[i - 1], &secs[i], &secs[i + 1]);
        let roof = jsnum::min_of(&[a[1][2], b[1][2], c[1][2]]);
        let near = |other: &Vec<[f64; 3]>| -> Vec<[f64; 3]> {
            let len = jsnum::hypot(&[other[0][0] - b[0][0], other[0][1] - b[0][1]]);
            let t = jsnum::min(0.01 / jsnum::max(len, 1e-8), 0.01);
            b.iter()
                .enumerate()
                .map(|(j, p)| {
                    let mut q = *p;
                    for k in 0..3 {
                        q[k] = if k == 2 {
                            if j == 1 || j == 2 { roof } else { b[0][2] }
                        } else {
                            p[k] + (other[j][k] - p[k]) * t
                        };
                    }
                    q
                })
                .collect()
        };
        let joint: Vec<[f64; 3]> = b
            .iter()
            .enumerate()
            .map(|(j, p)| [p[0], p[1], if j == 1 || j == 2 { roof } else { b[0][2] }])
            .collect();
        ribbon(&[near(a), joint, near(c)], out);
    }
}

/// Port of `ribbon` (web/solids.js:99-114): bridge a run of cross-sections into one closed
/// solid and push its triangles. Each section is a ring of `k` vertices in the same order;
/// consecutive rings are joined with quads and the two ends are fan-capped. Caps assume the
/// section is convex, which both sections below are (a rectangle, and a rectangle with a
/// tapered top).
///
/// Wound OUTWARD. Inherited from M3, where this emitted every triangle backwards:
/// the shell was closed and consistent -- euler 2, no boundary edges -- but its
/// volume came out NEGATIVE, so every normal faced into the solid. M3's own check
/// only asked whether the mesh was watertight, which it was, so this survived
/// being called validated. A slicer would read it as a hole rather than a wall.
pub fn ribbon<P: Pt3>(secs: &[Vec<P>], out: &mut Vec<V4>) {
    let k = secs[0].len();
    let mut tri = |a: &P, b: &P, c: &P| {
        out.push(a.v4());
        out.push(c.v4());
        out.push(b.v4());
    };
    for i in 0..secs.len() - 1 {
        for j in 0..k {
            let j2 = (j + 1) % k;
            tri(&secs[i][j], &secs[i][j2], &secs[i + 1][j2]);
            tri(&secs[i][j], &secs[i + 1][j2], &secs[i + 1][j]);
        }
    }
    // end caps
    for j in 1..k.saturating_sub(1) {
        tri(&secs[0][0], &secs[0][j + 1], &secs[0][j]);
        let e = &secs[secs.len() - 1];
        tri(&e[0], &e[j], &e[j + 1]);
    }
}

/// Port of `boxExtrude` (web/solids.js:123-135): extrude a CCW polygon (in the a,b plane of
/// the right-handed frame a,b,c) from c = lo to c = hi, emitting outward-wound triangles as
/// vertex triples. The twin of the old fins.js `extrude` (since removed): the winding
/// only comes out consistently outward when (a,b,c) is right-handed, which every
/// caller below guarantees by construction.
pub fn box_extrude(
    poly: &[[f64; 2]],
    lo: f64,
    hi: f64,
    p: impl Fn(f64, f64, f64) -> [f64; 3],
    out: &mut Vec<V4>,
) {
    let n = poly.len();
    let vlo: Vec<V4> = poly.iter().map(|&[a, b]| p(a, b, lo).v4()).collect();
    let vhi: Vec<V4> = poly.iter().map(|&[a, b]| p(a, b, hi).v4()).collect();
    for i in 0..n {
        let j = (i + 1) % n;
        out.extend([vlo[i], vlo[j], vhi[j]]);
        out.extend([vlo[i], vhi[j], vhi[i]]);
    }
    for i in 1..n.saturating_sub(1) {
        out.extend([vhi[0], vhi[i], vhi[i + 1]]);
        out.extend([vlo[0], vlo[i + 1], vlo[i]]);
    }
}

/// Port of `loftExtrude` (web/solids.js:141-153): `box_extrude` with a different (same-size)
/// outline at the top: a tine whose end leans with the part's surface. Same vertex order and
/// faces as `box_extrude`, so a tine is still one 36-vertex block (tests/_util.js tineBoxes).
pub fn loft_extrude(
    poly_lo: &[[f64; 2]],
    poly_hi: &[[f64; 2]],
    lo: f64,
    hi: f64,
    p: impl Fn(f64, f64, f64) -> [f64; 3],
    out: &mut Vec<V4>,
) {
    let n = poly_lo.len();
    let vlo: Vec<V4> = poly_lo.iter().map(|&[a, b]| p(a, b, lo).v4()).collect();
    let vhi: Vec<V4> = poly_hi.iter().map(|&[a, b]| p(a, b, hi).v4()).collect();
    for i in 0..n {
        let j = (i + 1) % n;
        out.extend([vlo[i], vlo[j], vhi[j]]);
        out.extend([vlo[i], vhi[j], vhi[i]]);
    }
    for i in 1..n.saturating_sub(1) {
        out.extend([vhi[0], vhi[i], vhi[i + 1]]);
        out.extend([vlo[0], vlo[i + 1], vlo[i]]);
    }
}

/// Port of `stackExtrude` (web/solids.js:158-170): `box_extrude` through a stack of outlines:
/// `polys[i]` (same vertex count and order) at `zs[i]`, zs rising. One closed solid -- a resin
/// pin that narrows to its contact tip.
pub fn stack_extrude(
    polys: &[Vec<[f64; 2]>],
    zs: &[f64],
    p: impl Fn(f64, f64, f64) -> [f64; 3],
    out: &mut Vec<V4>,
) {
    let n = polys[0].len();
    let rings: Vec<Vec<V4>> = polys
        .iter()
        .enumerate()
        .map(|(r, poly)| poly.iter().map(|&[a, b]| p(a, b, zs[r]).v4()).collect())
        .collect();
    for r in 0..rings.len().saturating_sub(1) {
        let (vlo, vhi) = (&rings[r], &rings[r + 1]);
        for i in 0..n {
            let j = (i + 1) % n;
            out.extend([vlo[i], vlo[j], vhi[j]]);
            out.extend([vlo[i], vhi[j], vhi[i]]);
        }
    }
    let (vlo, vhi) = (&rings[0], &rings[rings.len() - 1]);
    for i in 1..n.saturating_sub(1) {
        out.extend([vhi[0], vhi[i], vhi[i + 1]]);
        out.extend([vlo[0], vlo[i + 1], vlo[i]]);
    }
}
