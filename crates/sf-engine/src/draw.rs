//! Port of `web/draw.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! DRAW MODE -- the user hands the tool one contact line and it sweeps ONE breakaway wall along
//! it.
//!
//! This is exactly how tools/support/breakaway.py works in the video repo: a human places one
//! wall per feature, so the wall's contact line is GIVEN, never guessed. That is the whole
//! reason breakaway.py's output was always cleaner than the auto-placer's -- the sweep was never
//! the fragile part, GUESSING a contact line for a 2D overhang region was. On a square face the
//! auto-placer fit a PCA axis to a near-isotropic footprint and the eigenvector snapped to
//! ~45deg, so the walls ran diagonally across the face and self-intersected. A line the user
//! draws is straight in XY by construction; there is nothing left to snap.
//!
//! Geometry only. The interaction (picking the two endpoints, live preview, undo) lives in
//! ui/walls.js; this module turns two surface points plus the part's triangles into a watertight
//! wall, reusing prop/sweep.js's proven `sweep` and its three line-settling passes verbatim.
//!
//! Line numbers below are at the checkout's commit (299ef49, the pin of M3); `draw.js` is
//! unchanged between 9f643a4 and 299ef49.
//!
//! **Crest state.** `drawnWall` calls `crestPart(tris, opts.layerHeight)` (module state in JS):
//! here it assigns `ctx.crest`, so `drawn_wall` takes `ctx: &mut BuildCtx` and the new part
//! stays set after the call, as in JS.

use std::sync::Arc;

use crate::config::Config;
use crate::drawarc::{DRAW_MIN_RADIUS, arc_stations, arc_through};
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::attached::PART_BAND;
use crate::prop::brace::{BraceKinds, brace_wall};
use crate::prop::contact::{contour_top, lower_sag, settle_top};
use crate::prop::crest::{Crest, crest_on};
use crate::prop::ctx::BuildCtx;
use crate::prop::mold::{FloorPt, floor_line, mold_line};
use crate::prop::squat::sweep_squat;
use crate::prop::surface::{surface_hits_at, surface_zs_at};
use crate::prop::sweep::{foot_for, sweep, sweep_between};
use crate::prop::tines::{TineOpts, emit_tines, tine_step_for};
use crate::tris::Tris;
use crate::types::{Rot3, V4, Xyz};

type V3 = [f64; 3];

/// `DRAW_MIN_LEN` (web/draw.js:27): a hand-drawn wall may be much shorter than an auto wall
/// (PROP.minSpan, 7 mm): the user is pointing at one small overhang -- a cleat, a fingertip, a
/// chin -- that no auto wall reaches. 2 mm still gives the three stations a sweep needs (local
/// issue 011).
pub const DRAW_MIN_LEN: f64 = 2.0;

/// `BAND_STEP` (web/draw.js:91).
const BAND_STEP: f64 = 0.25;
/// `SQUAT_INSET` (web/draw.js:94): mm a mixed run's squat stem is narrowed on each side where it
/// runs inside the full wall's stem: clear of its faces, far under what a slicer resolves.
const SQUAT_INSET: f64 = 0.01;

/// Port of `surfaceZsAt` (web/draw.js:38-40): every surface height directly above (x, y), as a
/// list.
///
/// `surfaceZAt` in prop/surface.js returns only the LOWEST, which is what an auto-placer wants
/// (the underside a wall props to). A hand-drawn wall instead wants the surface the user
/// actually clicked, so this keeps them all and the caller picks the one nearest the line the
/// user drew -- otherwise a wall under a shallow overhang would jump down to whatever
/// plate-resting geometry shares its (x, y).
fn surface_zs_all(tris: &Tris, x: f64, y: f64) -> Vec<f64> {
    surface_hits_at(tris, x, y).iter().map(|h| h[0]).collect()
}

