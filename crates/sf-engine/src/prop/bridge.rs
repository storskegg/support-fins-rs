//! Port of `web/prop/bridge.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Run direction for walls under a near-flat overhang: ACROSS the slicer's bridge lines,
//! never along them (local issue 038).

use crate::config::Config;
use crate::inside::inside_part;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::surface::surface_z_at;
use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `MARGIN` (web/prop/bridge.js:32): grid cells read outside the patch (a post beside the face).
const MARGIN: usize = 2;
/// `LOOK` (web/prop/bridge.js:33): cells a free run's end may skip to find its anchor.
const LOOK: usize = 2;
/// `BRIDGED` (web/prop/bridge.js:34): share of the free area anchored at both ends that makes it a bridge.
const BRIDGED: f64 = 0.25;

/// The `support` object `patchTracks` and `flatRunAxis` take (web/prop.js:450):
/// `{ topo, rot, offset, sway }`. `flatRunAxis` reads the first three; `patchTracks` also
/// reads `sway` (`!!opts.sway?.on`) and `rowsOnly`.
#[derive(Clone, Copy, Debug)]
pub struct Support<'a> {
    pub topo: &'a Topology,
    pub rot: &'a Rot3,
    pub offset: &'a Xyz,
    pub sway: bool,
    /// `rowsOnly` (prop.js buildProps): lay the mid-strip rows and only flag the free ends the
    /// flush-row candidate would move (`Tracks::flushable`), so the race can judge them.
    pub rows_only: bool,
}

/// What `flatRunAxis` returns: a JS `[ux, uy]` array carrying `turned` and `old`
/// (web/prop/bridge.js:101-102). The early "old rule" returns (no part to read, or a
/// degenerate span) are plain arrays: `turned` is false and `old` is `None`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatRun {
    pub axis: [f64; 2],
    /// Not what the old rule (walls along the longer free run) picked.
    pub turned: bool,
    /// The old rule's pick, when `turned` is meaningful (`Some` whenever the bridge logic ran).
    pub old: Option<[f64; 2]>,
}

/// The per-axis scan result of `flatRunAxis`' `scan`.
#[derive(Clone, Copy)]
struct Scan {
    longest: f64,
    anchored: f64,
    free: f64,
}

