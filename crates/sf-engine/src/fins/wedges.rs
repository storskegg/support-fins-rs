//! Port of `web/fins/wedges.js`. Read `docs/AGENTS.md` before changing anything here.

use indexmap::IndexMap;
use indexmap::IndexSet;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::config::Config;
use crate::cutout::{Station, Wall, cut_wall};
use crate::fins::seating::seated_part_tris;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::planes::{Patch, Xy, find_wall_patches, patch_point, patch_probe, z_at};
use crate::prop::build::{PropRecord, RasterVeto};
use crate::prop::config::TineDir;
use crate::prop::crest::{Crest, crest_kinds, emit_crest, flat_crest_ring, flat_cut};
use crate::prop::ctx::BuildCtx;
use crate::prop::surface::{surface_hits_at, surface_z_at};
use crate::prop::tines::{TineOpts, emit_tines, tine_step_for};
use crate::solids::{Radius, box_extrude, rounded_polygon};
use crate::tris::Tris;
use crate::types::{Pt3, Rot3, V4, Xyz};

/// `PERP` (web/fins/wedges.js:50). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct PerpConfig {
    /// JS key `roundFeet`.
    pub round_feet: bool,
    // wedge thickness (thin across the face)
    pub th: f64,
    // breakaway clearance under the contact (matches PROP/FIN)
    pub gap: f64,
    /// JS key `footHalf`.
    // foot flange half-width past the wedge, each side
    pub foot_half: f64,
    /// JS key `footH`.
    pub foot_h: f64,
    // mm between wedges across the face (a ROW, not a wall of plastic)
    pub pitch: f64,
    /// JS key `tStep`.
    // sampling step up the face
    pub t_step: f64,
    /// JS key `minH`.
    // skip a wedge shorter than this
    pub min_h: f64,
    // keep wedges off the very edges of the patch
    pub inset: f64,
    /// JS key `maxRow`.
    // hard cap on wedges per patch, so a wide face never sprays
    pub max_row: f64,
    /// JS key `minArea`.
    // A wedge is for a BROAD leaning face props can't reach. Below this face area
    // (or u-extent) it is a small/curved patch better left to props or draw-mode --
    // wedging it just sprays spikes (the hook's curved arm).
    pub min_area: f64,
    /// JS key `minWidth`.
    pub min_width: f64,
}

impl Default for PerpConfig {
    fn default() -> Self {
        PerpConfig {
            round_feet: false,
            th: 1.2,
            gap: 0.2,
            foot_half: 3.0,
            foot_h: 0.6,
            pitch: 24.0,
            t_step: 1.5,
            min_h: 2.0,
            inset: 2.0,
            max_row: 14.0,
            min_area: 500.0,
            min_width: 22.0,
        }
    }
}

/// `TH0` (web/fins/wedges.js:69): the tuned wedge's thickness, read once when the module loads
/// (so the default `PERP.th`, whatever a later `applyTunables` sets). A thicker blade (Wall
/// lines, nozzle.js) is also read at its edges.
fn th0() -> f64 {
    PerpConfig::default().th
}

/// The `{ patches, faceMap }` of `gripPatches`. `face_map` maps a face to the index of the
/// patch (in `patches`) that first listed it.
#[derive(Clone, Debug)]
pub struct GripPatches {
    pub patches: Vec<Patch>,
    pub face_map: IndexMap<usize, usize>,
}

/// Port of `gripPatches` (web/fins/wedges.js:29-34): DRAW mode support: every grippable face in
/// this pose, and a map from any face index to the patch it belongs to. Grip-first Draw lets
/// the user pick the face by hand, so the UI needs to know which faces CAN take a fin (to guide
/// the pointer) and, given a picked face, which patch to stand a fin against. This is
/// findWallPatches plus a face->patch index; the caller caches it per orientation and rebuilds
/// only when the part turns.
///
/// Only DOWNWARD faces are offered: a support fin is a perpendicular wedge that holds an
/// overhang up from below, so highlighting a vertical side (which the old beside-the-face fin
/// gripped) would just guide the pointer at a face the build then refuses.
pub fn grip_patches(topo: &Topology, result: &Analysis, rot: &Rot3) -> GripPatches {
    let patches: Vec<Patch> = find_wall_patches(topo, rot, &result.offset, None, None)
        .into_iter()
        .filter(|p| p.n.z < -0.05)
        .collect();
    let mut face_map = IndexMap::new();
    for (i, p) in patches.iter().enumerate() {
        for &f in &p.faces {
            face_map.entry(f).or_insert(i);
        }
    }
    GripPatches { patches, face_map }
}

