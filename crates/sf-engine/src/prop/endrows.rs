//! Port of `web/prop/endrows.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Flush end rows (local issue 062): the candidate layout for a patch's FREE edges.
//!
//! The mid-strip layout (vExt / n apart, half that from each edge) leaves a lip of up to half a
//! pitch past the last row; on a FREE edge (air past it, see `free_edge`) that lip is a
//! cantilever, and on the slenderness coupon a 4 mm one curled on every ledge. So the outermost
//! row MOVES OUT to a free edge, flush (PROP.edgeInset), and the rows spread evenly between the
//! two ends, the spread never wider than the old pitch -- with two or more rows, one more row
//! than before (the ends moved out, the span grew). #143 added a row beside each end instead and
//! kept the old ones, which doubled every free edge (lowledge 3 -> 5 walls); Matthew 2026-10-01:
//! same count, the end ones flush; 2026-10-23: never a wider gap than the pitch, so +1 row.
//!
//! This only LAYS the candidate. Whether it is worth its wall is the judge's call per region
//! (buildProps races it against the mid-strip rows, prop/judge.js; a tie keeps flush); it
//! replaced a slope cutoff (PROP.edgeFlatDeg, 45 deg) that guessed from the underside's angle.
//! The one cutoff left is Sway's (PROP.edgeFlatDegSway, see patchTracks): a brace is the user's
//! own ask and the judge doesn't see it.

use crate::config::Config;
use crate::inside::inside_part;
use crate::jsnum;
use crate::prop::surface::surface_z_at;
use crate::prop::tracks::Probe;

/// `rows` (the mid-strip layout across v = vLo..vHi, `n` of them, `vExt` apart in all) with the
/// outermost row moved out flush to each free end, or `None` when neither end is free (or an old
/// row already sits at it): the rows stay as they were.
///
/// `endRows(rows, vLo, vHi, probe)` (web/prop/endrows.js:27-46).
pub(crate) fn end_rows(
    rows: &[f64],
    v_lo: f64,
    v_hi: f64,
    probe: &Probe,
    cfg: &Config,
) -> Option<Vec<f64>> {
    let v_ext = v_hi - v_lo;
    let e = cfg.prop.edge_inset;
    let n = rows.len();
    let near = 2.0 * e; // an old row this close already sits at the end
    if v_ext <= 2.0 * near {
        return None;
    }
    let lo = rows[0] - v_lo > near && free_edge(probe, v_lo, -1.0, cfg);
    let hi = v_hi - rows[n - 1] > near && free_edge(probe, v_hi, 1.0, cfg);
    if !lo && !hi {
        return None;
    }
    let a = if lo { v_lo + e } else { rows[0] };
    let b = if hi { v_hi - e } else { rows[n - 1] };
    // one row can't stand at an end without leaving the rest bare: it keeps its
    // place mid-strip and the free ends get their own (as #143)
    // Spread wider than the old pitch, and the gaps get another row instead:
    // moving the ends out must not open the middle. It used to allow up to
    // 2 x maxUnsupportedSpan (a point mid-gap still in a wall's reach), which
    // left a 40 mm deck's three walls 19.4 mm apart at a 12 mm pitch, and a
    // turned ledge's (local issue 038) two rows 20 mm apart.
    let cap = v_ext / n as f64;
    let m = jsnum::max(n as f64, jsnum::ceil((b - a) / cap - 1e-9) + 1.0) as usize;
    Some(if n == 1 {
        let mut r = Vec::new();
        if lo {
            r.push(a);
        }
        r.push(rows[0]);
        if hi {
            r.push(b);
        }
        r
    } else {
        (0..m)
            .map(|w| a + ((b - a) * w as f64) / (m - 1) as f64)
            .collect()
    })
}

/// Does the patch end in AIR along its edge at v = `vEdge` (`dir` -1 for the low side, +1 for
/// the high)? Probed at five points along the edge: the underside just inside the edge gives
/// the height, and a point 0.8 mm past the edge from 2 mm below to 2 mm above that height is
/// tested for part. Part there -- a spine the ledge hangs off, the same overhang carrying on
/// into the next sub-patch, or an upright the underside meets -- makes the edge attached. Free
/// only when most probes that found an underside found air, and nothing stands in the column
/// the flush row would fill (either face, down to the bed).
///
/// `freeEdge(probe, vEdge, dir)` (web/prop/endrows.js:60-89; private there, as here).
fn free_edge(probe: &Probe, v_edge: f64, dir: f64, cfg: &Config) -> bool {
    let prop = &cfg.prop;
    let support = probe.support;
    let (mut air, mut part) = (0usize, 0usize);
    for t in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let u = probe.u_lo + (probe.u_hi - probe.u_lo) * t;
        let at = |v: f64| [probe.ux * u + probe.vx * v, probe.uy * u + probe.vy * v];
        let [ix, iy] = at(v_edge - dir * 0.3);
        let Some(z) = surface_z_at(probe.patch_tris, ix, iy) else {
            continue;
        };
        // an edge down at the bed rests on it: no lip to curl, and a flush wall there
        // is lower than any wall worth building (a cube at X20's low edge)
        if z < prop.min_height_squat {
            part += 1;
            continue;
        }
        let [ox, oy] = at(v_edge + dir * 0.8);
        let solid = [-2.0, -1.0, -0.3, 0.3, 1.0, 2.0]
            .iter()
            .any(|dz| inside_part(support.topo, support.rot, support.offset, ox, oy, z + dz));
        if solid {
            part += 1;
            continue;
        }
        // the flush row's whole column must be clear too: just outside its outer
        // face, from the underside down to the bed (lbracket Y20: the side under the
        // edge leans out, and a flush wall stood in it lower down)
        // (both faces: a deck overhanging its leg by less than a wall stands the
        // flush row against the leg -- bridge, sparse: blocked and buried)
        for side in [1.0, -1.0] {
            let [cx, cy] =
                at(v_edge - dir * prop.edge_inset + side * dir * (prop.th / 2.0 + prop.side_clear));
            let mut zz = z - 0.5;
            while zz > 0.3 {
                if inside_part(support.topo, support.rot, support.offset, cx, cy, zz) {
                    return false;
                }
                zz -= 1.0;
            }
        }
        air += 1;
    }
    air >= 2 && air > part
}
