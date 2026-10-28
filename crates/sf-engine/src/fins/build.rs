//! Port of `web/fins.js` (`buildFins`, `applyTunables` lives in `crate::config`). Read `docs/AGENTS.md` before changing anything here.

use indexmap::IndexMap;

use crate::fins::seating::Seating;
use crate::pieces::FloatingPiece;
use crate::planes::PatchStats;
use crate::prop::build::{PropRecord, Skipped};
use crate::types::V4;

type V3 = [f64; 3];

/// What kind of fin a [`FinRecord`] is (`kind`, web/fins.js:262, 291, 352, 446): `'prop'` (the
/// default, `q.kind ?? 'prop'`), `'wedge'` (fins.js:331) or `'sway'` (fins.js:178).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinKind {
    Prop,
    Wedge,
    Sway,
}

/// One entry of `built.fins`, the unified per-fin array (web/fins.js:325-358 and 446-453 in
/// `buildFinsCore`, 172-181 in `buildFinsAndBraces`, 252-255 in `withFill`): the stats fields
/// (`height`, `length`, `tines`, `rows`, `stilt`, `lean`, `bearing`, `site`) and the `id`, `kind`,
/// `triRanges`, `line`, `span` the UI addresses a fin by. Keys only some fins carry are
/// `Option`/`bool`.
#[derive(Clone, Debug, PartialEq)]
pub struct FinRecord {
    pub height: f64,
    pub length: f64,
    pub tines: usize,
    pub rows: usize,
    pub stilt: f64,
    pub lean: f64,
    pub bearing: f64,
    /// `site`: always `null` in every writer.
    pub site: Option<f64>,
    pub id: usize,
    pub kind: FinKind,
    /// `triRanges`: `[from, to)` ranges of `built.triangles` (a prop's, offset past the base
    /// pass for wedges, the short walls, the fill and the sway braces).
    pub tri_ranges: Vec<[usize; 2]>,
    /// `line`: a prop's or wedge's centreline; a sway brace's two foot points (`r.foot`).
    pub line: Vec<V3>,
    pub span: f64,
    /// `short: true` on a last-resort wall (fins.js:353).
    pub short: bool,
    /// `fill: true` on a fill-pass wall (fins.js:253).
    pub fill: bool,
    /// `joined: true` (fins.js:254), a fill wall that joins another.
    pub joined: bool,
    /// `braces` (fins.js:254): the ribs built on a fill wall; `braceTallWalls` skips these.
    pub braces: Option<usize>,
    /// `ribs` (web/fins/tall.js:131): the face ribs `braceTallWalls` built on this wall.
    pub ribs: Option<f64>,
}

/// `built.pad`: the stats object `buildPad` returns (web/fins/pad.js:249, 520), canonical in
/// [`crate::fins::pad`] (its `outline` is the outline's length in mm, a float).
pub use crate::fins::pad::PadInfo;

/// `built.rejected` (web/fins.js:456-458): `{ blocked, tooFewTines, sites, tried }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rejected {
    pub blocked: usize,
    pub too_few_tines: usize,
    pub sites: usize,
    pub tried: usize,
}

/// `built.fill` (web/fins.js:262-266): the fill pass's numbers, from its stats and its
/// unserved leftover.
#[derive(Clone, Debug, PartialEq)]
pub struct FillStats {
    pub walls: usize,
    pub rows: usize,
    pub rows_lost: f64,
    pub row_builds: usize,
    pub joined: usize,
    pub ribs: Option<usize>,
    pub ties: Option<usize>,
    pub tries: usize,
    pub checks: usize,
    pub capped: bool,
    /// `refused`: why-not message to count, in first-seen order.
    pub refused: IndexMap<String, usize>,
    pub bare_before: f64,
    pub unserved_area: f64,
    pub unserved_pts: Vec<V3>,
    pub islands: usize,
    pub islands_bare: usize,
    pub island_pts: Vec<V3>,
    pub islands_capped: bool,
}

/// `built.curvedFill` (web/fins.js:274): `{ walls, unservedArea, capped }`, Auto with curved
/// walls (it replaces `fill`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvedFill {
    pub walls: usize,
    pub unserved_area: f64,
    pub capped: bool,
}

/// `built.bare` (web/fins.js:92-97): how much red the walls leave bare, `{ area, total,
/// coverage }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bare {
    pub area: f64,
    pub total: f64,
    pub coverage: f64,
}

/// One cross-rib arm of a sway brace (`sway.js:474`): `{ a, b, th, plan }`.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayArm {
    pub a: V3,
    pub b: V3,
    pub th: f64,
    pub plan: Vec<[f64; 2]>,
}

/// One footprint level of a sway brace (`sway.js:446`): `{ z, a, b }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwayLevel {
    pub z: f64,
    pub a: V3,
    pub b: V3,
}

/// One brace outline sent back in `built.sway.braces` (web/fins.js:193-195):
/// `{ foot, halfW, th, height, levels, arms }`.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayBrace {
    pub foot: [V3; 2],
    pub half_w: f64,
    pub th: f64,
    pub height: f64,
    pub levels: Vec<SwayLevel>,
    pub arms: Vec<SwayArm>,
}

/// `built.sway` (web/fins.js:187-196): `{ count, tines, skipped, reason, cross, braces }`.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayInfo {
    pub count: usize,
    pub tines: usize,
    pub skipped: usize,
    /// `reason`: `null` when ribs were built.
    pub reason: Option<String>,
    pub cross: usize,
    pub braces: Vec<SwayBrace>,
}