/// `pushSolid` (web/fins/wedges.js:72-81): push a closed solid, flipping winding to outward if
/// its signed volume is negative.
fn push_solid(local: &[V4], out: &mut Vec<V4>) {
    let mut v = 0.0;
    let mut i = 0;
    while i < local.len() {
        let (a, b, c) = (local[i], local[i + 1], local[i + 2]);
        v += (a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
        i += 3;
    }
    if v < 0.0 {
        let mut i = 0;
        while i < local.len() {
            out.extend([local[i], local[i + 2], local[i + 1]]);
            i += 3;
        }
    } else {
        out.extend(local.iter().copied());
    }
}

/// `extrudeRing` (web/fins/wedges.js:84-97): extrude a planar ring (a list of `[x,y,z]`) by +/-
/// half along the horizontal uDir.
fn extrude_ring(ring: &[[f64; 3]], u_dir: Xy, half: f64, out: &mut Vec<V4>) {
    let n = ring.len();
    let lo: Vec<V4> = ring
        .iter()
        .map(|p| [p[0] - u_dir.x * half, p[1] - u_dir.y * half, p[2], 0.0])
        .collect();
    let hi: Vec<V4> = ring
        .iter()
        .map(|p| [p[0] + u_dir.x * half, p[1] + u_dir.y * half, p[2], 0.0])
        .collect();
    let mut local: Vec<V4> = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        local.extend([lo[i], lo[j], hi[j], lo[i], hi[j], hi[i]]);
    }
    // convex fan caps (a monotone wedge is convex)
    for i in 1..n.saturating_sub(1) {
        local.extend([hi[0], hi[i], hi[i + 1], lo[0], lo[i + 1], lo[i]]);
    }
    push_solid(&local, out);
}

/// `CUT_TOP` (web/fins/wedges.js:110).
const CUT_TOP: f64 = 0.05;

/// Port of `emitBlade` (web/fins/wedges.js:111-129): the wedge's blade, with the Cutouts pattern
/// through it when one is picked.
///
/// A cube stood on its edge at 45deg is held by wedges, not prop walls, so a wedge that only
/// ever extruded solid left Cutouts doing nothing there. The blade is a flat-topped section per
/// contact station handed to cutWall, the same as a prop wall's: the top band keeps the contact
/// and the tines solid, the bottom band sits over the foot flange, and cutWall's end posts hold
/// the two ends. When cutWall declines (pattern off, too small to be worth it) the blade is the
/// solid extrusion it always was.
fn emit_blade(
    top: &[[f64; 3]],
    ring: &[[f64; 3]],
    u_dir: Xy,
    half: f64,
    out: &mut Vec<V4>,
    cfg: &Config,
) {
    let perp = &cfg.perp;
    let mut st: Vec<Station> = Vec::new();
    let mut full: Vec<Vec<[f64; 3]>> = Vec::new();
    for i in 0..top.len() {
        let a = top[jsnum::max(0.0, i as f64 - 1.0) as usize];
        let b = top[jsnum::min((top.len() - 1) as f64, (i + 1) as f64) as usize];
        let mut rx = b[0] - a[0];
        let mut ry = b[1] - a[1];
        let rn = jsnum::hypot(&[rx, ry]);
        if rn < 1e-9 {
            // a vertical step: no (s, z) plane to cut in
            st.clear();
            break;
        }
        rx /= rn;
        ry /= rn;
        let sx = ry; // across the blade, as prop/sweep.js
        let sy = -rx;
        let p = top[i];
        let z = p[2];
        let pp = |o: f64, zz: f64| [p[0] + sx * o, p[1] + sy * o, zz];
        full.push(vec![
            pp(half, 0.0),
            pp(half, z),
            pp(-half, z),
            pp(-half, 0.0),
        ]);
        // the blade's top is flat, not a tip: ztip a hair under it keeps the top
        // band's section free of repeated corners
        st.push(Station {
            p: [p[0], p[1]],
            sx,
            sy,
            top: z,
            ztip: z - CUT_TOP,
            hole_top: None,
            tip_half: None,
            sh: None,
            bot: 0.0,
            bot_tip: perp.foot_h,
            taper_bot: false,
            bot_n: None,
            bot_p: None,
        });
    }
    let wall = Wall {
        th: perp.th,
        tip: perp.th,
        min_stations: 3.0,
    };
    if !st.is_empty() && cut_wall(&st, &full, out, &wall, cfg) {
        return;
    }
    extrude_ring(ring, u_dir, half, out);
}