/// Port of `solidAt` (web/draw.js:61-67): is (x, y, z) inside the part? Odd number of surfaces
/// below it in that column. The squat fallback has no clearance probe of its own (Auto's
/// stationIsClear needs the topology), and at 0.6 mm of headroom a line on an UPWARD face reads
/// that face as its floor -- so it checks the solid directly.
fn solid_at(tris: &Tris, x: f64, y: f64, z: f64) -> bool {
    // a column through a shared edge hits both triangles: one surface, counted once
    let mut zs: Vec<f64> = surface_zs_all(tris, x, y)
        .into_iter()
        .filter(|&zz| zz < z)
        .collect();
    // `.sort((p, q) => p - q)`: a NaN difference compares as 0
    zs.sort_by(|p, q| {
        let d = p - q;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let mut n = 0;
    for i in 0..zs.len() {
        if i == 0 || zs[i] - zs[i - 1] > 1e-5 {
            n += 1;
        }
    }
    n % 2 == 1
}

/// Port of `underAnOverhang` (web/draw.js:71-74): each station sits under a DOWN-facing surface:
/// air just below it and part within half a mm above (the settle passes leave a top a little
/// under the underside).
fn under_an_overhang(line: &[V3], tris: &Tris) -> bool {
    line.iter().all(|p| {
        !solid_at(tris, p[0], p[1], p[2] - 0.02)
            && [0.02, 0.1, 0.25, 0.5]
                .iter()
                .any(|&d| solid_at(tris, p[0], p[1], p[2] + d))
    })
}

/// Port of `onThePart` (web/draw.js:79-88): does an interior station lie ON the part -- the
/// surface it touches facing up? A line drawn on an upward face does; one under an overhang
/// touches a face looking down. Not the end stations: Auto and the user put those on the
/// overhang's edge.
fn on_the_part(line: &[V3], tris: &Tris) -> bool {
    line.iter().enumerate().any(|(i, p)| {
        if i == 0 || i == line.len() - 1 {
            return false;
        }
        let mut best: Option<[f64; 2]> = None;
        for h in surface_hits_at(tris, p[0], p[1]) {
            let closer = match best {
                None => true,
                Some(b) => jsnum::abs(h[0] - p[2]) < jsnum::abs(b[0] - p[2]),
            };
            if jsnum::abs(h[0] - p[2]) < 0.5 && closer {
                best = Some(h);
            }
        }
        matches!(best, Some(b) if b[1] > 0.0)
    })
}

/// Port of `bandHitsPart` (web/draw.js:95-111): does part surface cross z0..z1(i) anywhere under
/// the wall, w(i) either side of its centre line? A body resting on the plate beside a plate
/// wall's flange puts its bottom face in the flange's band; one standing in its stem, a side
/// face. Sampled every BAND_STEP along and across -- at the stations alone, a pin 0.5 mm thick
/// slipped between them -- through prop.js's gridded surfaceZsAt (a mini has ~1M faces).
fn band_hits_part(
    line: &[V3],
    tris: &Tris,
    w: impl Fn(usize) -> f64,
    z0: f64,
    z1: impl Fn(usize) -> f64,
) -> bool {
    for i in 0..line.len().saturating_sub(1) {
        let p = line[i];
        let q = line[i + 1];
        let len = jsnum::hypot(&[q[0] - p[0], q[1] - p[1]]);
        if len < 1e-9 {
            continue;
        }
        let rx = (q[0] - p[0]) / len;
        let ry = (q[1] - p[1]) / len;
        let wi = jsnum::max(w(i), w(i + 1));
        let top = jsnum::max(z1(i), z1(i + 1));
        let na = jsnum::ceil(len / BAND_STEP);
        let nc = jsnum::ceil((2.0 * wi) / BAND_STEP);
        let mut s = 0.0;
        while s <= na {
            let mut k = 0.0;
            while k <= nc {
                let t = (len * s) / na;
                let o = -wi + (2.0 * wi * k) / nc;
                for z in surface_zs_at(tris, p[0] + rx * t + ry * o, p[1] + ry * t - rx * o) {
                    if z >= z0 && z <= top {
                        return true;
                    }
                }
                k += 1.0;
            }
            s += 1.0;
        }
    }
    false
}

/// Port of `acrossClear` (web/draw.js:114-126): points across a wall at each station, `w` either
/// side of its centre line, at height z(i).
fn across_clear(line: &[V3], tris: &Tris, w: f64, z: impl Fn(usize) -> f64) -> bool {
    for i in 0..line.len() {
        let a = line[jsnum::max(0.0, i as f64 - 1.0) as usize];
        let b = line[jsnum::min((line.len() - 1) as f64, (i + 1) as f64) as usize];
        let mut rx = b[0] - a[0];
        let mut ry = b[1] - a[1];
        let rn = jsnum::hypot(&[rx, ry]);
        if rn < 1e-9 {
            return false;
        }
        rx /= rn;
        ry /= rn;
        for o in [-w, -w / 2.0, 0.0, w / 2.0, w] {
            if solid_at(tris, line[i][0] + ry * o, line[i][1] - rx * o, z(i)) {
                return false;
            }
        }
    }
    true
}

/// Port of `drawnLine` (web/draw.js:136-178): the contact polyline for a wall drawn from `a` to
/// `b` (both surface points in PRINT space, `[x, y, z]`). Straight in XY by construction. Each
/// station's height is the part surface nearest the line the user drew -- not the global lowest,
/// which would jump to another feature -- and then the three prop/contact.js passes pull the top
/// to a clean `gap` below the part exactly as the auto-placer does. `under` limits the pick to
/// down-facing surfaces (the fill pass; see below).
///
/// JS defaults: `step = PROP.stationStep`, `band = Infinity`, `under = false`, `through = null`;
/// callers pass them explicitly. `None` is JS's `null`.
pub fn drawn_line(
    a: V3,
    b: V3,
    tris: &Tris,
    step: f64,
    band: f64,
    under: bool,
    through: Option<V3>,
    cfg: &Config,
) -> Option<Vec<V3>> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = jsnum::hypot(&[dx, dy]);
    if len < 1e-6 {
        return None;
    }
    // `through`: a curved wall's third click -- the stations run round the arc from a to b
    // through it (drawarc.js); in line with a and b it is the straight wall
    let arc = through.and_then(|m| arc_stations(a, b, m, step, cfg.prop.min_stations));
    let n = match &arc {
        Some(arc) => (arc.len() - 1) as f64,
        None => jsnum::max(cfg.prop.min_stations - 1.0, jsnum::ceil(len / step)),
    };
    let mut line: Vec<V3> = Vec::new();
    let mut k = 0.0;
    while k <= n {
        let t = k / n;
        let (x, y) = match &arc {
            Some(arc) => (arc[k as usize][0], arc[k as usize][1]),
            None => (a[0] + dx * t, a[1] + dy * t),
        };
        // the height the drawn line implies here
        let hint = match &arc {
            Some(arc) => arc[k as usize][2],
            None => a[2] + (b[2] - a[2]) * t,
        };
        let mut z = hint;
        let mut best = f64::INFINITY;
        // `under`: only surfaces facing DOWN. A column through a leaning figure meets its back
        // (the underside, red) and its front; on a curved back the straight-line hint lands
        // nearer the front, so a long wall the fill pass laid down a spine read "faces up" at
        // every station. A hand-drawn line keeps the nearest of all.
        // (Through prop's grid: the fill pass runs this per candidate line, and a linear scan
        // per station was minutes on a 1M-face mini.)
        let zs: Vec<f64> = if under {
            surface_hits_at(tris, x, y)
                .iter()
                .filter(|h| h[1] < 0.0)
                .map(|h| h[0])
                .collect()
        } else {
            surface_zs_all(tris, x, y)
        };
        for zz in zs {
            let d = jsnum::abs(zz - hint);
            if d < best {
                best = d;
                z = zz;
            }
        }
        line.push([x, y, z]);
        k += 1.0;
    }
    // `band` bounds every settle pass to surfaces within `band` of the clicked overhang, so an
    // over-the-part overhang isn't dragged down onto its own floor (the collapse that made
    // part-attached supports impossible). Infinity keeps the bed-attached behaviour exactly.
    contour_top(&mut line, tris, band, cfg);
    lower_sag(&mut line, tris, band);
    settle_top(&mut line, tris, 0.25, band, cfg);
    Some(line)
}

/// `opts.braceClear`: `(tris) => bool`.
pub type BraceClear<'a> = &'a dyn Fn(&[V4]) -> bool;

/// The `opts` of `drawnWall` (web/draw.js:166-176, 194-216), every key with its JS default
/// (`undefined`/falsy: the `Default`).
///
/// - `under`: follow only down-facing surfaces (`drawn_line`) -- the fill pass's walls.
/// - `plate_only`: never stand on the part; a line with part under it is refused (#218).
/// - `brace`: a wall that stands on the plate over BRACE.aspect x its length gets ribs at the
///   plate (prop/brace.js; `braces` counts them). `brace_tall`: one over BRACE.tallH gets face
///   ribs along it (the Brace tall walls box), each kept clear of other supports by
///   `brace_clear(tris)` when given (JS default `() => true`, in `braceWall`).
/// - `through`: a curved wall's third click (drawarc.js), refused when it bends tighter than its
///   foot can follow. In line with the ends it is the straight wall.
/// - `tines` with `topo`/`rot`/`offset`: the wall grips with the tine comb; `emitTines` needs
///   the part in topology form (for bite direction), so it only runs when the caller passes
///   `topo`/`rot`/`offset` -- the live preview omits them and stays a plain wall for speed.
/// - `layer_height`: `opts.layerHeight` (`crestPart`'s and `emitTines`' layer height).
#[derive(Clone, Default)]
pub struct DrawOpts<'a> {
    pub under: bool,
    pub plate_only: bool,
    pub brace: bool,
    pub brace_tall: bool,
    pub brace_clear: Option<BraceClear<'a>>,
    pub through: Option<V3>,
    pub layer_height: Option<f64>,
    pub tines: bool,
    pub topo: Option<&'a Topology>,
    pub rot: Option<Rot3>,
    pub offset: Option<Xyz>,
    pub tine_density: Option<f64>,
}