/// The object `buildFins` returns and its passes mutate (web/fins.js:79-110 `buildFins`, 112-199
/// `buildFinsAndBraces`, 208-271 `withFill`, 273-446 `buildFinsCore`'s auto branch, 447-476 its
/// prop return, fins/tall.js `braceTallWalls`). Keys follow the prop-mode return in JS order,
/// then what the later stages add; a key a stage writes only sometimes is an `Option` / `bool`
/// that is `None` / `false` until then (`None` is the key absent).
///
/// `patchStats` is JS's `{}` and the counters start at 0 here; **when it is serialised the zero
/// counters are omitted** (issue #23).
#[derive(Clone, Debug, PartialEq)]
pub struct FinsBuilt {
    pub triangles: Vec<V4>,
    pub pad_triangles: Vec<V4>,
    pub pad: Option<PadInfo>,
    pub mode: String,
    pub fins: Vec<FinRecord>,
    pub props: Vec<PropRecord>,
    /// `volume` (prop mode's `built.volume`; the auto branch spreads it from its base).
    pub volume: f64,
    pub skipped: Skipped,
    pub rejected: Rejected,
    pub patch_count: usize,
    pub patch_stats: PatchStats,
    pub tines: usize,
    pub served_regions: Vec<usize>,
    /// `unserved`: regions not served; `(base.unserved ?? 0) - short.served.length` can go
    /// negative in JS, so a float.
    pub unserved: f64,
    pub sag_risk: bool,
    pub seating: Seating,
    /// `tip`: always `null`.
    pub tip: Option<f64>,
    // -- the auto/full branch (fins.js:337-446) --
    /// `braceCount` (auto/full only).
    pub brace_count: Option<usize>,
    /// `propCount` (auto/full only).
    pub prop_count: Option<usize>,
    /// `fill`: Full coverage's numbers (replaced by `curved_fill` in Auto with curved walls).
    pub fill: Option<FillStats>,
    pub curved_fill: Option<CurvedFill>,
    /// `curvedDropped: true`: Full coverage dropped the curves (fins.js:296).
    pub curved_dropped: bool,
    // -- `buildFinsAndBraces` --
    pub sway: Option<SwayInfo>,
    // -- `buildFins` --
    /// `smallHoleRegions` (`result.smallHoleRegions ?? 0`).
    pub small_hole_regions: usize,
    /// `smallHoleMask`: `result.excludedSupportFaces`, `null` in Draw.
    pub small_hole_mask: Option<Vec<u8>>,
    /// `tallRibs`: the ribs `braceTallWalls` built (`None` when it did not run).
    pub tall_ribs: Option<usize>,
    pub floating: Vec<FloatingPiece>,
    pub bare: Option<Bare>,
}

impl FinsBuilt {
    /// A `built` as `buildFinsCore`'s prop-mode return has it before any pass touches it, for a
    /// part that got no walls (`noProps()`, web/prop.js:99-105, through the return at
    /// web/fins.js:447-476): no triangles, `mode: 'prop'`, every counter 0, `rejected` and
    /// `patchStats` zeroed, `tip: null`, and none of the keys a later stage adds (`braceCount`,
    /// `fill`, `sway`, `tallRibs`, `bare`, ...). `seating` is the one value it cannot invent.
    pub fn empty(seating: Seating) -> FinsBuilt {
        FinsBuilt {
            triangles: Vec::new(),
            pad_triangles: Vec::new(),
            pad: None,
            mode: "prop".to_string(),
            fins: Vec::new(),
            props: Vec::new(),
            volume: 0.0,
            skipped: Skipped::default(),
            rejected: Rejected::default(),
            patch_count: 0,
            patch_stats: PatchStats::default(),
            tines: 0,
            served_regions: Vec::new(),
            unserved: 0.0,
            sag_risk: false,
            seating,
            tip: None,
            brace_count: None,
            prop_count: None,
            fill: None,
            curved_fill: None,
            curved_dropped: false,
            sway: None,
            small_hole_regions: 0,
            small_hole_mask: None,
            tall_ribs: None,
            floating: Vec::new(),
            bare: None,
        }
    }
}

// ==================================================================================================
// buildFins (web/fins.js)
// ==================================================================================================

use std::sync::Arc;

use indexmap::IndexSet;

use crate::cancel::{Cancel, Cancelled};
use crate::config::{Config, Tunables, apply_tunables};
use crate::fins::coverage::{Targets, automatic_targets_cancellable};
use crate::fins::downhill::{Slack, judge_downhill};
use crate::fins::fill::{FillOpts, bare_area, fill_coverage};
use crate::fins::pad::build_pad;
use crate::fins::seating::{SeatingKind, bed_contact, forget_seated, seated_part_tris, seating_of};
use crate::fins::shortwalls::{ShortWalls, last_resort_walls};
use crate::fins::tall::brace_tall_walls;
use crate::fins::wedges::{
    WedgeOpts, build_perp_fins, prop_serves_patch, unserved_after_wedges, wedge_veto,
};
use crate::islands::{Island, IslandOpts, find_islands};
use crate::overhangs::{Analysis, Topology};
use crate::pieces::floating_pieces_cancellable;
use crate::planes::find_wall_patches_cancellable;
use crate::prop::build::{BuildOpts, PropsPass, RasterVeto, build_props, no_props};
use crate::prop::config::Iface;
use crate::prop::crest::{Crest, crest_on};
use crate::prop::ctx::{BaseMemo, BuildCtx, TineSeed};
use crate::prop::curved::bends;
use crate::sway::{Avoid, SwayOpts, build_sway_braces};
use crate::types::Rot3;

/// `opts.sway` (the page's `swayOpts()` and the plugins'; web/fins.js:150-170): the braces'
/// request. `on` turns Auto's braces on; the other keys feed `buildSwayBraces`. `None` is the key
/// absent. `tines` and `layerHeight` of this object are overridden by the build's own (the JS
/// spreads `...opts.sway` first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwayRequest {
    pub on: bool,
    pub grip_from: Option<f64>,
    pub tine_spacing: Option<f64>,
    pub reach: Option<f64>,
    pub gap: Option<f64>,
    pub tine_contact_offset: Option<f64>,
    pub cross_above: Option<f64>,
    pub cross_rise: Option<f64>,
    pub uncapped_depth: bool,
    pub round_feet: Option<bool>,
    pub allow_stilt: Option<bool>,
    pub max_height: Option<f64>,
}