/// Port of `crestSections` (web/fins/wedges.js:136-145): the interface crest along a blade's
/// `top`, as sections across it (uDir, signed the way emitBlade's sections face so the ribbon
/// winds outward), or `None` (JS `null`) when a station repeats in XY (a vertical step: no run to
/// sweep along).
fn crest_sections(
    top: &[[f64; 3]],
    u_dir: Xy,
    half: f64,
    crest: &Crest,
    cfg: &Config,
) -> Option<Vec<Option<Vec<[f64; 3]>>>> {
    let n = top.len();
    for i in 1..n {
        if jsnum::hypot(&[top[i][0] - top[i - 1][0], top[i][1] - top[i - 1][1]]) < 1e-6 {
            return None;
        }
    }
    let rx = top[n - 1][0] - top[0][0];
    let ry = top[n - 1][1] - top[0][1];
    // uDir . (ry, -rx), as sweep's sx, sy
    let sgn = if u_dir.x * ry - u_dir.y * rx >= 0.0 {
        1.0
    } else {
        -1.0
    };
    Some(
        top.iter()
            .map(|q| {
                Some(flat_crest_ring(
                    crest,
                    |o, z| [q[0] + u_dir.x * sgn * o, q[1] + u_dir.y * sgn * o, z],
                    q[2],
                    half,
                    cfg,
                ))
            })
            .collect(),
    )
}

/// Port of `emitFoot` (web/fins/wedges.js:159-206): a flat foot flange under the wedge's bed
/// footprint (a -> b at z=0).
///
/// The flange reaches `footHalf` past both ends of the run, and past the LOW end that is under
/// the part: a wedge under a cube stood on its edge ran its foot straight across the edge and
/// under the far flank, so the slicer printed foot and part as one solid region for the foot's
/// three layers -- a weld the wedge's breakaway gap and tines were meant to avoid. So when
/// `part_tris` is given the flange keeps only its longest stretch along the run where nothing of
/// the part hangs lower than footH + gap over the flange's full width (plus the gap sideways):
/// the same clearance the wedge keeps, applied to its foot.
fn emit_foot(
    a: [f64; 3],
    b: [f64; 3],
    u_dir: Xy,
    out: &mut Vec<V4>,
    part_tris: Option<&Tris>,
    cfg: &Config,
) {
    let perp = &cfg.perp;
    let sx = b[0] - a[0];
    let sy = b[1] - a[1];
    let l = jsnum::hypot(&[sx, sy]);
    if l < 1e-6 {
        return;
    }
    let ux = sx / l; // along the run (bed footprint)
    let uy = sy / l;
    let hw = perp.th / 2.0 + perp.foot_half;
    let hl = l / 2.0 + perp.foot_half;
    let cx = (a[0] + b[0]) / 2.0;
    let cy = (a[1] + b[1]) / 2.0;
    let p = |s: f64, w: f64, z: f64| [cx + ux * s + u_dir.x * w, cy + uy * s + u_dir.y * w, z];
    let mut s0 = -hl;
    let mut s1 = hl;
    if let Some(part_tris) = part_tris {
        let step = 0.1;
        let need = perp.foot_h + perp.gap;
        let reach = hw + perp.gap;
        let n_s = jsnum::ceil((2.0 * hl) / step);
        let n_w = jsnum::ceil((2.0 * reach) / step);
        let clear_at = |s: f64| -> bool {
            let mut k = 0.0;
            while k <= n_w {
                let [x, y, _] = p(s, -reach + (2.0 * reach * k) / n_w, 0.0);
                if let Some(low) = surface_z_at(part_tris, x, y)
                    && low < need
                {
                    return false;
                }
                k += 1.0;
            }
            true
        };
        let mut best: Option<[f64; 2]> = None;
        let mut start: Option<f64> = None;
        let mut i = 0.0;
        while i <= n_s {
            let s = -hl + (2.0 * hl * i) / n_s;
            let ok = clear_at(s);
            if ok && start.is_none() {
                start = Some(s);
            }
            if let Some(st) = start
                && (!ok || i == n_s)
            {
                let end = if ok { s } else { s - (2.0 * hl) / n_s };
                if best.is_none_or(|bst| end - st > bst[1] - bst[0]) {
                    best = Some([st, end]);
                }
                start = None;
            }
            i += 1.0;
        }
        // nowhere to stand a flange
        let Some(best) = best else { return };
        if best[1] - best[0] < perp.th {
            return;
        }
        // Pull back one more sample so the kept edge is clear, not the last clear sample.
        s0 = if best[0] > -hl {
            best[0] + perp.gap
        } else {
            best[0]
        };
        s1 = if best[1] < hl {
            best[1] - perp.gap
        } else {
            best[1]
        };
    }
    let rect = [[s0, -hw], [s1, -hw], [s1, hw], [s0, hw]];
    let mut local: Vec<V4> = Vec::new();
    if perp.round_feet && cfg.prop.tine_dir != TineDir::Vertical {
        box_extrude(
            &rounded_polygon(&rect, Radius::All(hw), 8.0),
            0.0,
            perp.foot_h,
            p,
            &mut local,
        );
        push_solid(&local, out);
        return;
    }
    let lo: Vec<V4> = rect.iter().map(|&[s, w]| p(s, w, 0.0).v4()).collect();
    let hi: Vec<V4> = rect
        .iter()
        .map(|&[s, w]| p(s, w, perp.foot_h).v4())
        .collect();
    for i in 0..4 {
        let j = (i + 1) % 4;
        local.extend([lo[i], lo[j], hi[j], lo[i], hi[j], hi[i]]);
    }
    for i in 1..3 {
        local.extend([hi[0], hi[i], hi[i + 1], lo[0], lo[i + 1], lo[i]]);
    }
    push_solid(&local, out);
}

