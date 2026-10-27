//! Port of `web/prop/sweep.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! The wall solid: `sweep` emits the upside-down T (thin wall on a flat flange)
//! along a contact line down to the plate, `sweepBetween` a part-attached wall
//! between an overhang and the floor under it. `footFor` / `profileHalf` give the
//! cross-section the clearance probes test against.
//!
//! Split out of prop.js, which re-exports the public names.

use crate::config::Config;
use crate::cutout::{Station, Wall, cut_wall};
use crate::jsnum;
use crate::prop::config::TineDir;
use crate::prop::crest::{crest_cut, crest_kinds, drop_collapsed, emit_crest};
use crate::prop::ctx::BuildCtx;
use crate::prop::resinfoot::{Foot, windowed_flange};
use crate::solids::{ribbon, rounded_flange};
use crate::types::V4;

type V3 = [f64; 3];

/// Port of `footFor` (web/prop/sweep.js:30-34): foot half-width for a wall of height `h`.
///
/// The spike used a fixed 7mm foot, which assumes the overhang sits well above
/// the plate. Real parts' overhangs are low -- median wall height 2.5mm -- so 22
/// of 36 walls degenerated into 14mm-wide splayed sheets. The foot has to scale
/// with how tall the wall actually is: `footRatio * h`, floored and capped.
///
/// The cap is the load-bearing part. A row of walls is spaced maxUnsupportedSpan
/// apart, so the foot MUST stay under half that spacing or adjacent feet overlap
/// and the whole row merges into a solid buttress (the flagship's "thick feet",
/// measured 2026-07-30: 65mm-tall walls maxed the old 7mm foot -> 14mm feet on a
/// 12mm pitch). `footMax` is set below that line, and this clamp enforces it even
/// if the span is later dialled down -- leave ~1mm of air between neighbours.
pub fn foot_for(h: f64, cfg: &Config) -> f64 {
    let p = &cfg.prop;
    let span_cap = jsnum::max(p.foot_min, (p.max_unsupported_span - 1.0) / 2.0);
    jsnum::max(
        p.foot_min,
        jsnum::min_of(&[p.foot_max, span_cap, h * p.foot_ratio]),
    )
}

/// Port of `profileHalf` (web/prop/sweep.js:44-57): half-width of the ⊥ cross-section at height
/// `z` above the bed, for a wall whose contact tips out at `top`: a flat base flange (the foot
/// of the T), a straight thin wall, then the breakaway tip taper. ONE definition, shared by
/// the geometry in sweep() and the two measurement passes -- they used to carry
/// three separate copies of a cone formula, which is exactly how a shape change
/// silently disagrees with the checker that is supposed to catch it.
pub fn profile_half(z: f64, top: f64, cfg: &Config) -> f64 {
    let p = &cfg.prop;
    let ztip = jsnum::max(top - p.tip_h, p.base_h + 0.1);
    // the body as sweep() builds it: its neck starts over a flange that shrinks on a
    // low tail (min(baseH, top / 2)), so read the width from that same rise -- never
    // narrower than the mesh, or a clearance probe under-reads the wall
    let half = stem_half(
        top - jsnum::max(top - p.tip_h, jsnum::min(p.base_h, top / 2.0) + 0.1),
        cfg,
    );
    if z < p.base_h {
        return foot_for(top, cfg); // flat flange (the T's foot)
    }
    if z < ztip {
        return half; // straight wall (the T's stem)
    }
    half // neck into the contact tip
        - (half - p.tip / 2.0) * ((z - ztip) / jsnum::max(1e-6, top - ztip))
}