/// The `opts` of `buildFins` (web/fins.js:60-110 and every pass that reads one): each key as the
/// page's `finOpts()` and the plugins send it. `None` / `false` is the key absent.
#[derive(Clone, Default)]
pub struct FinsOpts {
    /// `mode`: `'prop'` (the default when absent), `'auto'`, `'stabilize'`, `'full'`, `'draw'`;
    /// anything else builds `'prop'`.
    pub mode: Option<String>,
    /// `bedPad` (`?? true`).
    pub bed_pad: Option<bool>,
    /// `tines` (`?? true` in Auto/Full; `=== true` in Prop).
    pub tines: Option<bool>,
    pub tine_density: Option<f64>,
    pub layer_height: Option<f64>,
    /// `coverage` (the slider, 0..1; `?? FIN.coverDefault`).
    pub coverage: Option<f64>,
    /// `plateOnly === true`.
    pub plate_only: bool,
    /// `longWalls` (only `=== false` turns the fill's rows off).
    pub long_walls: Option<bool>,
    /// `separateWalls === true`.
    pub separate_walls: bool,
    /// `tallRibs` (only `=== false` skips the ribs).
    pub tall_ribs: Option<bool>,
    /// `curvedWalls === true`.
    pub curved_walls: Option<bool>,
    /// `downhill`: unset judges both; true / false builds that one.
    pub downhill: Option<bool>,
    /// `raster === false` is the tests' normal-pass-alone switch.
    pub raster: Option<bool>,
    /// `lastResort === false` skips the short walls.
    pub last_resort: Option<bool>,
    /// `rimWalls === false`.
    pub rim_walls: Option<bool>,
    /// `tunables` (applied to the build's own `Config`, `applyTunables`).
    pub tunables: Option<Tunables>,
    pub sway: Option<SwayRequest>,
    /// `islands`: Full coverage's island starts when the page found them already (`findIslands`
    /// otherwise).
    pub islands: Option<Vec<Island>>,
    /// `fillRegions` (Auto's curved-walls fill; set by `buildFins` itself).
    pub fill_regions: Option<IndexSet<usize>>,
    /// `rasterVeto` (set by `buildFins` itself: the real `wedgeVeto`).
    pub raster_veto: Option<RasterVeto>,
}

impl std::fmt::Debug for FinsOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FinsOpts")
            .field("mode", &self.mode)
            .field("bed_pad", &self.bed_pad)
            .field("tines", &self.tines)
            .field("coverage", &self.coverage)
            .field("downhill", &self.downhill)
            .field("raster", &self.raster)
            .field("curved_walls", &self.curved_walls)
            .field("sway", &self.sway)
            .finish_non_exhaustive()
    }
}

impl FinsOpts {
    fn mode_str(&self) -> &str {
        self.mode.as_deref().unwrap_or("prop")
    }

    /// The options `buildProps` / `lastResortWalls` read off `opts` (the JS passes the whole
    /// object along): `tines === true`, `sway?.on`, ... `targets` carries
    /// `result.excludedSupportFaces`.
    fn prop_opts(&self, targets: Option<&Targets>) -> BuildOpts {
        BuildOpts {
            coverage: self.coverage,
            tines: self.tines == Some(true),
            tine_density: self.tine_density,
            layer_height: self.layer_height,
            raster: self.raster,
            raster_veto: self.raster_veto.clone(),
            sway_on: self.sway.as_ref().is_some_and(|s| s.on),
            rim_walls: self.rim_walls,
            downhill: self.downhill,
            curved_walls: self.curved_walls,
            plate_only: self.plate_only,
            excluded_support_faces: targets.map(|t| Arc::new(t.excluded_support_faces.clone())),
            ..Default::default()
        }
    }
}

/// Port of `coverPitch` (web/fins.js:57-62): wedge row pitch for a coverage setting, mirroring
/// prop.js's coverRowSpan: 0.5 is the neutral default (coverSparse, the pre-slider behaviour),
/// left of it loosens toward coverExtraSparse, right of it tightens toward coverDense. Kept
/// monotonic.
pub fn cover_pitch(coverage: f64, cfg: &Config) -> f64 {
    let fin = &cfg.fin;
    let c = jsnum::max(0.0, jsnum::min(1.0, coverage));
    if c <= 0.5 {
        fin.cover_sparse + ((0.5 - c) / 0.5) * (fin.cover_extra_sparse - fin.cover_sparse)
    } else {
        fin.cover_sparse - ((c - 0.5) / 0.5) * (fin.cover_sparse - fin.cover_dense)
    }
}

/// `result` of the passes: the analyze result, or `automaticTargets`' augmented copy of it.
#[derive(Clone, Copy)]
struct Res<'a> {
    a: &'a Analysis,
    /// `None` only in Draw mode, where `buildFins` keeps the plain result.
    t: Option<&'a Targets>,
}

impl<'a> Res<'a> {
    fn targets(&self) -> &'a Targets {
        self.t.expect("automatic targets (not Draw mode)")
    }
}

/// Port of `buildFins` (web/fins.js:79-110): generate supports for the part in its current
/// orientation. `topo` is an `Arc` because the raster veto outlives the call that makes it;
/// `cfg` is the caller's config: JS's `applyTunables` writes the module config, here it writes a
/// clone that the build owns (nothing persists after it).
///
/// Keeps the original analysis for rendering/export callers. Every automatic pass below shares
/// the targets view; Draw remains an explicit placement decision.
pub fn build_fins(
    topo: &Arc<Topology>,
    result: &Analysis,
    rot: &Rot3,
    opts: &FinsOpts,
    cfg: &Config,
) -> FinsBuilt {
    build_fins_inner(topo, result, rot, opts, cfg, false).0
}

/// [`build_fins`] that stops soon after `cancel`'s flag is raised and then returns `Err(Cancelled)`
/// (never a partial build; nothing of the abandoned build is kept). Unraised, the result is
/// byte-identical to [`build_fins`]'s.
pub fn build_fins_cancellable(
    topo: &Arc<Topology>,
    result: &Analysis,
    rot: &Rot3,
    opts: &FinsOpts,
    cfg: &Config,
    cancel: &Cancel,
) -> Result<FinsBuilt, Cancelled> {
    let mut cfg = cfg.clone();
    cfg.cancel = cancel.clone();
    let built = crate::cancel::catch(|| build_fins_inner(topo, result, rot, opts, &cfg, false).0);
    if built.is_err() {
        // the seated part is cached on the topology for one build only
        forget_seated(topo);
    }
    built
}

/// `buildFins` run the way the JS tests run it with `globalThis.__TINECAP = []` set around the
/// call (web/prop/tines.js, web/prop.js:141): the build's tine capture is seeded with an outer,
/// empty capture, and the seeds it holds afterwards come back with the build. A downhill judge
/// inside the build leaves the capture holding the returned build's tines only (fins/downhill.js).
pub fn build_fins_capturing(
    topo: &Arc<Topology>,
    result: &Analysis,
    rot: &Rot3,
    opts: &FinsOpts,
    cfg: &Config,
) -> (FinsBuilt, Vec<TineSeed>) {
    build_fins_inner(topo, result, rot, opts, cfg, true)
}

