//! Port of `web/prop/squat.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Squat walls: the near-bed band of an overhang, too low for the full T-wall,
//! gets a thin wall on a wide two-layer brim (`sweepSquat`), built over the low
//! stations the tall run leaves behind (`buildSquatBed`).
//!
//! Split out of prop.js, which re-exports the public names.

use crate::inside::{Nearest, solid_clearance};
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::clearance::{longest_run, station_certified, station_is_clear, welds};
use crate::prop::config::TineDir;
use crate::prop::contact::settle_top;
use crate::prop::crest::{crest_cut, crest_kinds, drop_collapsed, emit_crest};
use crate::prop::ctx::BuildCtx;
use crate::prop::resinfoot::{Foot, windowed_flange};
use crate::prop::sweep::stem_half;
use crate::solids::{ribbon, rounded_flange};
use crate::tris::Tris;
use crate::types::{Rot3, V4, Xyz};

type V3 = [f64; 3];

/// Port of `sweepSquat` (web/prop/squat.js:34-83): sweep a SQUAT breakaway wall: a thin wall
/// necking to the breakaway tip, on a flat brim. The full T-flange (`sweep`) can't fit here --
/// baseH 1.0 alone is most of the wall -- but a squat wall still has to hold to the plate, so it
/// gets a thin WIDE brim instead: two layers tall (snaps off, leaves no part mark since it sits
/// on the plate), wide enough to grip. Two overlapping solids the slicer unions, exactly like
/// `sweep`'s wall + flange.
///
/// `inset` (optional, mm per station; JS default `null` is `None`) narrows the stem, its tip and
/// the brim on each side there: a stretch that runs inside a full wall's stem (draw.js's mixed
/// run), or past another squat run's, would otherwise have the same section and meet it face on
/// face, flush instead of overlapping. A station past the end of `inset` counts as 0 (JS
/// `inset?.[i] ?? 0`).
pub fn sweep_squat(
    line: &[V3],
    z_bed: f64,
    out: &mut Vec<V4>,
    inset: Option<&[f64]>,
    ctx: &BuildCtx,
) -> bool {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    let mut wall: Vec<Vec<V3>> = Vec::new();
    let mut brim: Vec<Vec<V3>> = Vec::new();
    let mut crest: Vec<Option<Vec<V3>>> = Vec::new();
    let mut feet: Vec<Foot> = Vec::new();
    // the interface crest's plan, null when off
    let kinds = crest_kinds(&ctx.crest, line, cfg);
    let brim_top = z_bed + prop.squat_brim_h;
    for i in 0..line.len() {
        let p = line[i];
        let a = line[jsnum::max(0.0, i as f64 - 1.0) as usize];
        let b = line[jsnum::min(line.len() as f64 - 1.0, i as f64 + 1.0) as usize];
        let mut rx = b[0] - a[0];
        let mut ry = b[1] - a[1];
        let rn = jsnum::hypot(&[rx, ry]);
        if rn < 1e-9 {
            return false;
        }
        rx /= rn;
        ry /= rn;
        let sx = ry; // horizontal, across the wall
        let sy = -rx;

        let top = p[2] - prop.gap;
        let h = top - z_bed;
        if h < prop.min_height_squat {
            return false;
        }
        // neck to the tip over whatever height is left above the brim
        let ztip = jsnum::max(top - prop.tip_h, brim_top + 0.05);
        let pt = |o: f64, z: f64| -> V3 { [p[0] + sx * o, p[1] + sy * o, z] };
        // where the interface crest runs, the stem stops at the crest's cut
        let inn = inset.and_then(|v| v.get(i)).copied().unwrap_or(0.0);
        let sh = stem_half(top - ztip, cfg) - inn; // no wider than its neck can climb from (044)
        let kind = kinds.as_ref().and_then(|k| k[i]);
        let c = crest_cut(
            &ctx.crest,
            top,
            ztip,
            brim_top,
            kind,
            2.0 * sh,
            prop.tip - 2.0 * inn,
            cfg,
        );
        crest.push(c.map(|c| c.ring(pt)));
        let (zt, z_top, w_top) = match c {
            Some(c) => (c.zt, c.cut, c.wc),
            None => (ztip, top, prop.tip / 2.0 - inn),
        };

        // the stem: bed to breakaway tip, th-wide then necking to the contact tip
        wall.push(vec![
            pt(sh, z_bed),
            pt(sh, zt),
            pt(w_top, z_top),
            pt(-w_top, z_top),
            pt(-sh, zt),
            pt(-sh, z_bed),
        ]);
        // the brim: a thin flat slab overlapping the wall's base, for plate grip
        let bw = prop.squat_brim_w - inn;
        brim.push(vec![
            pt(bw, z_bed),
            pt(bw, brim_top),
            pt(-bw, brim_top),
            pt(-bw, z_bed),
        ]);
        feet.push(Foot {
            p: [p[0], p[1]],
            sx,
            sy,
            foot: bw,
            sh,
            z_bed,
            base_top: brim_top,
        });
    }
    let from = out.len();
    ribbon(&wall, out);
    // resin pins (#309): a windowed brim, so it flexes off a resin plate (not on an
    // inset run, whose brim is narrowed to stay off its neighbour's; Draw passes an
    // all-zero inset on an all-squat line)
    if prop.round_feet && prop.tine_dir != TineDir::Vertical {
        rounded_flange(&brim, out);
    } else if prop.tine_dir != TineDir::Vertical
        || inset.is_some_and(|v| v.iter().any(|&x| x > 0.0))
        || !windowed_flange(&feet, out, cfg)
    {
        ribbon(&brim, out);
    }
    if kinds.is_some() {
        drop_collapsed(out, from);
        emit_crest(&crest, out);
    }
    true
}