impl std::fmt::Debug for DrawOpts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrawOpts")
            .field("under", &self.under)
            .field("plate_only", &self.plate_only)
            .field("brace", &self.brace)
            .field("brace_tall", &self.brace_tall)
            .field("brace_clear", &self.brace_clear.is_some())
            .field("through", &self.through)
            .field("layer_height", &self.layer_height)
            .field("tines", &self.tines)
            .field("topo", &self.topo.is_some())
            .field("rot", &self.rot)
            .field("offset", &self.offset)
            .field("tine_density", &self.tine_density)
            .finish()
    }
}

/// What a successful `drawnWall` returns (web/draw.js:231, 252-253, 280, 299): `{ ok: true, tris,
/// length, height, top, ... }`. Keys only some returns carry are `Option`/`bool`: `partAttached`
/// (part-attached and squat), `squat` (squat), `braces` and `floors` (not the squat return),
/// `tines`. `top` is the wall's contact line, the surface-z stations its top follows; `floors`
/// each station's bottom z, for a caller that braces later (the fill pass, once every wall is
/// in).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnOk {
    pub tris: Vec<V4>,
    pub length: f64,
    pub height: f64,
    /// `partAttached`: `Some(true)` on the part-attached return, `Some(partAttached)` on the
    /// squat return, `None` (key absent) on the plate return.
    pub part_attached: Option<bool>,
    /// `squat: true` on the squat return only.
    pub squat: bool,
    pub tines: usize,
    /// `braces`: absent on the squat return.
    pub braces: Option<usize>,
    pub top: Vec<V3>,
    /// `floors`: absent on the squat return.
    pub floors: Option<Vec<f64>>,
}