fn build_fins_inner(
    topo: &Arc<Topology>,
    result: &Analysis,
    rot: &Rot3,
    opts: &FinsOpts,
    cfg: &Config,
    capture: bool,
) -> (FinsBuilt, Vec<TineSeed>) {
    cfg.cancel.check();
    let targets = if opts.mode.as_deref() != Some("draw") {
        Some(automatic_targets_cancellable(
            topo,
            result,
            rot,
            &cfg.cancel,
        ))
    } else {
        None
    };
    let res = Res {
        a: targets.as_ref().map_or(result, |t| &t.analysis),
        t: targets.as_ref(),
    };
    let mut cfg = cfg.clone();
    apply_tunables(&mut cfg, opts.tunables.as_ref()); // buildFinsAndBraces
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    if capture {
        *ctx.tine_cap.borrow_mut() = Some(Vec::new());
    }
    cfg.cancel.check();
    let mut built = build_fins_and_braces(topo, res, rot, opts, &mut ctx);
    cfg.cancel.check();
    built.small_hole_regions = targets.as_ref().map_or(0, |t| t.small_hole_regions);
    // Face indices stay in the caller's input order. Reuse the generation's mask for
    // highlighting; rendering must not reclassify during a pose drag. Draw is an explicit
    // placement decision and has no automatic exclusions.
    built.small_hole_mask = if opts.mode.as_deref() == Some("draw") {
        None
    } else {
        targets.as_ref().map(|t| t.excluded_support_faces.clone())
    };
    // Tall walls get face ribs, last, around everything placed (fins/tall.js). Draw ribs its own
    // walls on the page (draw.js braceTall).
    if opts.mode.as_deref() != Some("draw") && opts.tall_ribs != Some(false) {
        brace_tall_walls(topo, res.a, rot, &mut built, &cfg);
    }
    // crestPart(null): the crest's part is only for this build (a Worker keeps no mesh)
    // forgetSeated(topo): nor the seated part: a page building inline would hold it
    forget_seated(topo);
    cfg.cancel.check();
    // A piece of the part that starts in mid-air (a cut clean through, a loose body) needs saying
    // no matter what was placed: see floatingPieces.
    built.floating = floating_pieces_cancellable(topo, res.a, rot, &cfg.cancel);
    // How much red the walls leave bare (fins/fill.js bareArea), so the readout can say it: Auto
    // measures its own walls; Full already knows its leftover, and only the total is added.
    // Draw's walls are placed on the page, so it has none. `coverage` rides along so the page can
    // re-measure after fins are removed by hand.
    cfg.cancel.check();
    let coverage = opts.coverage.unwrap_or(0.5);
    let fill_opts = fill_opts_of(opts, &[]);
    let mode = opts.mode.as_deref();
    if mode == Some("auto") || mode == Some("stabilize") {
        let (area, total) = bare_area(topo, res.targets(), rot, &fill_opts, &built.fins, &cfg);
        built.bare = Some(Bare {
            area,
            total,
            coverage,
        });
    } else if mode == Some("full")
        && let Some(fill) = &built.fill
    {
        let (_, total) = bare_area(topo, res.targets(), rot, &fill_opts, &[], &cfg);
        built.bare = Some(Bare {
            area: fill.unserved_area,
            total,
            coverage,
        });
    }
    let seeds = ctx.tine_cap.borrow_mut().take().unwrap_or_default();
    (built, seeds)
}

/// The `FillOpts` view of `opts` (fill.js reads `coverage`, `longWalls`, `fillRegions`, `islands`,
/// `plateOnly`, `separateWalls`, `tines`, `tineDensity`, `layerHeight`).
fn fill_opts_of<'a>(opts: &FinsOpts, islands: &'a [Island]) -> FillOpts<'a> {
    FillOpts {
        coverage: opts.coverage,
        long_walls: opts.long_walls,
        fill_regions: opts.fill_regions.clone(),
        islands,
        plate_only: opts.plate_only,
        separate_walls: opts.separate_walls,
        tines: opts.tines,
        tine_density: opts.tine_density,
        layer_height: opts.layer_height,
    }
}

