//! Port of `web/prop/attached.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Part-attached walls: an overhang above another part of the part stands its wall on that floor
//! (mold.rs's `floor_line`) instead of stilting to the plate through it, and refuses a buried
//! column (`buried_column`) or one a side wall cuts through (`clear_between`). A wall inside a
//! bore is built and flagged `in_bore`. `build_part_attached` is the verdict the auto-placer acts
//! on.
//!
//! The floor under a wall (`floor_line`) lives in mold.rs.
//!
//! `build_part_attached` sweeps the wall, which reads crest state, so it takes
//! `ctx: &BuildCtx` last in place of `cfg` (docs/AGENTS.md type mapping).

use indexmap::IndexMap;

use crate::inside::inside_part;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::clearance::longest_run;
use crate::prop::contact::{contour_top, lower_sag, settle_top};
use crate::prop::ctx::BuildCtx;
use crate::prop::mold::{FloorPt, floor_line, mold_line};
use crate::prop::sweep::sweep_between;
use crate::tris::Tris;
use crate::types::{Rot3, V4, Xyz};

/// `PART_BAND` (web/prop/attached.js:24): how far below the clicked/probed overhang a settle
/// pass may still pull the top down when looking for a part-attached support. Comfortably covers
/// an overhang's own slope over a wall's length, and stays well under the smallest
/// floor-to-overhang gap worth supporting -- so the top settles on the overhang, never its
/// floor. Shared with draw.js so the two paths measure a part-attached wall alike.
pub const PART_BAND: f64 = 3.0;

/// `BORE.radius` (web/prop/attached.js:27): a cavity narrower than ~2x this reads as a bore/slot.
const BORE_RADIUS: f64 = 10.0;
/// `BORE.dirs`: compass rays cast outward from the support column.
const BORE_DIRS: f64 = 8.0;
/// `BORE.walledMin`: this many hitting part within `radius` = enclosed.
const BORE_WALLED_MIN: usize = 6;
/// `BORE.step`: mm along each ray.
const BORE_STEP: f64 = 0.5;

/// `zMidAt` (web/prop/attached.js:33).
fn z_mid_at(top: &[[f64; 3]], floor: &[FloorPt], k: usize, gap: f64) -> f64 {
    (floor[k].z + (top[k][2] - gap)) / 2.0
}

/// Port of `buriedColumn` (web/prop/attached.js:36-38): a column whose own centreline is inside
/// the part is buried, not standable.
fn buried_column(
    top: &[[f64; 3]],
    floor: &[FloorPt],
    k: usize,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    gap: f64,
) -> bool {
    inside_part(
        topo,
        rot,
        offset,
        top[k][0],
        top[k][1],
        z_mid_at(top, floor, k, gap),
    )
}

/// Port of `enclosedColumn` (web/prop/attached.js:52-66): is the support column at station `k`
/// enclosed by part walls -- standing inside a bore or narrow slot? Cast `dirs` horizontal rays
/// out from the column's centreline at mid-height and count how many strike part within
/// `radius`: an open ledge-over-base has air on some sides, a bore is walled nearly all round.
///
/// This USED to refuse the wall. Matthew's print tests (2026-09-27) reversed that: a bore gets
/// supported, and the wall runs along the bore (the contact line under a bore's ceiling follows
/// its axis) so it pulls out an open end. Now it only labels the wall `inBore`, which Suggest
/// orientation weighs as a mild preference for poses that point the holes up.
fn enclosed_column(
    top: &[[f64; 3]],
    floor: &[FloorPt],
    k: usize,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    gap: f64,
) -> bool {
    let p = top[k];
    let z_mid = z_mid_at(top, floor, k, gap);
    let mut walled = 0;
    let mut d = 0.0;
    while d < BORE_DIRS {
        let ang = (d / BORE_DIRS) * 2.0 * std::f64::consts::PI;
        let (dx, dy) = (jsnum::cos(ang), jsnum::sin(ang));
        let mut r = BORE_STEP;
        while r <= BORE_RADIUS {
            if inside_part(topo, rot, offset, p[0] + dx * r, p[1] + dy * r, z_mid) {
                walled += 1;
                break;
            }
            r += BORE_STEP;
        }
        d += 1.0;
    }
    walled >= BORE_WALLED_MIN
}

