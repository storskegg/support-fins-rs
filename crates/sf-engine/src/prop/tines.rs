//! Port of `web/prop/tines.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Grip tines: the one-layer nubs a wall reaches into the part with, so a tilted
//! part can't peel off it. `emitTines` lays the comb along a wall, `biteDirsAt`
//! aims each nub at the nearest part face, `tineStepFor` maps the "Tine grip"
//! setting to nub spacing.
//!
//! Split out of prop.js, which re-exports the public names.

use crate::config::Config;
use crate::inside::inside_part;
use crate::jsnum;
use crate::kiss::{KissOpts, kiss_ends};
use crate::overhangs::Topology;
use crate::prop::config::{Iface, TineDir};
use crate::prop::crest::{crest_on, mark_interface};
use crate::prop::ctx::{BuildCtx, TineSeed};
use crate::prop::nearface::nearest_faces;
use crate::prop::pins::emit_pins;
use crate::solids::{box_extrude, loft_extrude};
use crate::tris::Tris;
use crate::types::{Rot3, V4, Xyz};

/// A horizontal unit heading: JS `{ x, y }`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Dir {
    x: f64,
    y: f64,
}

/// The positional rest arguments of `emitTines(line, tris, topo, rot, offset, out, ...rest)`
/// (and of `emitPins`, which takes the same): `(stepArg, minTop, tineH, body, grip)`. Read from
/// every call site (prop.js:509, :546, :672, draw.js:212, fins/wedges.js:397, the tests):
///
/// | position | JS | field | callers pass |
/// |---|---|---|---|
/// | 7th | `stepArg = PROP.tineStep` | `step` | `tineStepEff` / `tineStepFor(density)`, or nothing |
/// | 8th | `minTop = PROP.baseH + 0.2` | `min_top` | `undefined`, `PROP.squatBrimH`, a squat wall's `minTop` |
/// | 9th | `tineH = PROP.tineH` | `tine_h` | the layer height (`tineHeight`, `opts.layerHeight ?? PROP.tineH`), or nothing |
/// | 10th | `body = null` | `body` | `tallBody(...)`: `[i0, i1]`, the station range of a tall wall's full-height body |
/// | 11th | `grip = null` | `grip` | `{ z: Infinity }`, whose `z` the call lowers to the lowest tine bottom |
///
/// `None` is JS's `undefined` (the default applies). `Default` is all-`None`, i.e. no rest
/// arguments at all. **`grip` out-parameter:** the mutated `{ z }` object becomes
/// `Option<&mut f64>` (the `z`): the call only ever lowers it.
#[derive(Debug, Default)]
pub struct TineOpts<'a> {
    pub step: Option<f64>,
    pub min_top: Option<f64>,
    pub tine_h: Option<f64>,
    pub body: Option<[usize; 2]>,
    pub grip: Option<&'a mut f64>,
}

/// Port of `biteDirsAt` (web/prop/tines.js:25-46): horizontal INWARD-normal of the part face
/// nearest a tine seed -- the direction a tine must bite to grip "straight on". The bite heading
/// has to come from the PART (which way the face points), never from the wall's run: a wall
/// along a leaning face's level contour runs TANGENT to the surface, so a run-aligned nub lies
/// flat instead of biting in. The nearest face handles both scenarios the tool places walls in
/// -- under a sloped overhang (nearest face is the underside) and beside a near-vertical wall
/// (nearest face is that side) -- while a shallow ceiling (near-vertical normal, tiny
/// horizontal component) returns nothing, the honest "too flat to grip horizontally" case.
/// emitTines then squares the heading to the wall's own axes (squareToRun).
fn bite_dirs_at(topo: &Topology, rot: &Rot3, offset: &Xyz, px: f64, py: f64, pz: f64) -> Vec<Dir> {
    let nrm = &topo.nrm;
    // TIES. At an inside corner two faces can be EXACTLY equidistant (a step's
    // underside and the part's side face, 6.7100e-3 both), and which one a strict
    // `<` kept was decided by 1e-15 of float noise -- so an unrelated change that
    // nudged a station by that much flipped tines between biting and missing.
    // nearestFaces returns every tied face (nearest first by index, as before) and
    // the caller takes the first whose bite lands in the part. It used to measure
    // every face here, per tine: on a big part that was most of the build.
    let mut dirs = Vec::new();
    for f in nearest_faces(topo, rot, offset, px, py, pz) {
        let (nx, ny, nz) = (nrm[f * 3], nrm[f * 3 + 1], nrm[f * 3 + 2]);
        // seated normal, then INWARD (into the part) = negated, horizontal component only
        let sx = rot[0] * nx + rot[3] * ny + rot[6] * nz;
        let sy = rot[1] * nx + rot[4] * ny + rot[7] * nz;
        let (hx, hy) = (-sx, -sy);
        let hm = jsnum::hypot(&[hx, hy]);
        if hm < 0.34 {
            continue; // face too flat (near-horizontal ceiling) to grip sideways
        }
        dirs.push(Dir {
            x: hx / hm,
            y: hy / hm,
        });
    }
    dirs
}