/// `{ ok: true, ... } | { ok: false, reason }` (web/draw.js:160-165): a hand-drawn wall that
/// can't be built says WHY (too short, at the plate) rather than silently doing nothing, the
/// failure mode M5's scoreboard was built on. The `reason` is the message the UI shows,
/// formatted exactly as JS does.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawnWall {
    Ok(DrawnOk),
    Refused { reason: String },
}

impl DrawnWall {
    /// `r.ok`
    pub fn ok(&self) -> bool {
        matches!(self, DrawnWall::Ok(_))
    }
}

fn refuse(reason: impl Into<String>) -> DrawnWall {
    DrawnWall::Refused {
        reason: reason.into(),
    }
}

/// `floorLine` row as the `[x, y, z, (zNeg, zPos)]` array `sweepBetween` reads.
fn floor_vec(f: &FloorPt) -> Vec<f64> {
    let mut v = vec![f.x, f.y, f.z];
    if let Some(s) = f.sides {
        v.extend(s);
    }
    v
}

/// The `withTines` closure of `drawnWall` (web/draw.js:211-214): a drawn wall grips the part
/// with the same tine comb the auto fins use, when Tines is on. `topLine` is the wall's
/// surface-z contact line (emitTines subtracts the gap itself). (`min_top`: a squat plate wall's
/// base is its thin brim, so its tines start from squatBrimH, as Auto's squat walls do -- the
/// flange default would skip every one.)
fn with_tines(
    opts: &DrawOpts,
    line: &[V3],
    tris: &Tris,
    min_top: Option<f64>,
    out: &mut Vec<V4>,
    ctx: &BuildCtx,
) -> usize {
    let cfg = ctx.cfg;
    match (opts.tines, opts.topo) {
        (true, Some(topo)) => emit_tines(
            line,
            Some(tris),
            topo,
            // JS passes `opts.rot` / `opts.offset` as given; a caller with `topo` and no `rot`
            // would throw inside emitTines
            &opts.rot.expect("drawnWall: opts.topo without opts.rot"),
            &opts
                .offset
                .expect("drawnWall: opts.topo without opts.offset"),
            out,
            TineOpts {
                step: Some(tine_step_for(opts.tine_density, cfg)),
                min_top,
                tine_h: Some(opts.layer_height.unwrap_or(cfg.prop.tine_h)),
                body: None,
                grip: None,
            },
            ctx,
        ),
        _ => 0,
    }
}