/// `flatRunAxis(pts, patchTris, step, support)` (web/prop/bridge.js:42-125): `[ux, uy]` for a
/// near-flat patch, axis-aligned like the old rule. `pts` give the footprint, `patch_tris` the
/// face, `support` = `{ topo, rot, offset }` (`None` = JS `null`).
pub fn flat_run_axis(
    pts: &[[f64; 3]],
    patch_tris: &Tris,
    step: f64,
    support: Option<&Support>,
    cfg: &Config,
) -> FlatRun {
    let plain = |axis: [f64; 2]| FlatRun {
        axis,
        turned: false,
        old: None,
    };
    let mut x_lo = f64::INFINITY;
    let mut x_hi = f64::NEG_INFINITY;
    let mut y_lo = f64::INFINITY;
    let mut y_hi = f64::NEG_INFINITY;
    let mut z_sum = 0.0f64;
    for p in pts {
        if p[0] < x_lo {
            x_lo = p[0];
        }
        if p[0] > x_hi {
            x_hi = p[0];
        }
        if p[1] < y_lo {
            y_lo = p[1];
        }
        if p[1] > y_hi {
            y_hi = p[1];
        }
        z_sum += p[2];
    }
    let span_x = x_hi - x_lo;
    let span_y = y_hi - y_lo;
    // no part to read anchors from: the old rule, walls along the longer side
    let Some(support) = support.filter(|_| span_x > 0.0 && span_y > 0.0) else {
        return plain(if span_x >= span_y {
            [1.0, 0.0]
        } else {
            [0.0, 1.0]
        });
    };

    let z_mean = z_sum / pts.len() as f64;
    let gstep = jsnum::max(1.0, step);
    let nx = jsnum::min(80.0, jsnum::ceil(span_x / gstep)) as usize;
    let ny = jsnum::min(80.0, jsnum::ceil(span_y / gstep)) as usize;
    let dx = span_x / nx as f64;
    let dy = span_y / ny as f64;
    let (topo, rot, offset) = (support.topo, support.rot, support.offset);
    // FREE = face with nothing under it; ANCHOR = part in the layer below
    // (a supported cell of the face, or a post/leg just outside it)
    const FREE: u8 = 1;
    const ANCHOR: u8 = 2;
    let w = nx + 1 + 2 * MARGIN;
    let h = ny + 1 + 2 * MARGIN;
    let mut cell = vec![0u8; w * h];
    for i in 0..w {
        let x = x_lo + dx * (i as f64 - MARGIN as f64);
        for j in 0..h {
            let y = y_lo + dy * (j as f64 - MARGIN as f64);
            let z = surface_z_at(patch_tris, x, y);
            if let Some(z) = z {
                cell[i * h + j] = if inside_part(topo, rot, offset, x, y, z - 0.1) {
                    ANCHOR
                } else {
                    FREE
                };
            } else if inside_part(topo, rot, offset, x, y, z_mean - 0.3) {
                cell[i * h + j] = ANCHOR;
            }
        }
    }

    // per axis: the longest contiguous free run (the old rule's measure, now the
    // fallback) and the free length anchored at BOTH ends (what a slicer bridges)
    let scan = |along_x: bool| -> Scan {
        let n = if along_x { w } else { h };
        let m = if along_x { h } else { w };
        let d = if along_x { dx } else { dy };
        let at = |a: usize, b: usize| {
            if along_x {
                cell[a * h + b]
            } else {
                cell[b * h + a]
            }
        };
        let mut longest = 0usize;
        let mut anchored = 0usize;
        let mut free = 0usize;
        for b in 0..m {
            let mut a = 0;
            while a < n {
                if at(a, b) != FREE {
                    a += 1;
                    continue;
                }
                let mut e = a;
                while e < n && at(e, b) == FREE {
                    e += 1;
                }
                let len = e - a;
                free += len;
                if len > longest {
                    longest = len;
                }
                let mut lo = false;
                let mut hi = false;
                let mut k = 1;
                while k <= LOOK && a >= k && !lo {
                    lo = at(a - k, b) == ANCHOR;
                    k += 1;
                }
                let mut k = 0;
                while k < LOOK && e + k < n && !hi {
                    hi = at(e + k, b) == ANCHOR;
                    k += 1;
                }
                if lo && hi {
                    anchored += len;
                }
                a = e;
            }
        }
        let cross = if along_x { dy } else { dx };
        Scan {
            longest: longest as f64 * d,
            anchored: anchored as f64 * d * cross,
            free: free as f64 * d * cross,
        }
    };
    let sx = scan(true);
    let sy = scan(false);
    let pick = |want_y: bool, old_y: bool| FlatRun {
        axis: if want_y { [0.0, 1.0] } else { [1.0, 0.0] },
        turned: want_y != old_y,
        old: Some(if old_y { [0.0, 1.0] } else { [1.0, 0.0] }),
    };
    // the old rule: walls along the longer free run (a tie along x)
    let old_y = if sx.longest > 0.0 && sy.longest > 0.0 {
        sx.longest < sy.longest
    } else {
        span_x < span_y
    };
    // walls along x are as long as the free runs along x, so a direction whose
    // walls would come out under minSpan is no choice at all
    let along = |want_y: bool| -> bool {
        let len = if want_y { sy.longest } else { sx.longest };
        let other = if want_y { sx.longest } else { sy.longest };
        // (+2 cells: a run counts its boundary cells, and the wall stands clear of the part)
        if len < cfg.prop.min_span + 2.0 * jsnum::max(dx, dy) && other > len {
            !want_y
        } else {
            want_y
        }
    };
    let free = jsnum::max(sx.free, sy.free);
    if free > 0.0 && jsnum::max(sx.anchored, sy.anchored) >= BRIDGED * free {
        // bridge lines run along the better-anchored axis; walls cross them
        return pick(along(sx.anchored >= sy.anchored), old_y);
    }
    // no two-ended anchor: the slicer bridges along the longer free run, walls along the
    // shorter (with nothing free at all, the plain footprint decides). A square face (runs
    // within a cell) keeps the old pick, walls along x.
    if !(sx.longest > 0.0 && sy.longest > 0.0) {
        return pick(span_x > span_y + jsnum::max(dx, dy), old_y);
    }
    pick(along(sx.longest > sy.longest + jsnum::max(dx, dy)), old_y)
}