/// Port of `squareToRun` (web/prop/tines.js:62-88): square the part's bite headings to the wall:
/// each becomes the nearer of the wall's four axes (along the run either way, or straight across
/// either way), nearest first, then the next-nearer axis as a fallback. Only axes within 90
/// degrees of the part's heading are offered, so a tine still bites where the part is. The
/// part's own heading comes last: where no square tine lands (the lowest grip on a tilted coin
/// or torus), an angled one still beats losing the base grip -- 11 sweep cases' lowest tine rose
/// up to 2.7 mm without it.
///
/// The heading itself is the underside's down-slope (biteDirsAt), which is only
/// square to a wall that runs down the slope or along its contour -- what the
/// patch path lays. A wall that crosses a curved underside at an angle (a raster
/// track under a torus, or a straight track under a sphere) got its tines out of
/// the wall at 20-70 degrees, poking diagonally off its side (torus X30: 12 of 22
/// tines). A square tine is either inside the wall's own footprint (along) or a
/// clean bridge off its face (across), both of which print as one bead.
fn square_to_run(dirs: Vec<Dir>, line: &[[f64; 3]], k: usize) -> Vec<Dir> {
    // the run at station k: its own segment, or the nearest non-degenerate one
    // (a duplicated station has zero length)
    let (mut rx, mut ry) = (0.0f64, 0.0f64);
    let nseg = line.len() as isize - 1;
    let mut j: isize = 0;
    // `!(rx || ry)`: both 0 or NaN
    let truthy = |v: f64| v != 0.0 && !v.is_nan();
    while j < nseg && !(truthy(rx) || truthy(ry)) {
        for i in [k as isize + j, k as isize - j] {
            if i < 0 || i >= nseg {
                continue;
            }
            let i = i as usize;
            let dx = line[i + 1][0] - line[i][0];
            let dy = line[i + 1][1] - line[i][1];
            if jsnum::hypot(&[dx, dy]) > 1e-9 {
                rx = dx;
                ry = dy;
                break;
            }
        }
        j += 1;
    }
    let rn = jsnum::hypot(&[rx, ry]);
    if rn < 1e-9 {
        return dirs;
    }
    rx /= rn;
    ry /= rn;
    let axes = [[rx, ry], [-rx, -ry], [-ry, rx], [ry, -rx]];
    let mut out: Vec<Dir> = Vec::new();
    let mut seen = [false; 4];
    for c in &dirs {
        let mut ranked: Vec<(usize, Dir, f64)> = axes
            .iter()
            .enumerate()
            .map(|(i, &[ux, uy])| (i, Dir { x: ux, y: uy }, ux * c.x + uy * c.y))
            .filter(|u| u.2 > 1e-6)
            .collect();
        // `.sort((p, q) => q.dot - p.dot)`: stable; a NaN difference counts as equal
        ranked.sort_by(|p, q| {
            let d = q.2 - p.2;
            if d < 0.0 {
                std::cmp::Ordering::Less
            } else if d > 0.0 {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        for u in ranked {
            if seen[u.0] {
                continue; // tied faces often rank the same axis
            }
            seen[u.0] = true;
            out.push(u.1);
        }
    }
    out.extend(dirs);
    out
}

/// Port of `tineStepFor` (web/prop/tines.js:97-100): nub spacing (mm) for a user "Tine grip"
/// setting in [0 sparse .. 1 dense]. DEFAULT (`None`, JS `undefined`/`null`) is dense -- the
/// proven comb. Sparse only ever LOOSENS the requested spacing; emitTines's minGripTines floor
/// still guarantees grip on short walls, so a sparse setting thins surface marking without
/// starving grip.
pub fn tine_step_for(density: Option<f64>, cfg: &Config) -> f64 {
    let p = &cfg.prop;
    let d = jsnum::max(0.0, jsnum::min(1.0, density.unwrap_or(1.0)));
    p.tine_step_sparse - d * (p.tine_step_sparse - p.tine_step)
}

/// `STEP_INSET` (web/prop/tines.js:120): how far a wall step (below) stays inside its tine's
/// footprint: enough that no vertex coincides, far under anything a slicer resolves.
const STEP_INSET: f64 = 0.01;

/// Port of `emitTines` (web/prop/tines.js:122-135): lay a comb of grip tines along a placed
/// wall's top, biting a hair into the part, and return how many actually landed.
///
/// `line` is the wall's settled TOP contour (each station's z is the part surface
/// directly above it; the wall's own top sits `gap` under that). A tine is one
/// layer-tall nub that reaches horizontally off the wall top into the part. The
/// direction is chosen, not assumed: the part material adjacent to the top lies
/// DOWN-slope (where the underside is lower, the wall-top height is already inside
/// the solid), so the emitter tries both run directions and keeps whichever puts
/// the nub's tip inside the part -- and emits nothing where neither does, which is
/// the honest "this face is too shallow to grip" case a horizontal tine has by
/// nature (the old fins.js tineSpanMax rule, expressed as a containment test here).
///
/// Nubs are the wall's own thickness wide and overlap back into it, so the slicer
/// unions them onto the wall the same way every other solid here is unioned.
///
/// `opts.grip`, if given, gets the lowest tine bottom: the wall's grip height. Not the lowest
/// emitted vertex -- a wall step under a tine sits lower. `tris` (the seated part soup) is
/// accepted and ignored, as in JS (callers pass `null`).
#[allow(clippy::too_many_arguments)]
pub fn emit_tines(
    line: &[[f64; 3]],
    _tris: Option<&Tris>,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    out: &mut Vec<V4>,
    opts: TineOpts,
    ctx: &BuildCtx,
) -> usize {
    let prop = &ctx.cfg.prop;
    // with the interface crest on everywhere, the whole comb (tines and wall steps)
    // prints in the interface material: it is what reaches into the part. Not under
    // 'flat': a tine grips a sideways face, where the interface would climb through
    // every layer (prop/crest.js).
    let from = out.len();
    // 'vertical' (resin, #309): pins up into the part instead of the horizontal comb
    let n = if prop.tine_dir == TineDir::Vertical {
        emit_pins(line, topo, rot, offset, out, opts, ctx)
    } else {
        comb(line, topo, rot, offset, out, opts, ctx)
    };
    if crest_on(ctx.cfg) && prop.iface == Iface::All {
        mark_interface(out, from);
    }
    n
}

fn comb(
    line: &[[f64; 3]],
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    out: &mut Vec<V4>,
    opts: TineOpts,
    ctx: &BuildCtx,
) -> usize {
    let prop = &ctx.cfg.prop;
    let TineOpts {
        step: step_arg,
        min_top,
        tine_h,
        body,
        mut grip,
    } = opts;
    let step_arg = step_arg.unwrap_or(prop.tine_step);
    let min_top = min_top.unwrap_or(prop.base_h + 0.2);
    let tine_h = tine_h.unwrap_or(prop.tine_h);
    if line.len() < 2 {
        return 0;
    }

    // arc length along the run, to space nubs by a real distance not a station count
    let mut s = vec![0.0];
    for i in 1..line.len() {
        s.push(
            s[i - 1] + jsnum::hypot(&[line[i][0] - line[i - 1][0], line[i][1] - line[i - 1][1]]),
        );
    }
    // `body` = [i0, i1], the station range of a tall wall's full-height BODY (the
    // rest is its low TAILS, withLowTails). It only sizes the spacing below: the
    // minGripTines floor counts the body, as before walls grew tails, so a tail
    // never thins the comb. The comb itself runs the whole wall, anchored at the
    // lowest point a nub grips (see the end), so rows never land where a tail
    // can't take one. Callers without a tail pass nothing.
    let s0 = body.map_or(0.0, |b| s[b[0]]);
    let s1 = body.map_or(s[s.len() - 1], |b| s[b[1]]);
    let total = s1 - s0;
    // `!(total > 0)`: also true for NaN
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let none = !(total > 0.0);
    if none {
        return 0;
    }

    // The caller's step encodes tip-over risk (sparse for a stable part). But grip
    // is a floor no part goes under: a wall gets at least minGripTines along its
    // length, so a squat part's sparse spacing never starves a long wall of grip or
    // leaves a short wall with a single lonely nub. min() only ever TIGHTENS the
    // requested spacing, never loosens it past the dense comb.
    // PROP.tinesPerWall (calibration) asks for exactly n: spaced over the WHOLE run
    // (tails too, which the comb also covers), so n fit.
    let step = if prop.tines_per_wall > 0.0 {
        s[s.len() - 1] / prop.tines_per_wall
    } else {
        jsnum::min(step_arg, total / prop.min_grip_tines)
    };
    // `!(PROP.tinesPerWall > 0)`: also true for NaN
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let no_count = !(prop.tines_per_wall > 0.0);
    if total < step && no_count {
        return 0;
    }

    // How far out a tine looks for the part. The probe sits at the tine's mid-height,
    // half a layer under the underside, so under a shallow underside a thicker layer
    // needs a longer reach to find solid: at a fixed 0.5 a 0.6 mm layer lost every tine
    // under an underside within ~31deg of level (0.2 mm: ~11deg). The reach keeps the
    // tuned 0.2 layer's reach-to-layer ratio; 0.2 and thinner are unchanged. Capped at
    // 0.6's: a host's own layer height (a slicer plugin) can be thicker than the site's
    // field allows, and kissEnds looks out to twice the reach.
    let reach = prop.tine_reach * jsnum::min(3.0, jsnum::max(1.0, tine_h / prop.tine_h))
        + prop.tine_reach_tip;

    // half the tine's WIDTH across the run -- one nozzle bead (PROP.tineW), NOT the
    // wall thickness. Building it th-wide made a 1mm divot, ~2x Slant3D's spec.
    let half = prop.tine_w / 2.0;

    let mut count = 0;
    // Place ONE tine at arc length d0 (relative to the body start s0); true if it
    // landed. Everything below is per-station and unchanged.
    let mut place = |d0: f64| -> bool {
        let d = d0 + s0; // back onto the full run's arc length
        // interpolate the station at arc length d
        let mut k = 0;
        while k + 1 < s.len() && s[k + 1] < d {
            k += 1;
        }
        let seg = jsnum::max(1e-9, s[k + 1] - s[k]);
        let f = (d - s[k]) / seg;
        let x = line[k][0] + (line[k + 1][0] - line[k][0]) * f;
        let y = line[k][1] + (line[k + 1][1] - line[k][1]) * f;
        let z = line[k][2] + (line[k + 1][2] - line[k][2]) * f; // surface z
        let wall_top = z - prop.gap;
        if wall_top < min_top {
            // below the wall's base (flange or brim): no
            // face to attach a tine to. minTop defaults to
            // the flanged base; a squat wall passes its brim.
            return false;
        }

        // LAYER-SNAP so the tine prints as exactly ONE bead, not two partial layers.
        // A tine is tineH tall (= the slicer's layer height) precisely so it slices as a
        // single continuous bead that snaps clean. But its top used to be pinned to the
        // part underside `z`, which is almost never on the layer grid -- so a 0.2mm tine
        // straddled a layer boundary and sliced into two thin layers (Matthew's cube: all
        // 22 tines spanned 2 layers, each a 0.15mm + 0.05mm pair). A two-layer tine is a
        // taller, stronger weld that marks worse and won't bend-snap clean. Fix: snap the
        // tine's span onto the layer grid so it fills exactly one cell [tineBot, tineTop].
        //
        // Snap to the NEAREST grid line, not the one below. Flooring (always down) drops a
        // tine whose underside sits just under a layer line by nearly a full layer -- then
        // the part's own sub-layer sliver above it is too thin to print and the slicer
        // leaves a full empty layer between the tine top and the part's first real layer:
        // a "missing layer" with the part edge floating over it (Matthew's cube: every
        // underside sat ~0.19 above a line, so every tine dropped ~0.19). Rounding keeps
        // the tine top within half a layer of the underside, so it lands right where the
        // part's nearest layer begins -- supporting it -- and its bottom stays in the same
        // or the adjacent grid cell as the wall's top layer, so it still rests on the wall.
        // Grid is plate-origin (z = 0) at the layer height: exact when the slicer's first-
        // layer height equals its layer height (the common default); a different first
        // layer just offsets every tine by the same sub-layer amount. The probe below
        // still uses the underside level (zMid), so PLACEMENT is unchanged -- only the
        // built box moves onto the grid.
        let tine_top = jsnum::round(z / tine_h) * tine_h;
        let tine_bot = tine_top - tine_h;
        let z_mid = z - tine_h / 2.0;

        // BITE DIRECTION comes from the PART (which way the nearest face points),
        // never from the wall's run alone: a run-aligned nub lies flat on a leaning
        // face whose level contour the wall follows -- the regression. The part's
        // heading is then squared to the wall's nearest axis (squareToRun). Then require the
        // nub's full reach to actually land inside the part, or skip it (honest -- no
        // tine gripping air, no tine on a ceiling too shallow to grab sideways).
        let bd = square_to_run(bite_dirs_at(topo, rot, offset, x, y, z_mid), line, k)
            .into_iter()
            .find(|c| inside_part(topo, rot, offset, x + c.x * reach, y + c.y * reach, z_mid));
        let Some(bd) = bd else {
            return false;
        };
        let (dirx, diry) = (bd.x, bd.y);

        // frame (along = bite dir, across = z x along, up = z) is right-handed
        let (ax, ay) = (-diry, dirx); // across = z x along
        let base = [x, y, 0.0];
        let p = |a: f64, b: f64, c: f64| -> [f64; 3] {
            [base[0] + dirx * a + ax * b, base[1] + diry * a + ay * b, c]
        };
        // quad in (along, across): from -overlap (into the wall) to where the tine meets
        // the part, its end leaning with the surface between bottom and top (kissEnds).
        let e = kiss_ends(
            topo,
            rot,
            offset,
            x,
            y,
            dirx,
            diry,
            tine_bot,
            tine_top,
            &KissOpts {
                half,
                back: prop.tine_overlap,
                reach,
                contact_offset: prop.tine_contact_offset,
            },
        );
        let ov = -prop.tine_overlap;
        let outline = |h: [f64; 2]| vec![[ov, -half], [h[0], -half], [h[1], half], [ov, half]];
        loft_extrude(&outline(e.bot), &outline(e.top), tine_bot, tine_top, p, out);
        if let Some(g) = grip.as_deref_mut()
            && tine_bot < *g
        {
            *g = tine_bot;
        }
        // WALL STEP. The tine is snapped to the part's layer, the wall top isn't, so
        // the tine can start above the wall: up to half a layer when the gap is one
        // layer (PLA), a full layer when it's 1.5 (PETG 0.3) -- and past half a layer
        // the slice under the tine comes out empty, so the tine hangs off the part
        // without touching the wall (local issue 005). Raise the wall to meet it, under
        // the tine's wall end only (-tineOverlap..0): it sits directly beneath the tine,
        // so it never comes nearer the part than the tine does. It runs from half a layer
        // inside the wall to half a layer inside the tine, inset STEP_INSET all round, so
        // it shares no face or vertex with either: they overlap like a tine and its wall
        // (a step flush on the tine's bottom welded into a non-manifold body in Fusion).
        // Only where the slice needs it: a slicer samples each layer at mid-height, so the
        // layer under the tine holds wall while the wall top is above that layer's middle
        // (tineBot - tineH/2). Under a half-layer lift the print is already joined and
        // the output stays as it was (PLA's whole range); at half a layer or more -- the
        // tie included, a wall top ON the sampled plane -- the step goes in.
        if tine_bot - wall_top >= tine_h / 2.0 - 1e-6 {
            let e = STEP_INSET;
            let step = [
                [-prop.tine_overlap + e, -half + e],
                [-e, -half + e],
                [-e, half - e],
                [-prop.tine_overlap + e, half - e],
            ];
            box_extrude(
                &step,
                wall_top - tine_h / 2.0,
                tine_bot + tine_h / 2.0,
                p,
                out,
            );
        }
        // Capture seam: tests set globalThis.__TINECAP to an array, and so does Auto's
        // downhill judge (fins/downhill.js) to read each build's lowest tine.
        // Each tine's seed + bite heading; tests/tines_realparts.test.js reads them back
        // to verify grip on real parts through the whole pipeline. Unset -> a noop.
        ctx.capture(TineSeed {
            x,
            y,
            z: z_mid,
            bite_x: dirx,
            bite_y: diry,
        });
        count += 1;
        true
    };
    // ONE COMB, LAID UP FROM THE BOTTOM. The wall's LOW end is the part's bottom
    // edge, where a tilted part peels off first (Slant3D's "dense low"), so the comb
    // is anchored there: scan up from the low end a tine-width at a time for the
    // lowest point a nub actually grips (the last stretch is often under minTop, or
    // curls away on a curved part), then step up the WHOLE run (tail + body) at the
    // regular spacing from that anchor. An earlier version stacked three passes --
    // the body comb from half a step in, a forced nub against EACH end, and a tail
    // pass below the body -- which bunched the bottom tines at uneven gaps (35deg
    // cube: 0.8/2.1/2.8 then 2mm) and jammed a nub 0.5mm from the TOP end, where the
    // overhang face ends and it hung past the part's edge. So: no nub within
    // tineTopClear of the top end. That margin is fixed, not half a step, so a sparse
    // comb doesn't lose its last nub to a 2.5mm dead zone. The anchor scan runs as far
    // up as it must: capping it and falling back to an arbitrary phase let the whole
    // comb straddle a narrow grippable stretch and miss it entirely.
    //
    // EDGE-BIASED spacing: dense (`step`) within tineEdgeBand of either end, thinned
    // (`step * tineMidFactor`) across the middle, so the comb clusters at the run's
    // ends/corners and stops marching across a visible flat face (PROP.tineEdgeBand).
    // The step chosen for the NEXT gap depends on where we are now: still dense while
    // the current station sits in either end band. A run <= 2*band is all-edge.
    // The interior step thins by tineMidFactor but never past the slider's OWN
    // sparsest setting: edge-bias must not compound with a user who already dialed
    // grip to light and starve the comb to a few scattered nubs.
    let big_s = s[s.len() - 1];
    // Ends level to within float noise (a symmetric part: sphere walls end at the same
    // z) keep the line's own order, rather than let a 1e-9 nudge pick which end anchors
    // the comb (local issue 023: sphere 18 -> 17 tines, the mirrored comb lost one).
    let low_first = line[0][2] <= line[line.len() - 1][2] + 1e-6;
    let band = jsnum::min(prop.tine_edge_band, big_s / 2.0);
    let mid_step = if prop.tines_per_wall > 0.0 {
        step // an exact count: even, no edge bias
    } else {
        jsnum::min(step * prop.tine_mid_factor, prop.tine_step_sparse)
    };
    let u_top = big_s - jsnum::min(step / 2.0, prop.tine_top_clear);
    let z_lo = jsnum::min(line[0][2], line[line.len() - 1][2]);
    let z_hi = jsnum::max(line[0][2], line[line.len() - 1][2]);
    if body.is_none() && z_hi - z_lo < prop.tine_slope_min {
        // A LEVEL line with no tail (most wedges, squat walls): main's plain comb,
        // unchanged -- it was already even, and a level line has no bottom edge to
        // anchor. Re-phasing these moved nubs off the few grippable spots (dense
        // wedges lost 10-20% of their tines). A SLOPED line, tail or not, runs down to
        // the part's bottom edge and takes the bottom-anchored comb below.
        let mut d = step / 2.0;
        while d < big_s {
            place(d);
            d += if jsnum::min(d, big_s - d) <= band {
                step
            } else {
                mid_step
            };
        }
        return count;
    }
    // Start and scan scale down with the step on a very short run (a near-vertical
    // wedge line can be 0.1mm long in XY, and step shrinks to fit minGripTines there).
    let scan = jsnum::min(prop.tine_w, step / 2.0);
    let mut u = jsnum::min(half, step / 2.0);
    // `at(u)` = place((lowFirst ? u : S - u) - s0): u = arc length from the LOW end
    while u <= u_top && !place((if low_first { u } else { big_s - u }) - s0) {
        u += scan;
    }
    // Dense while this nub OR the next sparse one sits in an end band: anchoring
    // the comb lower shifts where it crosses into the top band, and judging only the
    // current nub let one 4mm gap straddle the band edge and cost the top a nub.
    loop {
        let dense = jsnum::min(u, big_s - u) <= band || big_s - (u + mid_step) <= band;
        u += if dense { step } else { mid_step };
        if u > u_top {
            break;
        }
        place((if low_first { u } else { big_s - u }) - s0);
    }
    count
}