/// The `withBraces` closure of `drawnWall` (web/draw.js:215-217).
fn with_braces(
    opts: &DrawOpts,
    line: &[V3],
    floor_z: impl Fn(usize) -> f64,
    tris: &Tris,
    z_bed: f64,
    out: &mut Vec<V4>,
    cfg: &Config,
) -> usize {
    if opts.brace || opts.brace_tall {
        brace_wall(
            line,
            floor_z,
            tris,
            z_bed,
            out,
            |v| opts.brace_clear.is_none_or(|c| c(v)),
            BraceKinds {
                slim: opts.brace,
                tall: opts.brace_tall,
            },
            cfg,
        )
    } else {
        0
    }
}

/// Port of `drawnWall` (web/draw.js:195-327): build one drawn breakaway wall. Returns
/// `Ok(DrawnOk)` (`top`: the wall's contact line, the surface-z stations its top follows) or
/// `Refused { reason }` with a message the UI can show -- a hand-drawn wall that can't be built
/// should say WHY (too short, at the plate) rather than silently doing nothing, the failure mode
/// M5's scoreboard was built on.
///
/// `z_bed` is JS's `zBed = 0` default (callers pass it). `tris` is the part's seated soup as a
/// shared `Arc`, because `crestPart(tris, ..)` keeps it in `ctx.crest`.
pub fn drawn_wall(
    a: V3,
    b: V3,
    tris: &Arc<Tris>,
    z_bed: f64,
    opts: &DrawOpts,
    ctx: &mut BuildCtx,
) -> DrawnWall {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let chord = jsnum::hypot(&[dx, dy]);
    if chord < DRAW_MIN_LEN {
        return refuse(format!(
            "wall too short — {}mm, needs {}mm",
            jsnum::to_fixed(chord, 1),
            jsnum::to_js_string(DRAW_MIN_LEN)
        ));
    }
    // `opts.through`: a curved wall (drawarc.js), refused when it bends tighter than its foot
    // can follow. In line with the ends it is the straight wall.
    let bend = opts
        .through
        .and_then(|m| arc_through([a[0], a[1]], [b[0], b[1]], [m[0], m[1]]));
    if let Some(bend) = &bend
        && bend.r < DRAW_MIN_RADIUS
    {
        return refuse(format!(
            "curve too tight — {}mm radius, needs {}mm",
            jsnum::to_fixed(bend.r, 1),
            jsnum::to_js_string(DRAW_MIN_RADIUS)
        ));
    }
    let through = if bend.is_some() { opts.through } else { None };
    let len = match &bend {
        Some(bend) => jsnum::abs(bend.sweep) * bend.r,
        None => chord,
    };
    let mut out: Vec<V4> = Vec::new();
    if crest_on(cfg) {
        // the interface crest's 'flat' test reads the underside here
        ctx.crest = Crest::new(
            Some(Arc::clone(tris)),
            opts.layer_height.unwrap_or(0.0),
            cfg,
        );
    }
    let ctx: &BuildCtx = ctx;
    let tris: &Tris = tris;
    // PART-ATTACHED first: if solid part sits below the overhang, the support stands on THAT,
    // not the plate. Probe with a BANDED top contour so the overhang isn't settled down onto
    // the very floor we're looking for; floorLine then returns the nearest surface below each
    // station (0 where the path to the plate is open). A real floor anywhere along the line
    // routes the whole wall through sweepBetween, whose bottom is the per-station floor -- bed
    // stations degrade to z=0 on their own. This is what stops the "marched past the part
    // straight to the plate" bug: on the over-the-part case sweep-to-plate SUCCEEDS and silently
    // builds the tall stilt, so the fix must PREFER the floor.
    let top_pa = drawn_line(
        a,
        b,
        tris,
        prop.station_step,
        PART_BAND,
        opts.under,
        through,
        cfg,
    );
    // ...unless the line lies ON the part: drawn on an UPWARD face (the UI lets you click one),
    // it read the part's own underside as its floor and stood the wall inside the part, floor to
    // top (local issue 033: a sphere's top, 296-384 vertices in). That line falls through to the
    // plate wall below.
    if let Some(top_pa) = &top_pa
        && top_pa.len() as f64 >= prop.min_stations
        && !on_the_part(top_pa, tris)
    {
        let floor = floor_line(top_pa, tris, 1.0, false, cfg);
        let mut floor_max = 0.0;
        for p in &floor {
            if p.z > floor_max {
                floor_max = p.z;
            }
        }
        // Plate only (#218): part under the overhang refuses the wall outright -- the plate
        // wall below would stilt straight through that part.
        if opts.plate_only && floor_max > prop.gap + 0.5 {
            return refuse(
                "part of the model sits under this overhang, and Plate only is on — a support can’t stand on the part here",
            );
        }
        let mold = if floor_max > prop.gap + 0.5 {
            Some(mold_line(top_pa, tris, cfg))
        } else {
            None
        };
        if let Some(mold) = mold {
            let floors: Vec<Vec<f64>> = mold.floor.iter().map(floor_vec).collect();
            if sweep_between(&mold.top, &floors, &mut out, prop.min_height, ctx) {
                let mut height = 0.0;
                for i in 0..top_pa.len() {
                    height = jsnum::max(height, (top_pa[i][2] - prop.gap) - floor[i].z);
                }
                // a station whose floor is open air runs down to the plate (floorLine's 0):
                // with no flange there, it is the one that most needs a brace (Isaac's hands)
                let braces = with_braces(opts, top_pa, |i| floor[i].z, tris, z_bed, &mut out, cfg);
                let tines = with_tines(opts, top_pa, tris, None, &mut out, ctx);
                return DrawnWall::Ok(DrawnOk {
                    tris: out,
                    length: len,
                    height,
                    part_attached: Some(true),
                    squat: false,
                    tines,
                    braces: Some(braces),
                    top: top_pa.clone(),
                    floors: Some(floor.iter().map(|f| f.z).collect()),
                });
            }
        }
    }

    let line = drawn_line(
        a,
        b,
        tris,
        prop.station_step,
        f64::INFINITY,
        opts.under,
        through,
        cfg,
    );
    let line = match line {
        Some(l) if l.len() as f64 >= prop.min_stations => l,
        _ => return refuse("no surface found along that line"),
    };
    // The flange and the stem must clear the part beside the line (Auto's clearance pass
    // measures the same profile): drawn beside a body resting on the plate, the flange reached
    // 0.4-1.1 mm into it (local issue 033). Checked before sweep writes.
    let top = |i: usize| line[i][2] - prop.gap - z_bed;
    if line
        .iter()
        .enumerate()
        .all(|(i, _)| top(i) >= prop.min_height)
        && (band_hits_part(
            &line,
            tris,
            |i| foot_for(top(i), cfg),
            z_bed - 0.01,
            |_| z_bed + prop.base_h + prop.gap,
        ) || band_hits_part(
            &line,
            tris,
            |_| prop.th / 2.0,
            z_bed - 0.01,
            |i| z_bed + top(i) / 2.0,
        ))
    {
        return refuse(
            "too close to the part — the wall’s foot would cut into it; draw the line a little further out",
        );
    }
    // Reaching here means no real floor was found below the overhang, so this is a
    // plate-attached wall. `sweep` returns false when any station is shorter than
    // PROP.minHeight, and two situations produce that:
    //   - the drawn line genuinely sits near the plate (drawn by a resting edge);
    //   - the user pointed at a real overhang HIGH above the bed, but other part geometry sits
    //     directly under it, so contourTop/settleTop pull the wall top down to that lower
    //     surface and it collapses.
    // The over-the-part case (the L-bracket boss over its base) is now handled by the
    // part-attached branch above, which stands on that lower surface instead of collapsing --
    // so a failure here that still LOOKS over-the-part means the floor was out of reach (too
    // thin a gap to seat a wall), not that we ignore it.
    // The clicked endpoints' heights are exactly "what the user pointed at", so a high clickTop
    // with a failed sweep is the unreachable case, not a low line.
    if !sweep(&line, z_bed, &mut out, prop.min_height, ctx) {
        // Neither full-height wall fits. Before refusing, the two SQUAT walls a user drawing by
        // hand can still want (local issue 011) -- tried only now, so every drawn wall that
        // built before builds exactly the same:
        //   - on the part, with as little as minHeightSquat headroom (a hand just over a thigh);
        //   - on the plate, its low stations a brimmed squat stem, the tall ones the full wall.
        let squat = squat_wall(top_pa.as_deref(), &line, tris, z_bed, opts.plate_only, ctx);
        if let Some(squat) = squat {
            out.extend(squat.tris);
            let tines = with_tines(
                opts,
                &squat.top,
                tris,
                if squat.part_attached {
                    None
                } else {
                    Some(prop.squat_brim_h)
                },
                &mut out,
                ctx,
            );
            return DrawnWall::Ok(DrawnOk {
                tris: out,
                length: len,
                height: squat.height,
                part_attached: Some(squat.part_attached),
                squat: true,
                tines,
                braces: None,
                top: squat.top,
                floors: None,
            });
        }
        let click_top = jsnum::min(a[2], b[2]) - z_bed;
        if click_top >= prop.min_height + prop.gap {
            return refuse(
                "this overhang sits above another part of the model, so a wall standing on the plate can’t reach it — rotate so it faces the plate",
            );
        }
        return refuse("nothing to hold up there — the line sits at the plate");
    }
    let mut height = 0.0;
    for p in &line {
        height = jsnum::max(height, p[2] - prop.gap - z_bed);
    }
    let braces = with_braces(opts, &line, |_| z_bed, tris, z_bed, &mut out, cfg);
    let tines = with_tines(opts, &line, tris, None, &mut out, ctx);
    let floors = line.iter().map(|_| z_bed).collect();
    DrawnWall::Ok(DrawnOk {
        tris: out,
        length: len,
        height,
        part_attached: None,
        squat: false,
        tines,
        braces: Some(braces),
        top: line,
        floors: Some(floors),
    })
}

