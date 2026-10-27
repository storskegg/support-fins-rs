//! Port of `web/drawarc.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Curved drawn walls (local issue 063): the plan path of a wall drawn with three
//! clicks -- its two ends, then a point it runs through. The path is the circular
//! arc through all three, so one wall can go well past a half circle (push the third
//! click out beyond the ends), and its radius is one number the minimum-radius rule
//! reads exactly.
//!
//! Geometry only: draw.js's drawnLine turns these stations into a contact line (the
//! same surface pick and settle passes as a straight wall), ui/walls.js picks the points.

use crate::jsnum;

/// `DRAW_MIN_RADIUS` (web/drawarc.js:19): the tightest a drawn curve may bend, in mm of
/// centre-line radius. The wall's foot reaches PROP.footMax (3 mm) either side of the line, so
/// under that its inner edge folds back on itself; the rest is room for the stem and tines on
/// the inside.
pub const DRAW_MIN_RADIUS: f64 = 5.0;

/// `STRAIGHT_R` (web/drawarc.js:22): past this radius (mm) the "arc" is a straight line to
/// print precision.
const STRAIGHT_R: f64 = 1e4;

/// The circle `arcThrough` returns (web/drawarc.js:28-39): `{ cx, cy, r, a0, sweep, toM }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    /// start angle
    pub a0: f64,
    /// signed sweep (rad) from a to b the way that passes m
    pub sweep: f64,
    /// counter-clockwise angle from a to m, in [0, 2pi)
    pub to_m: f64,
}

/// Port of `arcThrough` (web/drawarc.js:33-48): the circle through a, b and m in plan (each
/// `[x, y, ...]`: only x and y are read), or `None` when the three are (near enough) in line.
pub fn arc_through(a: [f64; 2], b: [f64; 2], m: [f64; 2]) -> Option<Arc> {
    let d = 2.0 * (a[0] * (b[1] - m[1]) + b[0] * (m[1] - a[1]) + m[0] * (a[1] - b[1]));
    if jsnum::abs(d) < 1e-9 {
        return None;
    }
    let sa = a[0] * a[0] + a[1] * a[1];
    let sb = b[0] * b[0] + b[1] * b[1];
    let sm = m[0] * m[0] + m[1] * m[1];
    let cx = (sa * (b[1] - m[1]) + sb * (m[1] - a[1]) + sm * (a[1] - b[1])) / d;
    let cy = (sa * (m[0] - b[0]) + sb * (a[0] - m[0]) + sm * (b[0] - a[0])) / d;
    let r = jsnum::hypot(&[a[0] - cx, a[1] - cy]);
    // `!(r < STRAIGHT_R)`: also true for NaN
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let straight = !(r < STRAIGHT_R);
    if straight {
        return None;
    }
    let tau = 2.0 * std::f64::consts::PI;
    let ang = |p: [f64; 2]| jsnum::atan2(p[1] - cy, p[0] - cx);
    let a0 = ang(a);
    let to_b = ((ang(b) - a0) % tau + tau) % tau;
    let to_m = ((ang(m) - a0) % tau + tau) % tau;
    Some(Arc {
        cx,
        cy,
        r,
        a0,
        sweep: if to_m < to_b { to_b } else { to_b - tau },
        to_m,
    })
}

/// Port of `arcStations` (web/drawarc.js:56-77): stations ~`step` apart round the arc from a
/// to b through m, each `[x, y, hint]`: `hint` the height the clicks imply there (a to m to b,
/// linear in angle), which drawnLine snaps to the nearest surface. At least `min_pts` stations.
/// `None` when the three are in line -- the caller lays the straight wall.
pub fn arc_stations(
    a: [f64; 3],
    b: [f64; 3],
    m: [f64; 3],
    step: f64,
    min_pts: f64,
) -> Option<Vec<[f64; 3]>> {
    let c = arc_through([a[0], a[1]], [b[0], b[1]], [m[0], m[1]])?;
    let n = jsnum::max(min_pts - 1.0, jsnum::ceil(jsnum::abs(c.sweep) * c.r / step));
    // m's place along the arc, 0..1 (toM is counter-clockwise; a clockwise arc meets m
    // the other way round)
    let fm = if c.sweep > 0.0 {
        c.to_m / c.sweep
    } else {
        (2.0 * std::f64::consts::PI - c.to_m) / -c.sweep
    };
    let mut out = Vec::new();
    let mut k = 0.0;
    while k <= n {
        let t = k / n;
        let th = c.a0 + c.sweep * t;
        let hint = if t <= fm {
            a[2] + (m[2] - a[2]) * (if fm > 0.0 { t / fm } else { 1.0 })
        } else {
            m[2] + (b[2] - m[2]) * ((t - fm) / (1.0 - fm))
        };
        out.push([
            c.cx + c.r * jsnum::cos(th),
            c.cy + c.r * jsnum::sin(th),
            hint,
        ]);
        k += 1.0;
    }
    // the ends exactly where clicked, not a rounding away
    let last = out.len() - 1;
    out[0] = [a[0], a[1], a[2]];
    out[last] = [b[0], b[1], b[2]];
    Some(out)
}