/// Port of `columnClear` (web/fins/wedges.js:217-230): is a wedge at this u standable AND
/// uninterrupted by a bore? Two conditions:
///   - its first contiguous contact run (from the bed up) is tall enough to matter;
///   - the face does NOT resume ABOVE a gap -- material / void / material is a BORE punched
///     through the face, and a column there dies at the void (the angle-bracket bug). A clean
///     top is material / void / END (past the top), which is fine. The resume must persist a
///     couple of samples so a sliver gap in the triangulation is not mistaken for a hole.
fn column_clear(p: &Patch, u: f64, cfg: &Config) -> bool {
    let perp = &cfg.perp;
    let n_t = jsnum::max(2.0, jsnum::ceil((p.t1 - p.t0) / perp.t_step));
    let (mut started, mut ended) = (false, false);
    let (mut top, mut resume) = (0.0, 0);
    let mut i = 0.0;
    while i <= n_t {
        let t = p.t0 + ((p.t1 - p.t0) * i) / n_t;
        i += 1.0;
        let Some(dev) = patch_probe(p, u, t) else {
            if started {
                ended = true;
            }
            continue;
        };
        if ended {
            resume += 1; // material above a gap
            continue;
        }
        started = true;
        let z = z_at(p, dev, t) - perp.gap;
        if z > top {
            top = z;
        }
    }
    top >= perp.min_h && resume < 2
}

/// Port of `perpColumns` (web/fins/wedges.js:239-266): the u positions to stand wedges at across
/// [lo, hi]. With no hole this is the old even row (round(span/pitch) columns). A bore/slot
/// splits the standable u's into BANDS on either side of it; each band gets its own row, so a
/// drawn or auto fin lands as two fins FLANKING the bore instead of one column dying at the void.
/// (The bore's own ceiling is a separate overhang; a part-attached wall serves it.)
pub fn perp_columns(p: &Patch, lo: f64, hi: f64, pitch: f64, cfg: &Config) -> Vec<f64> {
    let perp = &cfg.perp;
    let span = hi - lo;
    if span <= 0.0 {
        return Vec::new();
    }
    let n_s = jsnum::max(2.0, jsnum::ceil(span / 1.0)) as usize;
    let mut clear = Vec::new();
    for i in 0..=n_s {
        clear.push(column_clear(p, lo + (span * i as f64) / n_s as f64, cfg));
    }

    // contiguous clear samples -> u-bands (the void is the gap between them)
    let mut bands: Vec<[f64; 2]> = Vec::new();
    let mut s: i64 = -1;
    for i in 0..=n_s {
        if clear[i] && s < 0 {
            s = i as i64;
        }
        if s >= 0 && (!clear[i] || i == n_s) {
            let e = if clear[i] { i as i64 } else { i as i64 - 1 };
            if e >= s {
                bands.push([
                    lo + (span * s as f64) / n_s as f64,
                    lo + (span * e as f64) / n_s as f64,
                ]);
            }
            s = -1;
        }
    }
    if bands.is_empty() {
        bands.push([lo, hi]); // nothing read as clear: fall back to one row
    }

    let mut cols = Vec::new();
    for [a, b] in bands {
        let w = b - a;
        let m = jsnum::max(1.0, jsnum::min(perp.max_row, jsnum::round(w / pitch)));
        let mut k = 0.0;
        while k < m {
            cols.push(if m == 1.0 {
                (a + b) / 2.0
            } else {
                a + (w * k) / (m - 1.0)
            });
            k += 1.0;
        }
    }
    // `cols.slice(0, PERP.maxRow)`
    let end = jsnum::trunc(perp.max_row);
    if end < 0.0 {
        let keep = jsnum::max(0.0, cols.len() as f64 + end) as usize;
        cols.truncate(keep);
    } else if (end as usize) < cols.len() {
        cols.truncate(end as usize);
    }
    cols
}