/// Port of `buildFinsAndBraces` (web/fins.js:112-199). `applyTunables` has been applied to the
/// context's config by [`build_fins`].
fn build_fins_and_braces(
    topo: &Arc<Topology>,
    res: Res,
    rot: &Rot3,
    opts: &FinsOpts,
    ctx: &mut BuildCtx,
) -> FinsBuilt {
    let cfg = ctx.cfg;
    // the interface crest's 'flat' test reads the underside off the seated part
    ctx.crest = Crest::new(
        if crest_on(cfg) {
            Some(seated_part_tris(topo, rot, &res.a.offset))
        } else {
            None
        },
        opts.layer_height.unwrap_or(0.0),
        cfg,
    );
    let built = build_fins_core(topo, res, rot, opts, ctx);
    // Sway braces are an optional ADD-ON to whatever the mode placed (sway.js): a tall part still
    // needs its overhangs held, and bracing its sides is a separate job on separate faces.
    // Draw stands its braces by hand (ui/walls.js), so it never gets Auto's.
    let Some(sway) = opts.sway.as_ref().filter(|s| s.on) else {
        return built;
    };
    if opts.mode.as_deref() == Some("draw") {
        return built;
    }
    let mut built = built;
    // Braces run LAST, so everything this mode placed is already on the plate: hand the props' and
    // wedges' centrelines over as things to stand clear of. Fused to one of those, a brace is no
    // longer a piece that snaps off by itself.
    let walls: Vec<Vec<V3>> = built
        .fins
        .iter()
        .map(|f| f.line.clone())
        .filter(|l| !l.is_empty())
        .collect();
    let sw = build_sway_braces(
        topo,
        res.a,
        rot,
        &SwayOpts {
            round_feet: sway.round_feet.unwrap_or(cfg.fin.round_feet),
            tines: opts.tines,
            layer_height: opts.layer_height,
            gap: sway.gap,
            grip_from: sway.grip_from,
            tine_spacing: sway.tine_spacing,
            reach: sway.reach,
            iface_all: cfg.prop.iface == Iface::All,
            avoid: Avoid {
                braces: &[],
                walls: &walls,
            },
            tine_contact_offset: Some(
                sway.tine_contact_offset
                    .unwrap_or(cfg.prop.tine_contact_offset),
            ),
            // "Brace tall walls" cross-ribs a brace over its start height, exactly as it does the
            // drawn walls. The page sends the threshold through swayOpts (it is Infinity when the
            // box is unticked); a caller that says nothing -- the plugins, the golden scenes --
            // keeps the plain brace it always got.
            cross_above: if opts.tall_ribs == Some(false) {
                Some(f64::INFINITY)
            } else {
                Some(sway.cross_above.unwrap_or(f64::INFINITY))
            },
            cross_rise: sway.cross_rise,
            uncapped_depth: sway.uncapped_depth,
            allow_stilt: sway.allow_stilt,
            max_height: sway.max_height,
            ..Default::default()
        },
        cfg,
    );
    // Each brace also gets a fin record: the Auto view draws and exports only the triangles some
    // record claims (per-fin removal), so an unrecorded brace would be counted in the readout but
    // never shown or written out.
    let base = built.triangles.len();
    let mut id = built.fins.iter().fold(built.fins.len(), |m, f| {
        jsnum::max(m as f64, (f.id + 1) as f64) as usize
    });
    #[allow(clippy::explicit_counter_loop)] // `id++` as in JS
    for r in &sw.ribs {
        let tr = r.tri_range.expect("triRange (set by buildSwayBraces)");
        built.fins.push(FinRecord {
            height: r.height,
            length: r.depth,
            tines: r.tines,
            rows: 0,
            stilt: 0.0,
            lean: 0.0,
            bearing: 0.0,
            site: None,
            id,
            kind: FinKind::Sway,
            tri_ranges: vec![[tr[0] + base, tr[1] + base]],
            line: r.foot.to_vec(),
            span: r.depth,
            short: false,
            fill: false,
            joined: false,
            braces: None,
            ribs: None,
        });
        id += 1;
    }
    built.triangles.extend(sw.triangles.iter().copied());
    built.sway = Some(SwayInfo {
        count: sw.count,
        tines: sw.tines,
        skipped: sw.skipped,
        reason: sw.reason.clone(),
        // Cross ribs ("Brace tall walls") built on the braces, for the readout
        cross: sw.ribs.iter().fold(0, |a, r| a + r.cross),
        // The outlines travel back so a brace the user then clicks by hand can be checked against
        // these: Auto builds in the Worker, so the page has no other way to know where they stand.
        // Plain data, structured-cloneable.
        braces: sw
            .ribs
            .iter()
            .map(|r| SwayBrace {
                foot: r.foot,
                half_w: r.half_w,
                th: r.th,
                height: r.height,
                levels: r.levels.clone(),
                arms: r.arms.clone(),
            })
            .collect(),
    });
    built
}

use crate::jsnum;

/// A [`FinRecord`] as the prop-mode return builds it for each wall (web/fins.js:447-453).
fn fin_of_prop(q: &PropRecord) -> FinRecord {
    FinRecord {
        height: q.height,
        length: q.span,
        tines: 0,
        rows: 0,
        stilt: 0.0,
        lean: 0.0,
        bearing: 0.0,
        site: None,
        id: q.id,
        kind: FinKind::Prop,
        tri_ranges: q.tri_ranges.clone(),
        line: q.line.clone(),
        span: q.span,
        short: false,
        fill: false,
        joined: false,
        braces: None,
        ribs: None,
    }
}

/// Port of `withFill` (web/fins.js:208-271): Auto's result plus the fill pass's walls
/// (fins/fill.js), merged the way the short walls are: triangles appended, props and fins
/// carrying ranges into them. Its targets include the island starts (islands.js): `opts.islands`
/// when the page found them already (the worker's topo has no vertex ids), else found here.
fn with_fill(
    topo: &Arc<Topology>,
    res: Res,
    rot: &Rot3,
    opts: &FinsOpts,
    auto: FinsBuilt,
    ctx: &mut BuildCtx,
) -> FinsBuilt {
    let cfg = ctx.cfg;
    let islands: Vec<Island> = if opts.fill_regions.is_some() {
        Vec::new()
    } else {
        match &opts.islands {
            Some(i) => i.clone(),
            None => find_islands(topo, res.a, rot, &IslandOpts::default(), cfg),
        }
    };
    let f = fill_coverage(
        topo,
        res.targets(),
        rot,
        &fill_opts_of(opts, &islands),
        &auto,
        ctx,
    );
    let at = auto.triangles.len();
    let mut wid = auto.fins.len();
    let mut props: Vec<PropRecord> = Vec::new();
    let mut fins: Vec<FinRecord> = Vec::new();
    #[allow(clippy::explicit_counter_loop)] // `wid++` as in JS
    for q in &f.props {
        let tri_ranges: Vec<[usize; 2]> =
            q.tri_ranges.iter().map(|[a, b]| [a + at, b + at]).collect();
        // (a fill wall has no region / id of its own in JS; the fin's id stands in for the id)
        props.push(PropRecord {
            tines: q.tines,
            span: q.span,
            height: q.height,
            line: q.line.clone(),
            id: wid,
            tri_ranges: tri_ranges.clone(),
            part_attached: q.part_attached,
            squat: q.squat,
            kind: "prop",
            fill: true,
            ..Default::default()
        });
        fins.push(FinRecord {
            height: q.height,
            length: q.span,
            tines: q.tines,
            rows: 0,
            stilt: 0.0,
            lean: 0.0,
            bearing: 0.0,
            site: None,
            id: wid,
            kind: FinKind::Prop,
            tri_ranges,
            line: q.line.clone(),
            span: q.span,
            short: false,
            fill: true,
            joined: q.joined,
            braces: q.braces.filter(|&b| b != 0),
            ribs: None,
        });
        wid += 1;
    }
    let with_tines = opts.tines != Some(false);
    let n_fins = fins.len();
    let mut out = auto;
    out.triangles.extend(f.triangles.iter().copied());
    out.props.extend(props);
    out.fins.extend(fins);
    out.tines += f.tines;
    out.brace_count = Some(out.brace_count.unwrap_or(0) + if with_tines { n_fins } else { 0 });
    out.prop_count = Some(out.prop_count.unwrap_or(0) + if with_tines { 0 } else { n_fins });
    out.fill = Some(FillStats {
        walls: f.stats.walls,
        rows: f.stats.rows.unwrap_or(0),
        rows_lost: f.stats.rows_lost.unwrap_or(0.0),
        row_builds: f.stats.row_builds.unwrap_or(0),
        joined: f.stats.joined.unwrap_or(0),
        ribs: f.stats.ribs,
        ties: f.stats.ties,
        tries: f.stats.tries,
        checks: f.stats.checks,
        capped: f.stats.capped,
        refused: f.stats.refused.clone(),
        bare_before: f.stats.start_area,
        unserved_area: f.unserved.area,
        unserved_pts: f.unserved.pts.clone(),
        islands: islands.len(),
        islands_bare: f.unserved.islands,
        island_pts: f.unserved.island_pts.clone(),
        islands_capped: f.stats.islands_capped.unwrap_or(false),
    });
    out
}