/// Port of `stemHalf` (web/prop/sweep.js:63-69): half-width of a wall's straight body when its
/// neck into the tip only has `rise` mm to climb: the full PROP.th / 2, or narrower when the
/// wall is too low to neck at PROP.neckDeg -- a wide wall (Wall lines, a big nozzle) squeezed
/// into a short taper leant its shoulder flatter than that, an overhang on the wall's own side
/// (local issue 044).
pub fn stem_half(rise: f64, cfg: &Config) -> f64 {
    let p = &cfg.prop;
    let k = 1.0 / jsnum::tan(p.neck_deg * std::f64::consts::PI / 180.0);
    // Plus half a line (tip / 3: the tip is 1.5 lines): a step that narrow prints at any
    // angle, the slicer's own 50% perimeter overlap. So the 2-line wall's 0.2 mm
    // shoulder never narrows -- not even on a low tail station -- and only a wide wall does.
    let slack = p.tip / 3.0 + 1e-9;
    jsnum::max(
        p.tip / 2.0,
        jsnum::min(p.th / 2.0, p.tip / 2.0 + jsnum::max(0.0, rise) * k + slack),
    )
}

/// Port of `sweep` (web/prop/sweep.js:79-129): sweep the prop along `line`, emitting an
/// upside-down T: a straight thin wall standing on a flat base flange. They are TWO overlapping
/// closed solids, not one -- the slicer unions them, the same overlap approach the rest of the
/// repo uses -- which keeps each section convex and sidesteps capping a T's concave outline.
/// Replaces the single cone-footed solid that read as a golf tee.
///
/// JS default `minH = PROP.minHeight`: callers pass it. Returns false where JS does.
pub fn sweep(line: &[V3], z_bed: f64, out: &mut Vec<V4>, min_h: f64, ctx: &BuildCtx) -> bool {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    let (mut wall, mut flange): (Vec<Vec<V3>>, Vec<Vec<V3>>) = (Vec::new(), Vec::new());
    let mut st: Vec<Station> = Vec::new();
    let mut crest: Vec<Option<Vec<V3>>> = Vec::new();
    let mut feet: Vec<Foot> = Vec::new();
    // the interface crest's plan, null when off
    let kinds = crest_kinds(&ctx.crest, line, cfg);
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
        if h < min_h {
            return false;
        }
        let foot = foot_for(h, cfg);
        // A low TAIL station (see withLowTails) has less headroom than the flange +
        // tip taper assume, so both shrink with it. Identical to before for h >= 1.2.
        let flange_h = jsnum::min(prop.base_h, h / 2.0);
        let ztip = jsnum::max(top - prop.tip_h, z_bed + flange_h + 0.1);
        let base_top = z_bed + flange_h;
        let pt = |o: f64, z: f64| -> V3 { [p[0] + sx * o, p[1] + sy * o, z] };
        // where the interface crest runs, the stem stops at the crest's cut
        let sh = stem_half(top - ztip, cfg);
        let kind = kinds.as_ref().and_then(|k| k[i]);
        let c = crest_cut(&ctx.crest, top, ztip, z_bed, kind, 2.0 * sh, prop.tip, cfg);
        crest.push(c.map(|c| c.ring(pt)));
        let (zt, z_top, w_top) = match c {
            Some(c) => (c.zt, c.cut, c.wc),
            None => (ztip, top, prop.tip / 2.0),
        };

        // the stem: a straight thin wall from the bed up to the breakaway tip, no wider
        // than its neck can climb from (stemHalf)
        wall.push(vec![
            pt(sh, z_bed),
            pt(sh, zt),
            pt(w_top, z_top),
            pt(-w_top, z_top),
            pt(-sh, zt),
            pt(-sh, z_bed),
        ]);
        // the foot: a flat slab, its own closed solid overlapping the wall's base
        flange.push(vec![
            pt(foot, z_bed),
            pt(foot, base_top),
            pt(-foot, base_top),
            pt(-foot, z_bed),
        ]);
        feet.push(Foot {
            p: [p[0], p[1]],
            sx,
            sy,
            foot,
            sh,
            z_bed,
            base_top,
        });
        st.push(Station {
            p: [p[0], p[1]],
            sx,
            sy,
            top: z_top,
            ztip: zt,
            hole_top: Some(ztip),
            tip_half: Some(w_top),
            sh: Some(sh),
            bot: z_bed,
            bot_tip: base_top,
            taper_bot: false,
            bot_n: None,
            bot_p: None,
        });
    }

    let from = out.len();
    if !cut_wall(&st, &wall, out, &Wall::from(prop), cfg) {
        ribbon(&wall, out);
    }
    // resin pins (#309): a windowed flange, so the base flexes off a resin plate
    if prop.round_feet && prop.tine_dir != TineDir::Vertical {
        rounded_flange(&flange, out);
    } else if prop.tine_dir != TineDir::Vertical || !windowed_flange(&feet, out, cfg) {
        ribbon(&flange, out);
    }
    if kinds.is_some() {
        drop_collapsed(out, from);
        emit_crest(&crest, out);
    }
    true
}