/// Port of `clearRun` (web/fins/wedges.js:288-331): the longest run of contact stations whose
/// blade column stands clear of the part.
///
/// A wedge drops straight from the face it holds to the bed, so any of the part under that face
/// is in its way: a channel stood at 45deg (GH #254) got a wedge under its upper plate that ran
/// straight through the lower one -- fused into the part, impossible to break away. A station is
/// clear when nothing of the part faces UP below the blade's top, under the blade and the gap
/// past its ends: the face the wedge holds faces down, so an up-facing surface under it is other
/// material. Not past the blade's sides, and not between the top and the contact: there a steep
/// side wall beside the blade (ssdMounts X45), a slot the blade stands in (bridge Y45) or the
/// face meeting the contact's end (needle X45) reads as up-facing and dropped wedges standing
/// clear of the part.
///
/// A clearly down-facing surface more than DIG under the top is in the way too: the held patch
/// is a plane fit, and past the real face (bridge Y45) the blade's top rides air over more of
/// the part. Shallower, or near-vertical, is the held face itself curving past the blade's edges
/// (cylinder, tube), as a wedge always did. The part's own bottom on the bed is under nothing, so
/// it never counts.
fn clear_run(
    contact: &[[f64; 3]],
    u_dir: Xy,
    half: f64,
    part_tris: &Tris,
    cfg: &Config,
) -> Vec<[f64; 3]> {
    let gap = cfg.perp.gap;
    let min_h = cfg.perp.min_h;
    let n = contact.len();
    let e = half - 0.05;
    const STEP: f64 = 0.25;
    const DIG: f64 = 1.0;
    // Across: rays STEP apart, so a rib under a wide blade (Wall lines, nozzle.js) can't
    // pass between them. Along: each station owns the run halfway to its neighbours
    // (the gap past an end), rays STEP apart, so a one-perimeter wall can't sit
    // between stations 1.5 mm apart up the face. The top follows the run between them.
    let n_s = jsnum::max(2.0, jsnum::ceil((2.0 * e) / STEP)) as usize;
    let across: Vec<f64> = (0..=n_s)
        .map(|k| -e + (2.0 * e * k as f64) / n_s as f64)
        .collect();
    let blocked = |w: [f64; 3], to: [f64; 3], reach: f64| -> bool {
        let d = jsnum::hypot(&[to[0] - w[0], to[1] - w[1]]);
        // a vertical step: the station alone
        let n_a = if d < 1e-9 {
            0.0
        } else {
            jsnum::max(1.0, jsnum::ceil(reach / STEP))
        };
        let mut k = 0.0;
        while k <= n_a {
            let f = if n_a != 0.0 {
                (reach * k) / n_a / d
            } else {
                0.0
            };
            k += 1.0;
            let x0 = w[0] + (to[0] - w[0]) * f;
            let y0 = w[1] + (to[1] - w[1]) * f;
            let z_top = w[2] + (to[2] - w[2]) * f - gap;
            for &s in &across {
                for [z, nz] in surface_hits_at(part_tris, x0 + u_dir.x * s, y0 + u_dir.y * s) {
                    if if nz > 0.0 {
                        z < z_top
                    } else {
                        nz < -0.3 && z < z_top - DIG
                    } {
                        return true;
                    }
                }
            }
        }
        false
    };
    // past an end: the gap outward, along the run's line from the inner neighbour
    let beyond = |w: [f64; 3], inner: [f64; 3]| {
        [
            2.0 * w[0] - inner[0],
            2.0 * w[1] - inner[1],
            2.0 * w[2] - inner[2],
        ]
    };
    let halfway = |w: [f64; 3], q: [f64; 3]| jsnum::hypot(&[q[0] - w[0], q[1] - w[1]]) / 2.0;
    let clear: Vec<bool> = contact
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            let back = if i > 0 {
                !blocked(w, contact[i - 1], halfway(w, contact[i - 1]))
            } else {
                !blocked(w, beyond(w, contact[1]), gap)
            };
            let fwd = if i < n - 1 {
                !blocked(w, contact[i + 1], halfway(w, contact[i + 1]))
            } else {
                !blocked(w, beyond(w, contact[n - 2]), gap)
            };
            back && fwd
        })
        .collect();
    // The longest clear run that still stands minH tall: a long low stretch under a
    // plate that cuts the column midway mustn't win over a shorter one that holds.
    let mut best: [i64; 2] = [0, -1];
    let mut s: i64 = -1;
    for i in 0..=n {
        if i < n && clear[i] {
            if s < 0 {
                s = i as i64;
            }
            continue;
        }
        if s >= 0 && i as i64 - 1 - s > best[1] - best[0] {
            let zs: Vec<f64> = contact[s as usize..i].iter().map(|w| w[2]).collect();
            if jsnum::max_of(&zs) - gap >= min_h {
                best = [s, i as i64 - 1];
            }
        }
        s = -1;
    }
    contact[best[0] as usize..(best[1] + 1) as usize].to_vec()
}