/// Port of `buildFinsCore` (web/fins.js:273-476): the modes. `Auto`/`Full` build the tined walls
/// plus wedges plus the last-resort short walls (and Full the fill); anything else is the prop
/// build.
fn build_fins_core(
    topo: &Arc<Topology>,
    res: Res,
    rot: &Rot3,
    opts: &FinsOpts,
    ctx: &mut BuildCtx,
) -> FinsBuilt {
    let cfg = ctx.cfg;
    let mode = opts.mode_str().to_string();
    let mut pad_out: Vec<V4> = Vec::new();

    // THE COMBINED SUPPORT IS A TINED PERPENDICULAR RIB (prop.js), not the old beside-the-face
    // fin that lay FLAT against the part. A support fin has to stand PERPENDICULAR to the part --
    // a wall UNDER the overhang, rising off the plate as an upside-down T, gripped along its top
    // by a comb of tines -- the way a human draws one and the way it actually prints. The earlier
    // geometry leaned the wall parallel to the face (planes.js "the fin leans with the part"),
    // which put a 35deg-raked blade flat on a 35deg-tilted cube: wrong, and the reason this was
    // rebuilt.
    //
    // So both the grip mode ('stabilize') and the recommendation ('auto') now deliver prop.js's
    // rib with the tine comb ON. prop.js already sites these correctly: one wall down a curved
    // tube's lowest line, rows across a wide flat overhang, part-attached where a floor sits
    // under the overhang -- all watertight and weld-certified. Tines default ON here (that is what
    // makes it "combined" rather than a plain breakaway prop); 'prop' proper leaves them off. The
    // old leaning-fin machinery (rankSites / chooseSpan / buildFin) was deleted once nothing
    // reached it; git history has it.
    if mode == "auto" || mode == "stabilize" || mode == "full" {
        // Downhill walls (issue #25) stay only if the whole build is no worse for them: see
        // fins/downhill.js. Unset = judge both; true / false = build that one. (`raster: false`
        // is the tests' normal-pass-alone switch: no races at all.)
        if opts.downhill.is_none() {
            let with = |o: &FinsOpts| -> FinsOpts { o.clone() };
            if opts.raster == Some(false) {
                let mut o = with(opts);
                o.downhill = Some(false);
                return build_fins_core(topo, res, rot, &o, ctx);
            }
            let cls = res.t.map(|t| t.coverage_classes.as_slice());
            // Curved walls (prop/curved.js): ticked, the curve is forced -- no judge in Auto,
            // where its own fill takes what the curves leave. In Full the curves can break up the
            // fill's rows (coffee bean on its side: held -5%), so the unticked Full build is built
            // too and the curved one stays only if it holds within CURVED.fullSlack of it, grips
            // no higher and keeps every wedge (fins/downhill.js); the readout says when the
            // curves were dropped.
            if opts.curved_walls == Some(true) {
                let curved = |ctx: &mut BuildCtx| {
                    let mut o = with(opts);
                    o.downhill = Some(true);
                    build_fins_core(topo, res, rot, &o, ctx)
                };
                if mode != "full" {
                    return curved(ctx);
                }
                let full_slack = cfg.curved.full_slack;
                return judge_downhill(
                    topo,
                    res.a,
                    rot,
                    |on, ctx| {
                        if on {
                            curved(ctx)
                        } else {
                            let mut o = with(opts);
                            o.curved_walls = None;
                            let mut b = build_fins_core(topo, res, rot, &o, ctx);
                            b.curved_dropped = true;
                            b
                        }
                    },
                    // the slack is for a curve: a ticked build whose downhill walls all run
                    // straight is judged as the downhill walls are, strictly
                    Slack::By(&|b: &FinsBuilt| {
                        if bends(
                            b.props
                                .iter()
                                .filter(|q| q.downhill == Some(true))
                                .map(|q| q.line.as_slice()),
                            cfg,
                        ) {
                            full_slack
                        } else {
                            0.0
                        }
                    }),
                    cls,
                    ctx,
                );
            }
            // The two builds share everything `build_props` does before the downhill race: the
            // first records it, the second replays it (prop/ctx.rs `BaseMemo`).
            let mut memo: Option<(PropsPass, Option<Vec<TineSeed>>)> = None;
            let judged = judge_downhill(
                topo,
                res.a,
                rot,
                |downhill, ctx| {
                    let mut o = with(opts);
                    o.downhill = Some(downhill);
                    *ctx.base_memo.borrow_mut() = match (downhill, memo.take()) {
                        (true, _) => BaseMemo::Record(None),
                        (false, Some((base, cap))) => BaseMemo::Replay(base, cap),
                        (false, None) => BaseMemo::Off,
                    };
                    let built = build_fins_core(topo, res, rot, &o, ctx);
                    if let BaseMemo::Record(Some(m)) = ctx.base_memo.take() {
                        memo = Some(m);
                    }
                    built
                },
                Slack::Fixed(0.0),
                cls,
                ctx,
            );
            *ctx.base_memo.borrow_mut() = BaseMemo::Off;
            return judged;
        }
        cfg.cancel.check();
        let with_tines = opts.tines.unwrap_or(true);
        // Wide-face coverage (0 sparse .. 1 dense) drives how densely a broad face is lined --
        // the prop rows (buildProps reads opts.coverage, forwarded below) and the wedge row pitch
        // here. Denser only ADDS supports and never loosens past the structural cap, so dragging
        // it right can't strand an overhang. Pinned by tests/coverage.test.js.
        let coverage = jsnum::max(
            0.0,
            jsnum::min(1.0, opts.coverage.unwrap_or(cfg.fin.cover_default)),
        );
        let cov_pitch = cover_pitch(coverage, cfg);
        let offset = res.a.offset;
        let t = res.targets();
        // The wedge candidates: broad down-facing patches. Found before the props so a raster
        // wall can be kept off the ones the normal pass leaves to a wedge (wedgeVeto).
        let wedgeable: Vec<_> = find_wall_patches_cancellable(
            topo,
            rot,
            &offset,
            None,
            Some(&t.excluded_support_faces),
            &cfg.cancel,
        )
        .into_iter()
        .filter(|p| {
            p.n.z < -0.05 && p.area >= cfg.perp.min_area && (p.u1 - p.u0) >= cfg.perp.min_width
        })
        .collect();
        cfg.cancel.check();
        let wedge_opts = WedgeOpts {
            pitch: Some(cov_pitch),
            tines: Some(with_tines),
            tine_density: opts.tine_density,
            layer_height: opts.layer_height,
        };
        let base = {
            let mut o = opts.clone();
            o.mode = Some("prop".to_string());
            o.tines = Some(with_tines);
            o.raster_veto = Some(wedge_veto(
                wedgeable.clone(),
                Arc::clone(topo),
                *rot,
                offset,
                wedge_opts.clone(),
                cfg.clone(),
                ctx.crest.clone(),
            ));
            build_fins_core(topo, res, rot, &o, ctx)
        };

        // Add ANGLED WEDGES on grippable down-facing patches that NO prop wall reached -- the
        // wide/long leaning face where a vertical wall is blocked by the part itself. "Reached"
        // is decided in SPACE (a prop line under the patch's footprint), not by face-index,
        // because a wall-patch and an overhang region grow from different seeds and don't share a
        // face set. Patches props already serve are left untouched, so the reachable parts don't
        // change.
        let mut wedge_tris: Vec<V4> = Vec::new();
        let mut wedge_recs: Vec<crate::fins::wedges::WedgeRec> = Vec::new(); // per-wedge records, triRange into wedgeTris (pre-offset)
        let mut wedge_tines = 0usize;
        let mut wedged_patches = 0usize;
        for p in &wedgeable {
            cfg.cancel.check();
            // downward, broad faces only
            if prop_serves_patch(p, &base.props, cfg) {
                continue; // a prop already stands under it
            }
            let w = build_perp_fins(p, topo, rot, &offset, &wedge_opts, ctx);
            if w.count == 0 {
                continue;
            }
            // Offset each wedge's range from its per-call `out` into the merged wedgeTris array,
            // so the range lands correctly in the final triangles.
            let off = wedge_tris.len();
            wedge_tris.extend(w.triangles.iter().copied());
            for wd in &w.wedges {
                wedge_recs.push(crate::fins::wedges::WedgeRec {
                    tri_range: [wd.tri_range[0] + off, wd.tri_range[1] + off],
                    line: wd.line.clone(),
                    height: wd.height,
                    span: wd.span,
                });
            }
            wedge_tines += w.tines;
            wedged_patches += 1;
        }
        let wedge_count = wedge_recs.len();

        // LAST RESORT: short walls under the regions no wall or wedge reached (issue #121's
        // strut lattice). Only bare regions, so a served part is unchanged. `lastResort: false`
        // skips it, like `raster: false` (the tests' baselines). A point-seated part with the pad
        // off gets no walls (the prop-mode gate below); the last resort must not build them back.
        cfg.cancel.check();
        let point_no_pad = base.seating.kind == SeatingKind::Point && base.pad.is_none();
        let no_short = opts.last_resort == Some(false) || point_no_pad;
        let short = if no_short {
            ShortWalls::default()
        } else {
            let mut so = opts.prop_opts(res.t);
            so.tines = with_tines;
            so.coverage = Some(coverage);
            let mut existing: Vec<V4> = base.triangles.clone();
            existing.extend(wedge_tris.iter().copied());
            last_resort_walls(
                topo,
                res.a,
                rot,
                &so,
                &base.served_regions,
                &wedge_tris,
                &existing,
                ctx,
            )
        };

        // Unified per-fin array: each entry carries its triangle segment(s) into the final
        // `built.triangles`, so the UI can address and remove an individual fin by its geometry.
        // Prop ranges already index `base.triangles` (the prefix of the merged array); wedge
        // ranges are offset past it by base.triangles.length. base.fins[i] is order-aligned with
        // base.props[i] (it is built from it), so the existing stats fields carry over and only
        // triRanges/id/kind are added.
        let base_len = base.triangles.len();
        let mut fins: Vec<FinRecord> = base.fins.clone();
        let mut wid = fins.len();
        for wd in &wedge_recs {
            fins.push(FinRecord {
                height: wd.height,
                length: wd.span,
                tines: 0,
                rows: 0,
                stilt: 0.0,
                lean: 0.0,
                bearing: 0.0,
                site: None,
                id: wid,
                kind: FinKind::Wedge,
                tri_ranges: vec![[wd.tri_range[0] + base_len, wd.tri_range[1] + base_len]],
                line: wd.line.clone(),
                span: wd.span,
                short: false,
                fill: false,
                joined: false,
                braces: None,
                ribs: None,
            });
            wid += 1;
        }
        let short_at = base_len + wedge_tris.len();
        // Into the merged triangles, for fins AND props (prototype/examples/compare.js reads
        // props' ranges against result.triangles).
        let short_props: Vec<PropRecord> = short
            .props
            .iter()
            .map(|q| {
                let mut q = q.clone();
                q.tri_ranges = q
                    .tri_ranges
                    .iter()
                    .map(|[a, b]| [a + short_at, b + short_at])
                    .collect();
                q
            })
            .collect();
        for q in &short_props {
            fins.push(FinRecord {
                height: q.height,
                length: q.span,
                tines: q.tines,
                rows: 0,
                stilt: 0.0,
                lean: 0.0,
                bearing: 0.0,
                site: None,
                id: wid,
                kind: FinKind::Prop,
                tri_ranges: q.tri_ranges.clone(),
                line: q.line.clone(),
                span: q.span,
                short: true,
                fill: false,
                joined: false,
                braces: None,
                ribs: None,
            });
            wid += 1;
        }
        let mut served_regions = base.served_regions.clone();
        served_regions.extend(short.served.iter().copied());
        let n_base_fins = base.fins.len();
        let n_fins = fins.len();
        let mut auto = base;
        auto.mode = mode.clone();
        auto.triangles.extend(wedge_tris.iter().copied());
        auto.triangles.extend(short.triangles.iter().copied());
        auto.props.extend(short_props);
        auto.fins = fins;
        auto.tines += wedge_tines + short.tines;
        // A tined rib/wedge IS the combined support (a "brace"); a tineless one is a plain prop.
        // Report the split so the stress harness / UI metrics keep working.
        auto.brace_count = Some(if with_tines { n_fins } else { wedge_count });
        auto.prop_count = Some(if with_tines {
            0
        } else {
            n_base_fins + short.props.len()
        });
        auto.unserved = if wedged_patches > 0 {
            let served: IndexSet<usize> = served_regions.iter().copied().collect();
            unserved_after_wedges(topo, rot, res.a, &served, &wedge_tris, cfg) as f64
        } else {
            auto.unserved - short.served.len() as f64
        };
        auto.served_regions = served_regions;

        // FULL COVERAGE (fins/fill.js): Auto, then walls under the red it left bare. Not on a
        // point-seated part with the pad off, which gets no walls at all (above).
        // (`lastResort: false` is the tests' Auto baseline switch; it doesn't turn the fill off)
        let no_walls = point_no_pad;
        if mode == "full" && !no_walls {
            let mut o = opts.clone();
            o.tines = Some(with_tines);
            o.coverage = Some(coverage);
            return with_fill(topo, res, rot, &o, auto, ctx);
        }
        // Curved walls in Auto (prop/curved.js): a forced curve can leave overhang the straight
        // walls held, so the fill pass runs under the regions a curve won -- there only, rows
        // off. Its numbers are its own (`curvedFill`): the readout's Full coverage lines are
        // Full's, but the red it couldn't reach is said all the same. (a curve traced across a
        // group's slivers counts for every region in it: prop/trace.js)
        let mut curved: IndexSet<usize> = IndexSet::new();
        for q in &auto.props {
            if q.downhill == Some(true) && bends([q.line.as_slice()], cfg) {
                match &q.members {
                    Some(m) => curved.extend(m.iter().copied()),
                    None => {
                        curved.extend(q.region);
                    }
                }
            }
        }
        if mode == "auto" && opts.curved_walls == Some(true) && !curved.is_empty() && !no_walls {
            let mut o = opts.clone();
            o.tines = Some(with_tines);
            o.coverage = Some(coverage);
            o.long_walls = Some(false);
            o.fill_regions = Some(curved);
            let mut f = with_fill(topo, res, rot, &o, auto, ctx);
            let fl = f.fill.take().expect("the fill pass's numbers");
            f.curved_fill = Some(CurvedFill {
                walls: fl.walls,
                unserved_area: fl.unserved_area,
                capped: fl.capped,
            });
            return f;
        }
        return auto;
    }

    // Prop (and any other mode) is its own support, built by its own module -- a wall UNDER each
    // overhang with no tines.
    cfg.cancel.check();
    let contact = bed_contact(topo, res.a, rot);
    cfg.cancel.check();
    let seating = seating_of(res.a, &contact.pts, cfg);
    cfg.cancel.check();
    let pad = if opts.bed_pad.unwrap_or(true) && res.a.bed_area < cfg.fin.pad_min_area {
        build_pad(
            &contact.pts,
            &seated_part_tris(topo, rot, &res.a.offset),
            &mut pad_out,
            opts.layer_height.unwrap_or(cfg.fin.tine_h),
            cfg,
        )
    } else {
        None
    };
    // A part seated on a POINT gets no props -- nothing standing on the plate can hold a part that
    // never touches it -- UNLESS the bed pad is on, in which case the pad is what seats it and the
    // walls have something to work against. That is not speculation, it is the shelter workflow
    // this whole tool descends from: hub.py's apex hub is a SPHERE-bottomed part with 0.0 mm2 of
    // bed contact, and it printed cleanly as core pad + two webs (tools/shelter/hub.py
    // --supports). The first version of this gate refused it -- the flagship real-world part, the
    // one breakaway.py was written for -- while the readout said "rotate", which is exactly the
    // advice the printed evidence contradicts. Refuse only when the user has turned the pad off.
    // Draw builds no walls here: it exports only the pad and hand-placed walls, so a full prop
    // pass would be thrown away (1 s+ on a big part).
    cfg.cancel.check();
    let built: PropsPass =
        if mode == "draw" || (seating.kind == SeatingKind::Point && pad.is_none()) {
            no_props()
        } else {
            build_props(topo, res.a, rot, &opts.prop_opts(res.t), ctx)
        };
    FinsBuilt {
        triangles: built.triangles.clone(),
        pad_triangles: pad_out,
        pad,
        mode,
        // Carry each prop's triangle range + identity up so per-fin removal can address an
        // individual fin regardless of which mode produced it. The stats fields
        // (height/length/tines/...) are preserved; triRanges/id/kind/line are additive on top of
        // built.props (which already carry them).
        fins: built.props.iter().map(fin_of_prop).collect(),
        props: built.props.clone(),
        volume: built.volume,
        // Prop's own reasons, unflattened. These USED to be squashed into the stabilize-shaped
        // object below, which keeps only `blocked` -- so noLine, notALine, stub, degenerate and
        // buried were dropped before anything could read them. M5 was then measured through that
        // channel and recorded as working on parts where it built nothing: hub_post_foot reported
        // `blocked: 0` at 0 degrees when the real reason was `buried: 1`. Whatever explains a
        // failure has to survive the trip to the UI.
        skipped: built.skipped,
        rejected: Rejected {
            blocked: built.skipped.blocked,
            too_few_tines: 0,
            sites: res.a.regions.len(),
            tried: res.a.regions.len(),
        },
        patch_count: 0,
        patch_stats: Default::default(),
        tines: built.tines,
        served_regions: built.served_regions.clone(),
        // regions minus SERVED REGIONS, not minus the prop count: a region can yield several walls
        // now that it is split into sub-patches, and the old subtraction would go negative.
        unserved: res.a.regions.len() as f64 - built.served as f64,
        sag_risk: built.sag_risk,
        seating,
        tip: None,
        brace_count: None,
        prop_count: None,
        fill: None,
        curved_fill: None,
        curved_dropped: false,
        sway: None,
        small_hole_regions: 0,
        small_hole_mask: None,
        tall_ribs: None,
        floating: Vec::new(),
        bare: None,
    }
}