/// Port of `clearBetween` (web/prop/attached.js:75-94): can a PART-ATTACHED wall at station `k`
/// stand between its floor and the overhang without piercing a side wall? Mirror of
/// stationIsClear, but bounded to the (floor, top) span the wall actually occupies -- it probes
/// STRICTLY between the ends, since the bottom is meant to weld to the floor and the top to
/// break away under the overhang, and probing those would read the intended contacts as welds.
#[allow(clippy::too_many_arguments)]
fn clear_between(
    top: &[[f64; 3]],
    floor: &[FloorPt],
    k: usize,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    th: f64,
    side_clear: f64,
    gap: f64,
) -> bool {
    let p = top[k];
    let a = top[jsnum::max(0.0, k as f64 - 1.0) as usize];
    let b = top[jsnum::min((top.len() - 1) as f64, (k + 1) as f64) as usize];
    let rx = b[0] - a[0];
    let ry = b[1] - a[1];
    let rn = jsnum::hypot(&[rx, ry]);
    if rn < 1e-9 {
        return true;
    }
    let (sx, sy) = (ry / rn, -rx / rn);
    let (z_top, z_bot) = (p[2] - gap, floor[k].z);
    let n_p = jsnum::max(3.0, jsnum::ceil((z_top - z_bot) / 1.5));
    let mut i = 1.0;
    while i < n_p {
        // strictly interior heights
        let z = z_bot + ((z_top - z_bot) * i) / n_p;
        i += 1.0;
        for m in [0.12, 0.24, side_clear] {
            let w = th / 2.0 + m;
            if inside_part(topo, rot, offset, p[0] + sx * w, p[1] + sy * w, z) {
                return false;
            }
            if inside_part(topo, rot, offset, p[0] - sx * w, p[1] - sy * w, z) {
                return false;
            }
        }
    }
    true
}

/// Why a part-attached refusal was counted: the plate path's own skip names (JS strings
/// `'buried'`, `'blocked'` = side walls in the way, `'stub'` = too short or too low,
/// `'degenerate'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Why {
    Buried,
    Blocked,
    Stub,
    Degenerate,
}

impl Why {
    /// The JS string.
    pub fn as_str(self) -> &'static str {
        match self {
            Why::Buried => "buried",
            Why::Blocked => "blocked",
            Why::Stub => "stub",
            Why::Degenerate => "degenerate",
        }
    }
}

/// The `prop` of a built part-attached wall: `{ span, height, stations, volume, partAttached:
/// true, inBore, line }`.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachedProp {
    pub span: f64,
    pub height: f64,
    pub stations: usize,
    pub volume: f64,
    /// JS `partAttached: true`, always.
    pub part_attached: bool,
    pub in_bore: bool,
    pub line: Vec<[f64; 3]>,
}

/// The three verdicts of `buildPartAttached`.
#[derive(Clone, Debug, PartialEq)]
pub enum Attached {
    /// `{ ok: true, prop }`: a part-attached wall was built into `out`.
    Built(AttachedProp),
    /// `{ floored: why }`: there IS a floor here (an over-the-part overhang) but no safe wall
    /// fits; the caller must NOT then stilt to the plate through the part.
    Floored(Why),
    /// `{}`: no floor beneath the overhang; an ordinary bed overhang. The caller falls through to
    /// the plate path UNCHANGED, so the flagship parts don't change.
    NoFloor,
}

/// A floor station as the JS array `[x, y, z]` or `[x, y, z, zNeg, zPos]`, the shape
/// `sweep_between` takes.
fn floor_vec(f: &FloorPt) -> Vec<f64> {
    let mut v = vec![f.x, f.y, f.z];
    if let Some(s) = f.sides {
        v.extend(s);
    }
    v
}