/// The `opts` of `buildPerpFins` / `wedgeVeto` (fins.js:312, the draw path's): every key may be
/// absent. `Default` is an absent `opts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WedgeOpts {
    /// `pitch` (`opts.pitch ?? PERP.pitch`).
    pub pitch: Option<f64>,
    /// `tines`: only `=== false` turns the tines off.
    pub tines: Option<bool>,
    /// `tineDensity`.
    pub tine_density: Option<f64>,
    /// `layerHeight` (`opts.layerHeight ?? PROP.tineH`).
    pub layer_height: Option<f64>,
}

/// One entry of `buildPerpFins`'s `wedges`: `{ triRange, line, height, span }`.
#[derive(Clone, Debug, PartialEq)]
pub struct WedgeRec {
    pub tri_range: [usize; 2],
    pub line: Vec<[f64; 3]>,
    pub height: f64,
    pub span: f64,
}

/// What `buildPerpFins` returns: `{ triangles, tines, count, wedges }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerpFins {
    pub triangles: Vec<V4>,
    pub tines: usize,
    pub count: usize,
    pub wedges: Vec<WedgeRec>,
}

/// Port of `buildPerpFins` (web/fins/wedges.js:337-411): tile perpendicular wedges across one
/// down-facing patch. Reads the crest and the tine capture through `ctx`.
pub fn build_perp_fins(
    p: &Patch,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    opts: &WedgeOpts,
    ctx: &BuildCtx,
) -> PerpFins {
    let cfg = ctx.cfg;
    let perp = &cfg.perp;
    let u_dir = Xy { x: p.u.x, y: p.u.y }; // horizontal, across the face (unit)
    let half = perp.th / 2.0;
    let edge = perp.th > th0(); // a thicker blade than the tuned one: read its edges too
    let lo = p.u0 + perp.inset;
    let hi = p.u1 - perp.inset;
    if hi - lo <= 0.0 {
        return PerpFins::default();
    }
    let mut out: Vec<V4> = Vec::new();
    let mut wedges: Vec<WedgeRec> = Vec::new(); // per-wedge records: { triRange, line, height, span }
    let mut tine_total = 0usize;
    let mut count = 0usize;
    let mut part_tris: Option<Arc<Tris>> = None; // seated part, built on the first foot that needs it

    for uc in perp_columns(p, lo, hi, opts.pitch.unwrap_or(perp.pitch), cfg) {
        cfg.cancel.check();
        // contact profile up the face at this u (stop at the first hole after starting)
        let mut contact: Vec<[f64; 3]> = Vec::new();
        let n_t = jsnum::max(2.0, jsnum::ceil((p.t1 - p.t0) / perp.t_step));
        let mut i = 0.0;
        while i <= n_t {
            let t = p.t0 + ((p.t1 - p.t0) * i) / n_t;
            i += 1.0;
            let Some(dev) = patch_probe(p, uc, t) else {
                if !contact.is_empty() {
                    break;
                }
                continue;
            };
            let mut w = patch_point(p, dev, uc, t);
            // The blade's top is flat across its width: on a curved face it rides the
            // lowest of the underside at its centre and at both edges, or an edge digs in.
            if edge {
                let pt = part_tris
                    .get_or_insert_with(|| seated_part_tris(topo, rot, offset))
                    .clone();
                let zc = w[2];
                for sg in [-1.0, 1.0] {
                    let mut near: Option<f64> = None; // the down-facing surface nearest the centre's contact
                    for [z, nz] in
                        surface_hits_at(&pt, w[0] + sg * half * u_dir.x, w[1] + sg * half * u_dir.y)
                    {
                        if nz < 0.0
                            && near.is_none_or(|nr| jsnum::abs(z - zc) < jsnum::abs(nr - zc))
                        {
                            near = Some(z);
                        }
                    }
                    if let Some(nr) = near
                        && nr < w[2]
                    {
                        w[2] = nr;
                    }
                }
            }
            if w[2] > 0.3 {
                contact.push(w);
            }
        }
        if contact.len() < 2 {
            continue;
        }
        let pt = part_tris
            .get_or_insert_with(|| seated_part_tris(topo, rot, offset))
            .clone();
        contact = clear_run(&contact, u_dir, half, &pt, cfg);
        if contact.len() < 2 {
            continue;
        }
        let top: Vec<[f64; 3]> = contact
            .iter()
            .map(|w| [w[0], w[1], w[2] - perp.gap])
            .collect();
        let tz: Vec<f64> = top.iter().map(|q| q[2]).collect();
        if jsnum::max_of(&tz) < perp.min_h {
            continue;
        }

        // With the interface crest on (prop/crest.js) the blade stops at the crest's
        // cut and the crest rides on it -- the whole blade or none of it: its top is
        // flat, so it can't step down where the crest ends. 'flat' crests a wedge only
        // under a level contact (a wedge serves a leaning face, so rarely). A vertical
        // step in the contact has no run to sweep the crest along: one body.
        let kinds = crest_kinds(&ctx.crest, &contact, cfg);
        let crest = match kinds {
            Some(k) if k.iter().all(|x| x.is_some()) => {
                crest_sections(&top, u_dir, half, &ctx.crest, cfg)
            }
            _ => None,
        };
        let blade: Vec<[f64; 3]> = if crest.is_some() {
            top.iter()
                .map(|q| [q[0], q[1], flat_cut(&ctx.crest, q[2], cfg)])
                .collect()
        } else {
            top.clone()
        };
        // ring: up the bed edge, along the top, down the bed edge (closes along the bed)
        let last = blade[blade.len() - 1];
        let mut ring = vec![[blade[0][0], blade[0][1], 0.0]];
        ring.extend(blade.iter().copied());
        ring.push([last[0], last[1], 0.0]);
        let before = out.len();
        emit_blade(&blade, &ring, u_dir, half, &mut out, cfg);
        if let Some(c) = &crest {
            emit_crest(c, &mut out);
        }
        let tl = top[top.len() - 1];
        emit_foot(
            [top[0][0], top[0][1], 0.0],
            [tl[0], tl[1], 0.0],
            u_dir,
            &mut out,
            Some(&pt),
            cfg,
        );
        // a tine is one layer tall, as a wall's (prop.js): at 0.2 on a 0.6 layer the
        // slicer drops it or merges it, and the reach grows with it (prop/tines.js)
        if opts.tines != Some(false) {
            tine_total += emit_tines(
                &contact,
                None,
                topo,
                rot,
                offset,
                &mut out,
                TineOpts {
                    step: Some(tine_step_for(opts.tine_density, cfg)),
                    tine_h: Some(opts.layer_height.unwrap_or(cfg.prop.tine_h)),
                    ..Default::default()
                },
                ctx,
            );
        }
        if out.len() > before {
            count += 1;
            // One wedge = ring + foot + its tines, all pushed contiguously since
            // emitTines ran inside this loop iteration. Record the range so the UI
            // can address/remove this individual wedge.
            let h_max = jsnum::max_of(&top.iter().map(|q| q[2]).collect::<Vec<_>>());
            let cl = contact[contact.len() - 1];
            let sp = jsnum::hypot(&[cl[0] - contact[0][0], cl[1] - contact[0][1]]);
            wedges.push(WedgeRec {
                tri_range: [before, out.len()],
                line: contact,
                height: h_max,
                span: sp,
            });
        }
    }
    PerpFins {
        triangles: out,
        tines: tine_total,
        count,
        wedges,
    }
}