/// Port of `sweepBetween` (web/prop/sweep.js:157-226): sweep a PART-ATTACHED wall between two
/// contours: its top stops `gap` below the overhang (`top_line`, exactly as `sweep` does) and
/// its bottom rests ON the floor contour (`bot_line` from `floorLine`) instead of a flat `zBed`.
/// A `bot_line` entry is `[x, y, z]`, or `[x, y, z, bN, bP]` with the floor under each side
/// (floorLine's sideFloors): `JS botLine[i].length < 5` reads the length.
///
/// The bottom is a per-station contour, so it conforms to a sloped or curved floor
/// for free -- no flat foot ellipse, which would only touch a level surface at one
/// edge. The wall tapers to the `tip` width at BOTH ends: the top tip breaks away
/// under the overhang (the part bridges the `gap`), and the bottom tip is the only
/// thing that welds to the part below, kept as narrow as the top so it leaves the
/// smallest possible witness mark and snaps off cleanly.
///
/// The bottom stops PROP.footGap above the floor -- the same clearance as the top,
/// a slicer's "bottom Z distance". It used to weld on purpose, on the worry that
/// a gap at both ends makes a floating island the slicer can't anchor; the foot
/// coupon (prototype/calibration/foot/) printed the gap clean, and the welded tip
/// was the only one that scarred the part (local issue 009). The first layer
/// sags across the gap, so the bottom keeps the full wall thickness -- two lines
/// over air, not one 0.6 mm tip line that could peel. footGap 0 welds as before.
/// Over open plate (a floor of 0) there is no part to scar, so no gap: the bottom
/// stands on the plate.
///
/// `min_h` is the least headroom a station needs (Draw mode passes minHeightSquat
/// for a squat wall the user asked for by hand; Auto keeps minHeight). JS default
/// `PROP.minHeight`: callers pass it.
pub fn sweep_between(
    top_line: &[V3],
    bot_line: &[Vec<f64>],
    out: &mut Vec<V4>,
    min_h: f64,
    ctx: &BuildCtx,
) -> bool {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    let welded = prop.foot_gap <= 0.0;
    // only the PART gets the gap: a station whose floor is open plate all across
    // (floorLine's 0, the highest surface under the wall's width) stands on it --
    // lifted, a wall over the plate floated 0.2 mm in the air with nothing for the
    // slicer to start it on (a figure's hand over the plate). Decided per STATION,
    // both sides together: sideFloors' tilt assumes one gap added to both, and a
    // plate side left down under a chamfer's toe ran 0.07 mm off the part
    let mut wall: Vec<Vec<V3>> = Vec::new();
    let mut st: Vec<Station> = Vec::new();
    let mut crest: Vec<Option<Vec<V3>>> = Vec::new();
    // the interface crest's plan, null when off
    let kinds = crest_kinds(&ctx.crest, top_line, cfg);
    for i in 0..top_line.len() {
        let p = top_line[i];
        let a = top_line[jsnum::max(0.0, i as f64 - 1.0) as usize];
        let b = top_line[jsnum::min(top_line.len() as f64 - 1.0, i as f64 + 1.0) as usize];
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
        let g = if bot_line[i][2] > 0.0 {
            prop.foot_gap
        } else {
            0.0
        };
        let bot = bot_line[i][2] + g;
        // judged on the headroom, not the lifted wall: the gap must not change
        // WHICH walls exist (hub_corner X60 lost a 31 mm wall to a 1.6 mm station)
        if top - bot_line[i][2] < min_h {
            return false;
        }
        // a lifted bottom tilts with the floor under each side (floorLine's
        // sideFloors); welded, or with no side floors, it is level at `bot`
        // (a molded side can sit above `bot`: never within 0.5 of the top, which the
        // headroom check above keeps >= minH - footGap over the plain floor + gap --
        // 1.3 at minHeight, 0.4 for Draw's squat minH)
        let side = |k: usize| {
            if welded || bot_line[i].len() < 5 {
                bot
            } else {
                jsnum::min(bot_line[i][k] + g, top - 0.5)
            }
        };
        let b_n = side(3);
        let b_p = side(4);
        let b_hi = jsnum::max(b_n, b_p);
        let h = top - b_hi;
        // tapers meet in the middle if short; lifted, the top taper stops 0.1 above
        // the bottom's high side -- ending ON it collapsed a section edge into
        // zero-area slivers (ssdMounts X30Y60: short part walls, 4 per wall)
        let taper = jsnum::min(prop.tip_h, if welded { h / 2.0 } else { h - 0.1 });
        let z_bot_tip = if welded { bot + taper } else { b_hi };
        let pt = |o: f64, z: f64| -> V3 { [p[0] + sx * o, p[1] + sy * o, z] };
        // where the interface crest runs, the wall stops at the crest's cut
        // no wider than the top neck can climb from (stemHalf; the welded bottom taper is
        // the same height, so it holds there too)
        let sh = stem_half(taper, cfg);
        let kind = kinds.as_ref().and_then(|k| k[i]);
        let c = crest_cut(
            &ctx.crest,
            top,
            top - taper,
            z_bot_tip,
            kind,
            2.0 * sh,
            prop.tip,
            cfg,
        );
        crest.push(c.map(|c| c.ring(pt)));
        let (z_top_tip, z_top, w_top) = match c {
            Some(c) => (c.zt, c.cut, c.wc),
            None => (top - taper, top, prop.tip / 2.0),
        };

        // one closed section: th-wide through the middle, tip-wide at the top and,
        // when welded, at the bottom too
        wall.push(if welded {
            vec![
                pt(prop.tip / 2.0, bot),
                pt(sh, z_bot_tip),
                pt(sh, z_top_tip),
                pt(w_top, z_top),
                pt(-w_top, z_top),
                pt(-sh, z_top_tip),
                pt(-sh, z_bot_tip),
                pt(-prop.tip / 2.0, bot),
            ]
        } else {
            vec![
                pt(sh, b_p),
                pt(sh, z_top_tip),
                pt(w_top, z_top),
                pt(-w_top, z_top),
                pt(-sh, z_top_tip),
                pt(-sh, b_n),
            ]
        });
        st.push(Station {
            p: [p[0], p[1]],
            sx,
            sy,
            top: z_top,
            ztip: z_top_tip,
            hole_top: Some(top - taper),
            tip_half: Some(w_top),
            sh: Some(sh),
            bot: b_hi,
            bot_tip: z_bot_tip,
            taper_bot: welded,
            bot_n: Some(b_n),
            bot_p: Some(b_p),
        });
    }

    let from = out.len();
    if !cut_wall(&st, &wall, out, &Wall::from(prop), cfg) {
        ribbon(&wall, out);
    }
    if kinds.is_some() {
        drop_collapsed(out, from);
        emit_crest(&crest, out);
    }
    true
}
