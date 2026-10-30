//! Port of `web/kiss.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Tine ends that KISS the part's surface: shared by the wall tines (prop/tines.js) and the
//! sway braces' tines (sway.js). Takes the reach and widths as arguments.

use crate::inside::inside_part;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::types::{Rot3, Xyz};

/// `TRIM_MIN` (web/kiss.js:30).
const TRIM_MIN: f64 = 0.05;
/// `TRIM_Z` (web/kiss.js:30).
const TRIM_Z: f64 = 0.01;
/// `STEP` (web/kiss.js:30).
const STEP: f64 = 0.1;

/// The options object of `kissEnds`: `{ half, back, reach, contactOffset = -0.01 }`
/// (web/kiss.js:31). `Default` gives the JS default for `contactOffset`; the other three
/// have no JS default and are required (0 here only so `..Default::default()` works).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KissOpts {
    pub half: f64,
    pub back: f64,
    pub reach: f64,
    pub contact_offset: f64,
}

impl Default for KissOpts {
    fn default() -> KissOpts {
        KissOpts {
            half: 0.0,
            back: 0.0,
            reach: 0.0,
            contact_offset: -0.01,
        }
    }
}

/// The `{ bot: [left, right], top: [left, right] }` `kissEnds` returns (web/kiss.js:68-73).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KissEnds {
    pub bot: [f64; 2],
    pub top: [f64; 2],
}

/// Where a tine meets the part, so it can KISS the surface instead of running on into it.
///
/// `kissEnds(topo, rot, offset, x, y, dx, dy, zb, zt, { half, back, reach, contactOffset })`
/// (web/kiss.js:31-74).
pub fn kiss_ends(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    zb: f64,
    zt: f64,
    opts: &KissOpts,
) -> KissEnds {
    let half = opts.half;
    let back = opts.back;
    let r = opts.reach;
    let contact = if opts.contact_offset.is_finite() {
        jsnum::max(-0.2, jsnum::min(0.2, opts.contact_offset))
    } else {
        -0.01
    };
    // The end may lean out past `reach` at the bottom (see the JS comment).
    let lo0 = -back + TRIM_MIN;
    let far = 2.0 * r;
    let reach = |s: f64, z: f64| -> f64 {
        let px = x - dy * s;
        let py = y + dx * s; // across = z x along, as emitTines
        let inside = |d: f64| inside_part(topo, rot, offset, px + dx * d, py + dy * d, z);
        let bisect = |mut lo: f64, mut hi: f64, n: usize| -> f64 {
            // air at lo, solid at hi
            for _ in 0..n {
                let mid = (lo + hi) / 2.0;
                if inside(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            hi
        };
        // far probe first; a part thinner than that can put it in air past the part, so fall
        // back to `reach` itself before giving up
        let hi: Option<f64> = if inside(far) {
            Some(far)
        } else if inside(r) {
            Some(r)
        } else {
            None
        };
        if hi.is_some() && inside(lo0) {
            return lo0; // the part already reaches back over the wall
        }
        // ~0.0001 mm on a 1.25 span; no solid within reach: the old length
        let e = match hi {
            None => r,
            Some(h) => bisect(lo0, h, 14),
        };
        // That search can pass solid; walk the line up to the answer; the first solid sample
        // short of it is the face.
        let mut prev = lo0;
        let mut d = lo0 + STEP;
        while d < e - STEP / 2.0 {
            if inside(d) {
                return if prev == lo0 && inside(lo0) {
                    lo0
                } else {
                    bisect(prev, d, 10) - contact
                };
            }
            prev = d;
            d += STEP;
        }
        if hi.is_none() { r } else { e - contact }
    };
    // measured a hair inside the layer, then carried out to the tine's real bottom and top
    // along the same lean
    let k = TRIM_Z / (zt - zb - 2.0 * TRIM_Z);
    let side = |s: f64| -> [f64; 2] {
        let b = reach(s, zb + TRIM_Z);
        let t = reach(s, zt - TRIM_Z);
        let clamp = |e: f64| jsnum::min(far, jsnum::max(lo0, e));
        [clamp(b + (b - t) * k), clamp(t + (t - b) * k)]
    };
    let l = side(-half);
    let r2 = side(half);
    KissEnds {
        bot: [l[0], r2[0]],
        top: [l[1], r2[1]],
    }
}