/// Port of `propServesPatch` (web/fins/wedges.js:420-440): does a prop wall already stand under
/// this patch's footprint? Decided in space, not by face index: a prop `line` is a centreline of
/// `[x,y,z]` points, and the patch's world footprint is the xy bbox of its four (u,t) corners. If
/// any prop point lands in that box (plus a small margin) the patch is already served and a wedge
/// would just double it.
pub fn prop_serves_patch(p: &Patch, props: &[PropRecord], cfg: &Config) -> bool {
    if props.is_empty() {
        return false;
    }
    let (mut x_lo, mut x_hi, mut y_lo, mut y_hi) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for u in [p.u0, p.u1] {
        for t in [p.t0, p.t1] {
            let q = patch_point(p, 0.0, u, t);
            if q[0] < x_lo {
                x_lo = q[0];
            }
            if q[0] > x_hi {
                x_hi = q[0];
            }
            if q[1] < y_lo {
                y_lo = q[1];
            }
            if q[1] > y_hi {
                y_hi = q[1];
            }
        }
    }
    let m = 8.0;
    for q in props {
        for pt in &q.line {
            // A tall wall's low TAIL (prop/clearance.js withLowTails) runs down into the corner
            // where this face may meet the one the wall serves; its sub-minHeight tip
            // landing in the margin is not a wall under this face (it dropped the wedge
            // on a steep cube face). Squat props are low by design and still count.
            if !q.squat && pt[2] < cfg.prop.min_height {
                continue;
            }
            if pt[0] >= x_lo - m && pt[0] <= x_hi + m && pt[1] >= y_lo - m && pt[1] <= y_hi + m {
                return true;
            }
        }
    }
    false
}