/// What `squatWall` returns: `{ tris, top, height, partAttached }`.
struct SquatWall {
    tris: Vec<V4>,
    top: Vec<V3>,
    height: f64,
    part_attached: bool,
}

/// Port of `squatWall` (web/draw.js:337-401): the squat fallback for a drawn line no
/// full-height wall fits: `{ tris, top, height, partAttached }` or null. Part first
/// (sweepBetween down to minHeightSquat of headroom -- in practice just over 1 mm of room, since
/// floorLine skips surfaces within its 1 mm margin), then the plate: stations from
/// minHeightSquat to minHeight get a brimmed squat stem (sweepSquat, what Auto builds near the
/// bed), taller runs keep the flanged wall. A station under minHeightSquat still refuses the
/// whole line. `plate_only`: the plate only (JS default `false`).
fn squat_wall(
    top_pa: Option<&[V3]>,
    line: &[V3],
    tris: &Tris,
    z_bed: f64,
    plate_only: bool,
    ctx: &BuildCtx,
) -> Option<SquatWall> {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    if let Some(top_pa) = top_pa
        && !plate_only
        && top_pa.len() as f64 >= prop.min_stations
        && under_an_overhang(top_pa, tris)
    {
        let floor = floor_line(top_pa, tris, 1.0, false, cfg);
        // the wall's middle, between its floor and its top, is air (on an upward slope the
        // "floor" is the slope itself and this is solid)
        let mid_clear = across_clear(top_pa, tris, prop.th / 2.0, |i| {
            (top_pa[i][2] - prop.gap + floor[i].z + prop.foot_gap) / 2.0
        });
        if mid_clear && floor.iter().any(|p| p.z > prop.gap + 0.5) {
            let mold = mold_line(top_pa, tris, cfg);
            let mut t: Vec<V4> = Vec::new();
            let floors: Vec<Vec<f64>> = mold.floor.iter().map(floor_vec).collect();
            if sweep_between(&mold.top, &floors, &mut t, prop.min_height_squat, ctx) {
                let mut height = 0.0;
                for i in 0..top_pa.len() {
                    height = jsnum::max(height, top_pa[i][2] - prop.gap - floor[i].z);
                }
                return Some(SquatWall {
                    tris: t,
                    top: top_pa.to_vec(),
                    height,
                    part_attached: true,
                });
            }
        }
    }
    let h: Vec<f64> = line.iter().map(|p| p[2] - prop.gap - z_bed).collect();
    if h.iter().any(|&v| v < prop.min_height_squat) {
        return None;
    }
    // a plate wall stands only where the way down is clear: part under a station (too close for
    // the squat wall on it above) would put this one straight through it
    if floor_line(line, tris, 1.0, false, cfg)
        .iter()
        .any(|p| p.z > prop.gap + 0.5)
    {
        return None;
    }
    // ...and only under a real overhang, with room for the whole brim (squatBrimW either side,
    // wider than the stem floorLine looked under) and the stem beside the line
    if !under_an_overhang(line, tris) {
        return None;
    }
    // (probed at the brim's TOP plus the gap: under an underside that dips beside the line,
    // mid-height passed while the brim's top corner sat in the part -- torus)
    if !across_clear(line, tris, prop.squat_brim_w, |_| {
        z_bed + prop.squat_brim_h + prop.gap
    }) {
        return None;
    }
    if !across_clear(line, tris, prop.th / 2.0, |i| {
        z_bed + (line[i][2] - prop.gap - z_bed) / 2.0
    }) {
        return None;
    }
    // runs of tall (full wall) and low (squat) stations; a tall run under two stations can't
    // sweep, so it goes squat with its neighbours
    let mut tall: Vec<bool> = h.iter().map(|&v| v >= prop.min_height).collect();
    for i in 0..tall.len() {
        // `tall[i - 1]` / `tall[i + 1]` past the ends are `undefined`, falsy
        let before = i > 0 && tall[i - 1];
        let after = i + 1 < tall.len() && tall[i + 1];
        if tall[i] && !before && !after {
            tall[i] = false;
        }
    }
    let mut t: Vec<V4> = Vec::new();
    let mut i = 0;
    let mut squats = 0usize;
    while i < line.len() {
        let mut j = i;
        while j + 1 < line.len() && tall[j + 1] == tall[i] {
            j += 1;
        }
        if tall[i] {
            if !sweep(&line[i..=j], z_bed, &mut t, prop.min_height, ctx) {
                return None;
            }
        } else {
            // a squat run reaches TWO stations into each tall neighbour: the tall sweep ends at
            // its own first/last station, so one would only meet it face to face -- the pieces
            // must overlap, never sit flush (Fusion's weld). sweepSquat takes any h >= 0.6.
            // On those shared stations the squat stem has the full wall's section, so it is
            // inset a hair to run inside it rather than on its faces (local issue 057); every
            // other squat run twice as far, since across a tall run of 2-3 stations the squat
            // runs either side reach into each other too.
            let from = i.saturating_sub(2);
            let seg = &line[from..line.len().min(j + 3)];
            let inn = SQUAT_INSET * (1 + squats % 2) as f64;
            squats += 1;
            let inset: Vec<f64> = (0..seg.len())
                .map(|k| if tall[from + k] { inn } else { 0.0 })
                .collect();
            if seg.len() < 2 || !sweep_squat(seg, z_bed, &mut t, Some(&inset), ctx) {
                return None;
            }
        }
        i = j + 1;
    }
    Some(SquatWall {
        tris: t,
        top: line.to_vec(),
        height: jsnum::max_of(&h),
        part_attached: false,
    })
}