/// Port of `buildPartAttached` (web/prop/attached.js:117-183): try to stand a PART-ATTACHED wall
/// under `line` (the overhang contact polyline, seated), pushing its triangles onto `out`.
///
/// This is the auto-placer's version of what draw.js does by hand: settle a BANDED top so the
/// overhang isn't dragged onto its own floor, read the floor with `floor_line`, and bridge the
/// two with `sweep_between`. It only claims a line when a real floor sits under MOST of it,
/// refuses a buried column (`buried_column`), and refuses to pierce side walls
/// (`clear_between`). A wall inside a bore is built and flagged `in_bore` (`enclosed_column`).
pub fn build_part_attached(
    line: &[[f64; 3]],
    part_tris: &Tris,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    out: &mut Vec<V4>,
    ctx: &BuildCtx,
) -> Attached {
    let cfg = ctx.cfg;
    let (gap, min_stations) = (cfg.prop.gap, cfg.prop.min_stations);
    let mut top: Vec<[f64; 3]> = line.to_vec();
    contour_top(&mut top, part_tris, PART_BAND, cfg);
    lower_sag(&mut top, part_tris, PART_BAND);
    settle_top(&mut top, part_tris, 0.25, PART_BAND, cfg);
    let floor = floor_line(&top, part_tris, 1.0, false, cfg);

    // A real floor under a majority of stations, or this is a bed overhang -- let
    // the plate path have it. floorLine returns ~0 with clear air to the plate, so
    // this declines on every ordinary overhang and the flagship parts don't change.
    let mut real = 0usize;
    for f in &floor {
        if f.z > gap + 0.5 {
            real += 1;
        }
    }
    if (real as f64) < jsnum::max(min_stations, jsnum::ceil(floor.len() as f64 * 0.5)) {
        return Attached::NoFloor;
    }

    // Why each station failed, so a refusal is counted under its real reason (the
    // one count used to be `bore` for all of them; most were short runs).
    let mut why: Vec<Why> = Vec::new();
    let ok: Vec<bool> = (0..top.len())
        .map(|k| {
            if floor[k].z <= gap + 0.5 {
                return false; // no real floor
            }
            if (top[k][2] - gap) - floor[k].z < cfg.prop.min_height {
                why.push(Why::Stub);
                return false;
            }
            if buried_column(&top, &floor, k, topo, rot, offset, gap) {
                why.push(Why::Buried);
                return false;
            }
            if clear_between(
                &top,
                &floor,
                k,
                topo,
                rot,
                offset,
                cfg.prop.th,
                cfg.prop.side_clear,
                gap,
            ) {
                return true;
            }
            why.push(Why::Blocked);
            false
        })
        .collect();
    let run = longest_run(&ok);
    let run = match run {
        Some(r) if (r[1] - r[0]) as f64 >= min_stations => r,
        _ => {
            // Mostly-failing stations name the refusal; a clear run that is merely short is a stub.
            let mut n: IndexMap<Why, usize> = IndexMap::new();
            for w in &why {
                *n.entry(*w).or_insert(0) += 1;
            }
            let mut keys: Vec<Why> = n.keys().copied().collect();
            keys.sort_by(|a, b| n[b].cmp(&n[a])); // stable: ties keep first-seen order
            let worst = keys.first().copied();
            return Attached::Floored(match worst {
                Some(w) if why.len() * 2 >= floor.len() => w,
                _ => Why::Stub,
            });
        }
    };

    let sub_top = &top[run[0]..run[1]];
    let sub_floor = &floor[run[0]..run[1]];
    let span = jsnum::hypot(&[
        sub_top[sub_top.len() - 1][0] - sub_top[0][0],
        sub_top[sub_top.len() - 1][1] - sub_top[0][1],
    ]);
    if span < cfg.prop.min_span_part {
        return Attached::Floored(Why::Stub);
    }

    let before = out.len();
    let mold = mold_line(sub_top, part_tris, cfg);
    let floors: Vec<Vec<f64>> = mold.floor.iter().map(floor_vec).collect();
    if !sweep_between(&mold.top, &floors, out, cfg.prop.min_height, ctx) {
        out.truncate(before);
        return Attached::Floored(Why::Degenerate);
    }

    // In a bore when most of the kept run is walled all round.
    let mut walled = 0usize;
    for k in run[0]..run[1] {
        if enclosed_column(&top, &floor, k, topo, rot, offset, gap) {
            walled += 1;
        }
    }
    let in_bore = walled * 2 > run[1] - run[0];

    let mut height = 0.0f64;
    let mut vol = 0.0f64;
    for i in 0..sub_top.len() {
        height = jsnum::max(height, (sub_top[i][2] - gap) - sub_floor[i].z);
    }
    let mut i = before;
    while i < out.len() {
        let (a, b, c) = (out[i], out[i + 1], out[i + 2]);
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
        i += 3;
    }
    Attached::Built(AttachedProp {
        span,
        height,
        stations: sub_top.len(),
        volume: jsnum::abs(vol),
        part_attached: true,
        in_bore,
        line: sub_top.iter().map(|p| [p[0], p[1], p[2] - gap]).collect(),
    })
}