/// Port of `wedgeVeto` (web/fins/wedges.js:452-462): the veto buildProps applies to raster walls
/// (web/prop/raster.js) in auto mode: a raster wall may not stand under a patch the normal
/// pass's walls leave to a WEDGE, because the wedge's tines grip from the bed. bore_bracket X45:
/// a raster wall under the leaning face dropped its wedge and the part's lowest grip rose
/// 1.2 -> 22.9 mm. Only patches where a wedge actually builds count -- a veto on every broad
/// down-facing patch also refused the dome and bowl their raster walls. `patches` = the wedge
/// candidates (down-facing, broad), as buildFins filters them; `wedge_opts` = buildPerpFins's
/// options there.
///
/// Divergences from the JS closure, both forced by `RasterVeto` being `Rc<dyn Fn>` (`'static`):
/// - it owns what it reads: `topo` is an `Arc`, `cfg` and `crest` are snapshots taken now (JS reads
///   the live module state, which buildFins sets once before buildProps and does not change
///   during it), and each call builds its own [`BuildCtx`];
/// - the JS memo (`normalProps !== seen`, array identity) compares the props by content, which is
///   the same wherever the array is the one finished normal pass.
pub fn wedge_veto(
    patches: Vec<Patch>,
    topo: Arc<Topology>,
    rot: Rot3,
    offset: Xyz,
    wedge_opts: WedgeOpts,
    cfg: Config,
    crest: Crest,
) -> RasterVeto {
    type Memo = Option<(Vec<PropRecord>, Vec<usize>)>;
    let memo: RefCell<Memo> = RefCell::new(None);
    Rc::new(move |q: &PropRecord, normal_props: &[PropRecord]| -> bool {
        let stale = match &*memo.borrow() {
            Some((seen, _)) => seen.as_slice() != normal_props,
            None => true,
        };
        if stale {
            // once per build: the patches left to wedges
            let ctx = BuildCtx::new(&cfg, crest.clone());
            let open: Vec<usize> = (0..patches.len())
                .filter(|&i| {
                    let p = &patches[i];
                    !prop_serves_patch(p, normal_props, &cfg)
                        && build_perp_fins(p, &topo, &rot, &offset, &wedge_opts, &ctx).count > 0
                })
                .collect();
            *memo.borrow_mut() = Some((normal_props.to_vec(), open));
        }
        let m = memo.borrow();
        let (_, open) = m.as_ref().unwrap();
        open.iter()
            .any(|&i| prop_serves_patch(&patches[i], std::slice::from_ref(q), &cfg))
    })
}

/// Port of `unservedAfterWedges` (web/fins/wedges.js:475-477): overhang regions still unsupported
/// once the wedges are in: neither served by a prop wall nor standing over any wedge. This used
/// to be `unserved - wedged PATCHES` -- a patch count off a region count, two different
/// segmentations -- so wedges under one region hid others nothing supports (hub_corner X45: a
/// 1025mm2 region with no support within reach reported "0 unserved"; voron_filter_housing X25
/// hid two). Credit is SPATIAL, like propServesPatch and check_stl's coverage, because a
/// wall-patch and a region don't share faces: a region counts when some wedge vertex sits within
/// maxUnsupportedSpan of one of its faces in plan and 0..3mm below it. Dropped overhangs are
/// surfaced.
pub fn unserved_after_wedges(
    topo: &Topology,
    rot: &Rot3,
    result: &Analysis,
    served_regions: &IndexSet<usize>,
    wedge_tris: &[V4],
    cfg: &Config,
) -> usize {
    bare_after_wedges(topo, rot, result, served_regions, wedge_tris, cfg).len()
}

/// Port of `bareAfterWedges` (web/fins/wedges.js:480-501): the regions unservedAfterWedges counts,
/// as indices into `result.regions`.
pub fn bare_after_wedges(
    topo: &Topology,
    rot: &Rot3,
    result: &Analysis,
    served_regions: &IndexSet<usize>,
    wedge_tris: &[V4],
    cfg: &Config,
) -> Vec<usize> {
    let pos = &topo.pos;
    let o = &result.offset;
    let span2 = cfg.prop.max_unsupported_span * cfg.prop.max_unsupported_span;
    let mut bare = Vec::new();
    'regions: for (i, g) in result.regions.iter().enumerate() {
        if served_regions.contains(&i) {
            continue;
        }
        for &f in &g.faces {
            let (mut cx, mut cy, mut cz) = (0.0, 0.0, 0.0);
            for j in 0..3 {
                let x = pos[f * 9 + j * 3];
                let y = pos[f * 9 + j * 3 + 1];
                let z = pos[f * 9 + j * 3 + 2];
                cx += (rot[0] * x + rot[3] * y + rot[6] * z + o.x) / 3.0;
                cy += (rot[1] * x + rot[4] * y + rot[7] * z + o.y) / 3.0;
                cz += (rot[2] * x + rot[5] * y + rot[8] * z + o.z) / 3.0;
            }
            for v in wedge_tris {
                if v[2] > cz - 3.0
                    && v[2] < cz + 0.5
                    && jsnum::pow(v[0] - cx, 2.0) + jsnum::pow(v[1] - cy, 2.0) <= span2
                {
                    continue 'regions;
                }
            }
        }
        bare.push(i);
    }
    bare
}