/// A squat wall `buildSquatBed` placed (web/prop/squat.js:175-190): `{ span, height, stations,
/// volume, squat: true, line, triRange }`.
#[derive(Clone, Debug, PartialEq)]
pub struct SquatWall {
    pub span: f64,
    pub height: f64,
    pub stations: usize,
    pub volume: f64,
    /// `squat: true`
    pub squat: bool,
    /// the wall's centreline (surface minus gap)
    pub line: Vec<V3>,
    /// Triangle range of THIS wall in the caller's `out`. The caller emits the tines separately
    /// (after every squat wall), so a fin's full geometry is [this wall range] + [its tines
    /// range] -- two non-contiguous segments, tracked as `triRanges` at the push site.
    pub tri_range: [usize; 2],
}

/// Port of `buildSquatBed` (web/prop/squat.js:107-196): build brimmed squat breakaway walls on
/// the sub-minHeight bed stations of a contoured overhang line -- the near-bed overhangs a full
/// T-wall can't reach.
///
/// A flanged wall needs ~minHeight of headroom to exist at all, so `sweep` and its
/// trim discard every station lower than that; on an organic part whose underside
/// ramps down to the plate, that abandons the whole low band and it prints into
/// air. Here the low band is built directly: the stations with minHeightSquat <=
/// height < minHeight (DISJOINT from the tall run the caller builds, so the two
/// never compete) are walked into maximal runs, and each is swept via `sweepSquat`
/// -- a thin wall on a thin WIDE brim. The full T-foot can't fit under a 1mm wall
/// (footMin 1.6 would splay into a sheet), but the wall still has to HOLD to the
/// plate, so the brim gives it the adhesion area a bare 0.6mm-wide tip never could.
/// Same weld guard as the plate path: a squat wall that would fuse is dropped,
/// never shipped ("no prop" is fixable, a fused prop is a ruined print).
///
/// Operates on a private deep copy of the line so `settleTop` never mutates the
/// points the caller's tall path still reads. Appends triangles to `out` and
/// returns the placed prop descriptors (marked `squat: true`).
///
/// `claimed` (optional, per station; JS default `null`) marks stations a tall wall already
/// covers with its low TAIL (withLowTails); those are skipped so the two never stack. Only
/// stations the built wall really spans are claimed -- a low band next to a BLOCKED tall
/// station still gets its squat wall.
#[allow(clippy::too_many_arguments)]
pub fn build_squat_bed(
    line: &[V3],
    region_tris: &Tris,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    out: &mut Vec<V4>,
    claimed: Option<&[bool]>,
    ctx: &BuildCtx,
) -> Vec<SquatWall> {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    let z_bed = 0.0;
    let mut placed: Vec<SquatWall> = Vec::new();
    let height_of = |p: &V3| (p[2] - prop.gap) - z_bed;
    let usable: Vec<bool> = line
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let h = height_of(p);
            h >= prop.min_height_squat
                && h < prop.min_height
                && !claimed.is_some_and(|c| c.get(k).copied().unwrap_or(false))
                && station_is_clear(line, k, topo, rot, offset, cfg)
        })
        .collect();

    let mut k = 0;
    while k < usable.len() {
        if !usable[k] {
            k += 1;
            continue;
        }
        let mut j = k;
        while j < usable.len() && usable[j] {
            j += 1;
        }
        let mut raw: Vec<V3> = line[k..j].to_vec(); // deep copy
        k = j;
        if (raw.len() as f64) < prop.min_stations {
            continue;
        }
        let span_raw = jsnum::hypot(&[
            raw[raw.len() - 1][0] - raw[0][0],
            raw[raw.len() - 1][1] - raw[0][1],
        ]);
        if span_raw < prop.min_span_squat {
            continue;
        }

        // Put the closest approach on spec, then re-trim: settling can lift a station
        // into the tall band or drop one below the squat floor, exactly as it can for
        // a full wall. Keep only what is still squat-height and measurably clear.
        // (`settleTop`'s defaults: step 0.25, band Infinity.)
        settle_top(&mut raw, region_tris, 0.25, f64::INFINITY, cfg);
        let avail: Vec<bool> = raw
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let h = height_of(p);
                h >= prop.min_height_squat
                    && h < prop.min_height
                    && station_certified(&raw, i, topo, rot, offset, cfg)
            })
            .collect();
        let Some(run) = longest_run(&avail) else {
            continue;
        };
        if ((run[1] - run[0]) as f64) < prop.min_stations {
            continue;
        }
        let settled: Vec<V3> = raw[run[0]..run[1]].to_vec();
        let span = jsnum::hypot(&[
            settled[settled.len() - 1][0] - settled[0][0],
            settled[settled.len() - 1][1] - settled[0][1],
        ]);
        if span < prop.min_span_squat {
            continue;
        }

        let before = out.len();
        if !sweep_squat(&settled, z_bed, out, None, ctx) {
            out.truncate(before);
            continue;
        }

        // Same acceptance as the plate path: an approach from above is the breakaway
        // interface (must clear the gap), anything else is a flank weld.
        let verts: Vec<V3> = out[before..].iter().map(|v| [v[0], v[1], v[2]]).collect();
        let hit = solid_clearance(topo, rot, offset, &verts, 0.25);
        if let Some(h) = hit
            && welds(
                &Nearest {
                    d: h.d,
                    cos_up: h.cos_up,
                },
                cfg,
            )
        {
            out.truncate(before);
            continue;
        }

        let top = jsnum::max_of(&settled.iter().map(|p| p[2]).collect::<Vec<_>>()) - prop.gap;
        let mut vol = 0.0;
        let mut i = before;
        while i < out.len() {
            let (a, b, c) = (out[i], out[i + 1], out[i + 2]);
            vol += (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
            i += 3;
        }
        placed.push(SquatWall {
            span,
            height: top - z_bed,
            stations: settled.len(),
            volume: jsnum::abs(vol),
            squat: true,
            line: settled
                .iter()
                .map(|p| [p[0], p[1], p[2] - prop.gap])
                .collect(),
            tri_range: [before, out.len()],
        });
    }
    placed
}
