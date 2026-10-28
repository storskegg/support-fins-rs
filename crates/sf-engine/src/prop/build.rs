//! Port of `web/prop.js` (`buildProps`, `buildShortWalls`, the pass machinery). Read `docs/AGENTS.md` before changing anything here.
//!
//! This file holds the pass machinery's types (the `buildPass` result, the prop records, the
//! build options, the patch queue) and `noProps` / `coverRowSpan`. `buildPass`, `buildProps`
//! and `buildShortWalls` land last in M2, once every leaf module has merged.

use std::rc::Rc;
use std::sync::Arc;

use indexmap::{IndexMap, IndexSet};

use crate::config::Config;
use crate::jsnum;
use crate::prop::circular::CircularFrame;
use crate::tris::Tris;
use crate::types::V4;

/// `COVER_SPARSE_SPAN` (web/prop.js:112): mm row pitch at the sparsest coverage (0). This is
/// DELIBERATELY wider than maxUnsupportedSpan: the mid-slider (0.5) is the structural anti-sag
/// cap, and dragging left of it trades sag safety for fewer supports -- the tool warns when a
/// part actually lands a row wider than the cap (buildProps.sagRisk). Matthew asked for this: a
/// small part was floored at 3 fins by the hard cap.
pub const COVER_SPARSE_SPAN: f64 = 30.0;

/// Port of `coverRowSpan` (web/prop.js:121-127): wide-face row pitch for a coverage setting. 0.5
/// is the neutral default and maps to the structural cap (maxUnsupportedSpan) -- the old sparse
/// behaviour, so a part built at the slider's default is byte-identical to before. Left of
/// centre loosens past the cap toward COVER_SPARSE_SPAN (fewer supports, may sag); right of
/// centre tightens to half the cap (denser). Monotonic, so tests/coverage.test.js holds.
pub fn cover_row_span(coverage: f64, cfg: &Config) -> f64 {
    let c = jsnum::max(0.0, jsnum::min(1.0, coverage));
    let cap = cfg.prop.max_unsupported_span;
    if c <= 0.5 {
        cap + ((0.5 - c) / 0.5) * (COVER_SPARSE_SPAN - cap) // 30 .. 12
    } else {
        cap - ((c - 0.5) / 0.5) * (cap / 2.0) // 12 .. 6
    }
}

/// The `skipped` tally of a pass (web/prop.js:99-104, 249-250): why a region or line built no
/// wall. Keys are the JS ones, in the JS order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Skipped {
    pub no_line: usize,
    pub wanders: usize,
    pub stub: usize,
    pub blocked: usize,
    pub degenerate: usize,
    pub buried: usize,
    pub weld: usize,
    pub sliver: usize,
    pub on_part: usize,
}

impl Skipped {
    /// The JS keys, in object order.
    pub const KEYS: [&'static str; 9] = [
        "noLine",
        "wanders",
        "stub",
        "blocked",
        "degenerate",
        "buried",
        "weld",
        "sliver",
        "onPart",
    ];

    /// `skipped[key]`; panics on a key the object does not have.
    pub fn get(&self, key: &str) -> usize {
        *self.slot(key)
    }

    /// `skipped[key]++` (the key is `pa.floored`, one of [`Self::KEYS`]).
    pub fn bump(&mut self, key: &str) {
        *self.slot_mut(key) += 1;
    }

    fn slot<'a>(&'a self, key: &str) -> &'a usize {
        match key {
            "noLine" => &self.no_line,
            "wanders" => &self.wanders,
            "stub" => &self.stub,
            "blocked" => &self.blocked,
            "degenerate" => &self.degenerate,
            "buried" => &self.buried,
            "weld" => &self.weld,
            "sliver" => &self.sliver,
            "onPart" => &self.on_part,
            _ => panic!("skipped has no key {key:?}"),
        }
    }

    /// `for (const k in skipped) skipped[k] += end[k] - mark[k]` (web/prop/rival.js:55).
    pub fn add_diff(&mut self, end: &Skipped, mark: &Skipped) {
        for k in Self::KEYS {
            let d = end.get(k) as isize - mark.get(k) as isize;
            let s = self.slot_mut(k);
            *s = (*s as isize + d) as usize;
        }
    }

    fn slot_mut(&mut self, key: &str) -> &mut usize {
        match key {
            "noLine" => &mut self.no_line,
            "wanders" => &mut self.wanders,
            "stub" => &mut self.stub,
            "blocked" => &mut self.blocked,
            "degenerate" => &mut self.degenerate,
            "buried" => &mut self.buried,
            "weld" => &mut self.weld,
            "sliver" => &mut self.sliver,
            "onPart" => &mut self.on_part,
            _ => panic!("skipped has no key {key:?}"),
        }
    }
}

/// One wall's record, the union of every key any `props.push` writes (web/prop.js:514, 556,
/// 675, and the `pa.prop` / `sq` spreads from attached.js and squat.js, and raster.js's
/// `take`). Keys a given wall does not have are `None`/`false`; the comment on each says which
/// push writes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropRecord {
    /// `region` (all pushes); `None` on a fill-pass wall, which has no region in JS (#23).
    pub region: Option<usize>,
    /// `tines` (all pushes): tines emitted for this wall.
    pub tines: usize,
    /// `caps`: the wall's slice of the tests' tine capture, `undefined` when it is off.
    pub caps: Option<[usize; 2]>,
    /// `grip`: the lowest tine bottom (`Infinity`: no tines); `None` is the key absent
    /// (`q.grip ?? Infinity`).
    pub grip: Option<f64>,
    /// `span` (plate walls, `pa.prop`, `sq`).
    pub span: f64,
    /// `height` (plate walls, `pa.prop`, `sq`).
    pub height: f64,
    /// `area` (prop.js:514, 556, 675).
    pub area: f64,
    /// `stations` (plate walls, `pa.prop`, `sq`).
    pub stations: usize,
    /// `trimmed` (prop.js:514, 675).
    pub trimmed: Option<usize>,
    /// `volume` (plate walls, `pa.prop`, `sq`).
    pub volume: f64,
    /// `line`: the wall's centreline (surface minus gap).
    pub line: Vec<[f64; 3]>,
    /// `id`: sequential per build (raster.js `take` renumbers).
    pub id: usize,
    /// `kind`: always `'prop'`.
    pub kind: &'static str,
    /// `triRanges`: `[from, to)` ranges of the pass's `triangles`.
    pub tri_ranges: Vec<[usize; 2]>,
    /// `triRange`: a squat wall's own range (squat.js, spread in by `...sq`).
    pub tri_range: Option<[usize; 2]>,
    /// `wallRange`: the wall without its tines (downhill passes).
    pub wall_range: Option<[usize; 2]>,
    /// `members`: a traced group's regions.
    pub members: Option<Vec<usize>>,
    /// `rimWall: true`.
    pub rim_wall: bool,
    /// `partAttached: true` (attached.js).
    pub part_attached: bool,
    /// `inBore` (attached.js; `Some` exactly where `pa.prop` has the key).
    pub in_bore: Option<bool>,
    /// `squat: true` (squat.js).
    pub squat: bool,
    /// The race tags raster.js `take` writes (`[tag]: build === b`): `Some(true)` for the second
    /// pass's walls, `Some(false)` for the normal pass's.
    pub raster: Option<bool>,
    pub circular: Option<bool>,
    pub rim: Option<bool>,
    pub downhill: Option<bool>,
    pub curved: Option<bool>,
    /// `fill: true`: a wall of the fill pass (web/fins.js `withFill`). JS gives it only `tines`, `span`,
    /// `height`, `line`, `triRanges`, `partAttached` and `squat` of the keys above; the rest is
    /// this record's default.
    pub fill: bool,
}

/// What `buildPass` / `buildProps` return (web/prop.js:707-712, `noProps` at 99-104): the support
/// soup and the walls in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropsPass {
    /// `triangles`: three vertices per triangle (`[x, y, z, tag]`, see [`V4`]).
    pub triangles: Vec<V4>,
    pub props: Vec<PropRecord>,
    pub skipped: Skipped,
    /// `served`: how many REGIONS have at least one wall.
    pub served: usize,
    /// `servedRegions` (absent from `noProps()`; read as `?? []`).
    pub served_regions: Vec<usize>,
    /// `tines` (absent from `noProps()`).
    pub tines: usize,
    /// `sagRisk` (absent from `noProps()`; read as `?? false`).
    pub sag_risk: bool,
    /// `sagRegions`: only a raster pass has it.
    pub sag_regions: Option<IndexSet<usize>>,
    /// `leaderOf`: only a traced (curved) pass has it. Region -> its group's leader.
    pub leader_of: Option<IndexMap<usize, usize>>,
    pub volume: f64,
}

/// Port of `noProps` (web/prop.js:99-105): what `buildProps` returns when it builds nothing,
/// kept here so the callers that refuse to build (a part seated on a point) return the same
/// shape.
pub fn no_props() -> PropsPass {
    PropsPass::default()
}

/// A track: the stations of one wall's centreline, with the properties JS hangs on the array
/// (`line.rim`, `piece.from`). `buildPass`'s internal shape: built from `tracks::Tracks` /
/// `rim::rim_tracks` output at its call sites, where JS's tag loss on array copies is reproduced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub pts: Vec<[f64; 3]>,
    /// `line.rim = true` (prop/rim.js): a rim wall.
    pub rim: bool,
    /// `piece.from` (raster.js `rasterRest`): the index its first station had in the line it was
    /// cut from.
    pub from: Option<usize>,
}

/// A set of tracks (`buildPass`'s internal shape, built from `tracks::Tracks`): the `lines` array with the properties JS hangs on it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lines {
    pub lines: Vec<Line>,
    /// `lines.spacing`: the widest gap between two rows (0 for one row); `undefined` is `None`.
    pub spacing: Option<f64>,
    /// `lines.turned`: the old axis, when the walls were turned across the bridge (raced:
    /// prop/bridgerace.js).
    pub turned: Option<[f64; 2]>,
}

/// One unit of placement (`buildPass`'s internal shape, built from `tracks::Region` and the other
/// producers' output), an element of `buildPass`'s `patches` queue (web/prop.js:301-409): a
/// region (tube, arc, raster, downhill), a locally straight sub-patch of one (`splitRegion`), a
/// lattice strut, or a twin queued by a race.
#[derive(Clone, Debug, Default)]
pub struct Patch {
    pub faces: Vec<usize>,
    pub area: f64,
    pub region: usize,
    /// The REGION's seated triangles (flat, 9 per face), shared by all its patches. A `Tris`, so the
    /// surface grid cached on the array (`surface_z_at`) is shared too.
    pub tris: Arc<Tris>,
    /// `patch.lines`: the tube's, arc's, raster's or downhill's tracks (a patch-path patch has
    /// none: `patchTracks` makes its tracks).
    pub lines: Option<Lines>,
    /// `patch.smallTube`: the region's split patches, when this is a small tube (or an arc,
    /// raced against the split).
    pub small_tube: Option<Vec<Patch>>,
    pub arc: bool,
    /// `patch.axis`: a twin's (or a split patch's) track direction.
    pub axis: Option<[f64; 2]>,
    /// `patch.members`: a traced group's regions.
    pub members: Option<Vec<usize>>,
    /// `patch.strut`: a lattice strut (takes one wall, as it would standing alone).
    pub strut: bool,
    /// `patch.reach`: a lattice strut's own triangles, instead of `tris`, for the rival's
    /// held-area bounds.
    pub reach: Option<Arc<Tris>>,
    /// `patch.bridgeTwin`.
    pub bridge_twin: bool,
    /// `patch.rival`: an index into the rivalry's arena (prop/rival.js).
    pub rival: Option<usize>,
    /// `patch === rival.last`: this patch is the last of its rival's queued patches.
    pub rival_last: bool,
}

/// The patch loop's accumulators (web/prop.js:242-256, 411-418): `{ out, props, skipped, served,
/// tally }`, which `tubeRivalry` and `bridgeRace` snapshot and undo.
#[derive(Clone, Debug, Default)]
pub struct Acc {
    /// `out`: the support vertices so far.
    pub out: Vec<V4>,
    pub props: Vec<PropRecord>,
    pub skipped: Skipped,
    /// `servedRegions`.
    pub served: IndexSet<usize>,
    pub tally: Tally,
}

/// `tally` (web/prop.js:240): `{ tines, sagRisk, sagRegions }`. Only `tines` and `sagRisk` are
/// snapshotted by the races; `sagRegions` is not.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tally {
    pub tines: usize,
    pub sag_risk: bool,
    pub sag_regions: IndexSet<usize>,
}

/// `opts.rasterVeto(q, normalProps)` (auto mode: fins/wedges.js `wedgeVeto`).
pub type RasterVeto = Rc<dyn Fn(&PropRecord, &[PropRecord]) -> bool>;

/// The `opts` of `buildProps` / `buildPass` and what they pass down (web/prop.js:139-215, the
/// prop modules). Each field says which key it is and the JS default; `Default` gives the JS
/// defaults (an absent `opts`).
///
/// `buildProps` itself adds `circularFrames` (the circular regions) to its own copy.
#[derive(Clone, Default)]
pub struct BuildOpts {
    /// `opts.step ?? PROP.stationStep`.
    pub step: Option<f64>,
    /// `opts.coverage ?? 0.5` (clamped to 0..1).
    pub coverage: Option<f64>,
    /// `opts.tines === true`. Default false.
    pub tines: bool,
    /// `opts.tineDensity` (`tineStepFor`'s argument; absent = the default comb).
    pub tine_density: Option<f64>,
    /// `opts.layerHeight ?? PROP.tineH`.
    pub layer_height: Option<f64>,
    /// `opts.raster === false` turns the raster race off; anything else leaves it on.
    pub raster: Option<bool>,
    /// `opts.rasterVeto`.
    pub raster_veto: Option<RasterVeto>,
    /// `!!opts.sway?.on`.
    pub sway_on: bool,
    /// `opts.rimWalls === false` turns the rim race off.
    pub rim_walls: Option<bool>,
    /// `opts.downhill === true` runs the downhill race.
    pub downhill: Option<bool>,
    /// `opts.curvedWalls === true`.
    pub curved_walls: Option<bool>,
    /// `opts.plateOnly === true`.
    pub plate_only: bool,
    /// `opts.shortWalls === true` (the last-resort pass).
    pub short_walls: bool,
    /// `opts.onlyRegions`: build these regions only.
    pub only_regions: Option<IndexSet<usize>>,
    /// `opts.downhillRegions`: the downhill pass over these regions.
    pub downhill_regions: Option<IndexSet<usize>>,
    /// `opts.rimRegions`: the rim pass over these regions.
    pub rim_regions: Option<IndexSet<usize>>,
    /// `opts.circularPass === true`.
    pub circular_pass: bool,
    /// `opts.circularFrames`: the circular regions, which `buildProps` adds to its own `opts`
    /// (web/prop.js:140).
    pub circular_frames: Option<std::sync::Arc<IndexMap<usize, CircularFrame>>>,
    /// `result.excludedSupportFaces`: JS reads it off the augmented analyze result that
    /// `automaticTargets` returns (prop/trace.js:83); `Analysis` has no such slot, so the caller
    /// passes it here (`Targets::excluded_support_faces`). `None` is the key absent.
    pub excluded_support_faces: Option<std::sync::Arc<Vec<u8>>>,
}

impl std::fmt::Debug for BuildOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuildOpts")
            .field("step", &self.step)
            .field("coverage", &self.coverage)
            .field("tines", &self.tines)
            .field("raster", &self.raster)
            .field("raster_veto", &self.raster_veto.is_some())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------- buildPass

use crate::fins::seating::seated_part_tris;
use crate::inside::{Nearest, inside_part, solid_clearance};
use crate::overhangs::{Analysis, MIN_REGION_AREA, Topology};
use crate::prop::arc::arc_line;
use crate::prop::attached::{Attached, build_part_attached};
use crate::prop::bridge::Support;
use crate::prop::bridgerace::bridge_race;
use crate::prop::circular::{CircularCache, circular_regions, circular_tracks};
use crate::prop::clearance::{
    body_mask, insert_floor_stations, longest_run, min_span_for, station_certified,
    station_is_clear, tall_body, tall_span, welds, with_low_tails,
};
use crate::prop::contact::{contour_top, lower_sag, settle_top, straightness};
use crate::prop::ctx::BuildCtx;
use crate::prop::curved::bends;
use crate::prop::downhill::{DownhillOpts, downhill_lines};
use crate::prop::judge::{JudgeCache, judge_ruler};
use crate::prop::lattice::{lattice_struts, strut_patches};
use crate::prop::raster::{
    RaceArgs, RaceTag, race_against, raster_rest, raster_tracks, with_raster,
};
use crate::prop::rim::{clip_to_rim, rim_tracks};
use crate::prop::rival::tube_rivalry;
use crate::prop::squat::build_squat_bed;
use crate::prop::surface::seat;
use crate::prop::sweep::sweep;
use crate::prop::tines::{TineOpts, emit_tines, tine_step_for};
use crate::prop::trace::trace_groups;
use crate::prop::tracks::{patch_tracks, split_region, tube_line};
use crate::types::{Rot3, Xyz};

/// `Math.hypot(a, b)`.
fn hyp2(a: f64, b: f64) -> f64 {
    jsnum::hypot(&[a, b])
}

/// The `[x, y, z]` of a soup vertex slice.
fn xyz_of(v: &[V4]) -> Vec<[f64; 3]> {
    v.iter().map(|p| [p[0], p[1], p[2]]).collect()
}

/// `solidClearance(topo, rot, off, out.slice(from), 0.25)`: 0.25 reach: the tightest threshold
/// below is 0.205, and every extra tenth of reach widens the broad phase for nothing.
fn clearance_of(
    topo: &Topology,
    rot: &Rot3,
    off: &Xyz,
    out: &[V4],
    from: usize,
) -> Option<crate::inside::Clearance> {
    solid_clearance(topo, rot, off, &xyz_of(&out[from..]), 0.25)
}

fn as_nearest(c: &crate::inside::Clearance) -> Nearest {
    Nearest {
        d: c.d,
        cos_up: c.cos_up,
    }
}

/// Signed volume of the solid `out[from..]` (divergence theorem over its triangles): the plastic
/// this wall costs, which is the number the "less material than slicer supports" claim has to be
/// measured against (web/prop.js:659-667).
fn signed_volume(out: &[V4], from: usize) -> f64 {
    let mut vol = 0.0f64;
    let mut i = from;
    while i < out.len() {
        let (a, b, c) = (out[i], out[i + 1], out[i + 2]);
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
        i += 3;
    }
    vol
}

/// What every patch of one `buildPass` shares.
struct Env<'a> {
    topo: &'a Topology,
    rot: &'a Rot3,
    off: &'a Xyz,
    cfg: &'a Config,
    ctx: &'a BuildCtx<'a>,
    part_tris: &'a Tris,
    with_tines: bool,
    tine_step_eff: f64,
    tine_height: f64,
    downhill: bool,
}

/// `runSquat` (web/prop.js:541-561): the SQUAT BED PASS: hold the near-bed stations too low for
/// the flanged wall (which discards everything under minHeight as stub/blocked). It runs AFTER
/// the tall wall, on every exit path, so it can skip exactly the stations that wall's low tail
/// covered (`claimed`) and nothing more. `squat_line` is a deep copy taken before the tall
/// path's settleTop mutates the shared points, so the squat pass sees the contoured line.
#[allow(clippy::too_many_arguments)]
fn run_squat(
    env: &Env,
    acc: &mut Acc,
    next_id: &mut usize,
    squat_line: &[[f64; 3]],
    claimed: &[bool],
    patch: &Patch,
    region_tris: &Tris,
    line_rim: bool,
) {
    let cfg = env.cfg;
    for sq in build_squat_bed(
        squat_line,
        region_tris,
        env.topo,
        env.rot,
        env.off,
        &mut acc.out,
        Some(claimed),
        env.ctx,
    ) {
        // a squat wall's base is the thin brim, not the tall flange, so tines
        // attach from squatBrimH up (the default minTop would skip every one).
        let t0 = acc.out.len();
        let c0 = env.ctx.cap_len();
        let mut grip = f64::INFINITY;
        let n_t = if env.with_tines {
            let top: Vec<[f64; 3]> = sq
                .line
                .iter()
                .map(|p| [p[0], p[1], p[2] + cfg.prop.gap])
                .collect();
            emit_tines(
                &top,
                Some(region_tris),
                env.topo,
                env.rot,
                env.off,
                &mut acc.out,
                TineOpts {
                    step: Some(env.tine_step_eff),
                    min_top: Some(cfg.prop.squat_brim_h),
                    tine_h: Some(env.tine_height),
                    body: None,
                    grip: Some(&mut grip),
                },
                env.ctx,
            )
        } else {
            0
        };
        acc.tally.tines += n_t;
        acc.served.insert(patch.region);
        // buildSquatBed pushed this wall (sq.triRange) BEFORE every squat wall's
        // tines, so a fin's wall and its tines are NON-contiguous in `out` --
        // track both segments so removing the fin takes wall AND tines together.
        let mut segs = vec![sq.tri_range];
        if acc.out.len() > t0 {
            segs.push([t0, acc.out.len()]);
        }
        acc.props.push(PropRecord {
            span: sq.span,
            height: sq.height,
            stations: sq.stations,
            volume: sq.volume,
            squat: sq.squat,
            line: sq.line.clone(),
            tri_range: Some(sq.tri_range),
            area: patch.area,
            region: Some(patch.region),
            tines: n_t,
            caps: c0.map(|c| [c, env.ctx.cap_len().unwrap()]),
            grip: Some(grip),
            id: {
                *next_id += 1;
                *next_id - 1
            },
            kind: "prop",
            tri_ranges: segs,
            rim_wall: line_rim,
            ..Default::default()
        });
    }
}

/// `seated(f)` (web/prop.js:363): face `f`'s three vertices, seated.
fn seated_face(topo: &Topology, rot: &Rot3, off: &Xyz, f: usize) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|i| seat(&topo.pos, f * 9 + i * 3, rot, off))
}

/// A `tracks`-shaped list of lines as `Lines` (no rim tags).
fn lines_of(tracks: Vec<Vec<[f64; 3]>>, spacing: Option<f64>, turned: Option<[f64; 2]>) -> Lines {
    Lines {
        lines: tracks
            .into_iter()
            .map(|pts| Line {
                pts,
                rim: false,
                from: None,
            })
            .collect(),
        spacing,
        turned,
    }
}

/// Port of `buildPass` (web/prop.js:211-713): one placement pass. `raster` None = the normal
/// placement for every region; Some = raster tracks (rasterTracks) for those regions only.
///
/// `opts.circular_frames` is `buildProps`'s own addition to `opts`.
pub(crate) fn build_pass(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    raster: Option<&IndexSet<usize>>,
    ctx: &BuildCtx,
) -> PropsPass {
    let cfg = ctx.cfg;
    let prop = &cfg.prop;
    let pos = &topo.pos;
    let step = opts.step.unwrap_or(prop.station_step);
    // Wide-face coverage (0 sparse .. 1 dense) sets the row spacing via coverRowSpan: 0.5 is the
    // anti-sag cap (the default), left of it loosens past the cap (fewer supports, flagged as
    // sagRisk when a row actually lands wider than the cap), right of it tightens. Pinned by
    // tests/coverage.test.js.
    let coverage = jsnum::max(0.0, jsnum::min(1.0, opts.coverage.unwrap_or(0.5)));
    let row_span = cover_row_span(coverage, cfg);
    // sagRisk warns ONLY when the user dragged coverage below centre, asking for row pitch wider
    // than the anti-sag cap. It is NOT enough that the placed spacing exceeds the cap: rounding
    // vExt/rowSpan down routinely lands a hair over the cap even at the neutral default (a 53mm
    // face / 12mm cap -> 4 rows at 13.25mm), and warning there is just noise. So gate on the
    // REQUESTED pitch, not the rounded result.
    let want_sparse = row_span > prop.max_unsupported_span + 0.5;
    let z_bed = 0.0;
    let off = &result.offset;
    let with_tines = opts.tines;
    let only = opts.only_regions.as_ref();
    let short = opts.short_walls;
    let downhill = opts.downhill_regions.as_ref();
    // Curved walls (prop/trace.js): the downhill lines trace regions grown across the slivers,
    // and regions whose growth meets trace (and race) as one
    let traced = match downhill {
        Some(d) if opts.curved_walls == Some(true) => Some(trace_groups(
            topo,
            result,
            opts.excluded_support_faces.as_ref().map(|v| v.as_slice()),
            rot,
            off,
            d,
            cfg,
        )),
        _ => None,
    };
    let mut acc = Acc::default();
    // Per-fin identity, assigned in build order. The id is only used to map a raycast hit back to
    // its fin WITHIN one build (the UI tracks removals across rebuilds by a spatial signature,
    // not this id). Sequential keeps it stable within a generation.
    let mut next_id = 0usize;
    // Plate only (#218): every support stands on the plate. A line with part under it (the
    // part-attached path's) is counted (onPart) and handed to the plate path, whose column probes
    // trim the stations over the part and keep any stretch that stands on the plate -- never
    // stilting through the part.

    // The whole part, seated once, for the part-attached floor probe: the floor a support lands
    // on is usually a DIFFERENT region than the overhang, so it must raycast the full mesh. It is
    // the pose's cached seated copy (fins/seating.js `seatedPartTris`, the same arithmetic as the
    // per-pass copy JS builds), so the surface grid on it is built once per pose, not once per
    // pass: on a 772k-triangle part that grid is ~36 ms each.
    let part_tris = seated_part_tris(topo, rot, off);

    // DENSE grip comb, uniform along every grippable wall (see the JS comment: the user's "Tine
    // grip" slider, opts.tineDensity, can loosen it, never silently).
    let tine_step_eff = tine_step_for(opts.tine_density, cfg);
    // A tine MUST be exactly one slicer layer tall or it stops printing as one clean continuous
    // bead and tears on removal (welts) instead of bending off. So it tracks the user's real layer
    // height (default 0.2mm) -- see the UI's Layer height.
    let tine_height = opts.layer_height.unwrap_or(prop.tine_h);
    let env = Env {
        topo,
        rot,
        off,
        cfg,
        ctx,
        part_tris: &*part_tris,
        with_tines,
        tine_step_eff,
        tine_height,
        downhill: downhill.is_some(),
    };
    let support = Support {
        topo,
        rot,
        offset: off,
        sway: opts.sway_on,
    };

    // The support unit is the locally-straight sub-patch, not the connected region -- see
    // splitRegion. Fragments too small to be worth a wall are counted, not silently dropped:
    // absence of output recorded as success is exactly how M5's scoreboard lied.
    let mut patches: Vec<Patch> = Vec::new();
    for ri in 0..result.regions.len() {
        if only.is_some_and(|o| !o.contains(&ri)) {
            continue;
        }
        let r_faces = &result.regions[ri].faces;
        let circular = opts.circular_frames.as_ref().and_then(|f| f.get(&ri));
        if opts.circular_pass && circular.is_none() {
            continue;
        }
        // Seat the WHOLE region's triangles once, shared by all its sub-patches. The polyline each
        // wall follows comes from its own sub-patch, but the surface its top must CLEAR is the
        // whole region's: a wall near a patch boundary can run under a sibling patch's faces, and
        // measuring against patch-only triangles welded it to geometry it could not see -- gap
        // 0.003 mm and flank 0.011 mm on hub_corner, measured the first time this split shipped
        // with per-patch triangles.
        let mut region_v = vec![0.0f64; r_faces.len() * 9];
        let mut region_pts: Vec<[f64; 3]> = Vec::new();
        let mut region_area = 0.0f64;
        for (k, &f) in r_faces.iter().enumerate() {
            region_area += topo.area[f];
            let (mut gx, mut gy, mut gz) = (0.0f64, 0.0f64, 0.0f64);
            for i in 0..3 {
                let v = seat(pos, f * 9 + i * 3, rot, off);
                region_v[k * 9 + i * 3] = v[0];
                region_v[k * 9 + i * 3 + 1] = v[1];
                region_v[k * 9 + i * 3 + 2] = v[2];
                region_pts.push(v);
                gx += v[0];
                gy += v[1];
                gz += v[2];
            }
            region_pts.push([gx / 3.0, gy / 3.0, gz / 3.0]);
        }
        let region_tris = Arc::new(Tris::new(region_v));

        if opts.circular_pass {
            let circular_lines =
                circular.and_then(|c| circular_tracks(c, &region_tris, step, row_span, cfg));
            if let Some(l) = circular_lines.filter(|l| !l.is_empty()) {
                patches.push(Patch {
                    faces: r_faces.clone(),
                    area: region_area,
                    region: ri,
                    tris: region_tris.clone(),
                    lines: Some(lines_of(l, None, None)),
                    ..Default::default()
                });
            }
            continue;
        }

        // Downhill pass (prop/downhill.js): walls down the region's slope, for the regions in
        // `opts.downhillRegions` only -- the race in buildProps decides.
        if let Some(dh) = downhill {
            if !dh.contains(&ri) {
                continue;
            }
            if let Some(traced) = &traced {
                if traced.leader_of.get(&ri) != Some(&ri) {
                    continue; // traced with its group's leader
                }
                let g = &traced.groups[&ri];
                let lines = downhill_lines(
                    &g.pts,
                    &g.tris,
                    step,
                    prop.max_unsupported_span,
                    &DownhillOpts {
                        h: Some(cfg.trace.h),
                        cover: Some(&g.red),
                        reach: None,
                    },
                    cfg,
                );
                let faces: Vec<usize> = g
                    .members
                    .iter()
                    .flat_map(|&m| result.regions[m].faces.iter().copied())
                    .collect();
                let mut area = 0.0f64;
                for &f in &faces {
                    area += topo.area[f];
                }
                if let Some(lines) = lines {
                    patches.push(Patch {
                        faces,
                        area,
                        region: ri,
                        tris: Arc::new(g.tris.clone()),
                        lines: Some(lines_of(lines, None, None)),
                        members: Some(g.members.clone()),
                        ..Default::default()
                    });
                }
                continue;
            }
            let lines = downhill_lines(
                &region_pts,
                &region_tris,
                step,
                prop.max_unsupported_span,
                &DownhillOpts::default(),
                cfg,
            );
            if let Some(lines) = lines {
                patches.push(Patch {
                    faces: r_faces.clone(),
                    area: region_area,
                    region: ri,
                    tris: region_tris.clone(),
                    lines: Some(lines_of(lines, None, None)),
                    ..Default::default()
                });
            }
            continue;
        }

        // Raster pass: the whole footprint's tracks, no tube route, no split.
        if let Some(rs) = raster {
            if !rs.contains(&ri) {
                continue;
            }
            // A lattice net (issue #121): its struts, each routed as a standalone strut region
            // would be, stand in for the raster tracks -- parallel rows across a net land in its
            // holes. See prop/lattice.js.
            let seated = |f: usize| seated_face(topo, rot, off, f);
            if let Some(struts) = lattice_struts(topo, r_faces, &seated, cfg) {
                if struts.lost != 0 {
                    acc.skipped.sliver += 1; // faces no strut reached: dropped, so counted
                }
                for sp in strut_patches(
                    topo,
                    rot,
                    &struts.groups,
                    &seated,
                    step,
                    ri,
                    &region_tris,
                    cfg,
                ) {
                    patches.push(Patch {
                        faces: sp.faces,
                        area: sp.area,
                        region: sp.region,
                        tris: sp.tris,
                        lines: sp.lines.map(|l| lines_of(l, None, None)),
                        small_tube: sp.small_tube.and_then(|s| {
                            s.map(|v| {
                                v.into_iter()
                                    .map(|q| Patch {
                                        faces: q.faces,
                                        area: q.area,
                                        axis: Some(q.axis),
                                        ..Default::default()
                                    })
                                    .collect()
                            })
                        }),
                        strut: sp.strut,
                        axis: sp.axis,
                        reach: sp.reach.map(Arc::new),
                        ..Default::default()
                    });
                }
                continue;
            }
            let lines = raster_tracks(
                &region_pts,
                &region_tris,
                &*part_tris,
                step,
                row_span,
                &support,
                cfg,
            );
            // the sparse-coverage sag warning, per region: it only counts if this region swaps
            if want_sparse && lines.spacing.is_some_and(|s| s > prop.max_unsupported_span) {
                acc.tally.sag_regions.insert(ri);
            }
            if !lines.lines.is_empty() {
                patches.push(Patch {
                    faces: r_faces.clone(),
                    area: region_area,
                    region: ri,
                    tris: region_tris.clone(),
                    lines: Some(lines),
                    ..Default::default()
                });
            }
            continue;
        }

        // Curved region whose lowest line is straight = a tube: ONE wall under that line, the way
        // breakaway.py props the shelter hubs. Only when the region is flat, or its lowest points
        // form a ring, does it go to splitRegion for rows of tracks. See tubeLine.
        // A SMALL tube (under tubeMinArea, see tubeLine) is a new claim on a region the patch path
        // used to own, so it keeps the region's split too: the tube is built first, the patches
        // then compete for the same region, and the one that holds more of it stays (see
        // web/prop/rival.js).
        let tube = tube_line(topo, r_faces, rot, &region_pts, &region_tris, step, cfg);
        let small = region_area < prop.tube_min_area;
        let tube_has = tube.as_ref().is_some_and(|t| !t.is_empty());
        let split: Option<Vec<Patch>> = if small || !tube_has {
            Some(
                split_region(topo, r_faces, rot, cfg)
                    .into_iter()
                    .map(|g| Patch {
                        faces: g.faces,
                        area: g.area,
                        ..Default::default()
                    })
                    .collect(),
            )
        } else {
            None
        };
        if tube_has {
            patches.push(Patch {
                faces: r_faces.clone(),
                area: region_area,
                region: ri,
                tris: region_tris.clone(),
                lines: Some(lines_of(tube.unwrap(), None, None)),
                small_tube: if small { split } else { None },
                ..Default::default()
            });
            continue;
        }
        // A level strip that curves in plan (a slot through a tube's wall): one wall along it,
        // raced against the split's straight tracks like a small tube -- and it keeps a tie,
        // since it stands under the strip's whole length. See prop/arc.js.
        if let Some(arc) = arc_line(&region_pts, &region_tris, step, cfg) {
            patches.push(Patch {
                faces: r_faces.clone(),
                area: region_area,
                region: ri,
                tris: region_tris.clone(),
                lines: Some(lines_of(arc, None, None)),
                small_tube: split,
                arc: true,
                ..Default::default()
            });
            continue;
        }

        for mut p in split.unwrap() {
            if p.area < MIN_REGION_AREA {
                acc.skipped.sliver += 1;
                continue;
            }
            p.region = ri;
            p.tris = region_tris.clone();
            patches.push(p);
        }
    }

    // A small tube competes with its region's patches: see web/prop/rival.js.
    let mut rivalry = tube_rivalry(topo, result, &part_tris.v, cfg);
    // A flat patch turned across the slicer's bridge competes with its old-way walls: see
    // web/prop/bridgerace.js.
    let mut race = bridge_race();

    let mut rims: IndexMap<usize, Vec<Vec<[f64; 3]>>> = IndexMap::new(); // region -> its rim tracks (prop/rim.js)
    let mut pi = 0;
    while pi < patches.len() {
        let patch = patches[pi].clone();
        pi += 1;
        let i = pi - 1;
        let region_tris = patch.tris.clone();
        race.begin(&mut acc, &patches[i], ctx); // first: settling a race cuts `out` back
        let mark = rivalry.begin(&mut acc, &patches[i]); // a small tube undoes to here
        let mut lines: Lines;
        // where the rim tracks sit in `lines` (first patch of a rim region), to write them back
        let mut rim_slot: Option<(usize, usize)> = None;
        if let Some(l) = &patch.lines {
            // (the rim pass rebuilds only patch-path regions: a tube's, an arc's, a raster's keep
            // what they had)
            if opts.rim_regions.is_some() {
                continue;
            }
            // A tube's lowest-line track(s), fitted and resampled by tubeLine.
            lines = l.clone();
        } else {
            // The patch's own geometry: vertices plus face centroids for the frame fit, and its
            // triangles for asking "does the patch cover this station".
            let mut pts: Vec<[f64; 3]> = Vec::new();
            let mut patch_v = vec![0.0f64; patch.faces.len() * 9];
            for (k, &f) in patch.faces.iter().enumerate() {
                let (mut gx, mut gy, mut gz) = (0.0f64, 0.0f64, 0.0f64);
                for q in 0..3 {
                    let v = seat(pos, f * 9 + q * 3, rot, off);
                    pts.push(v);
                    patch_v[k * 9 + q * 3] = v[0];
                    patch_v[k * 9 + q * 3 + 1] = v[1];
                    patch_v[k * 9 + q * 3 + 2] = v[2];
                    gx += v[0];
                    gy += v[1];
                    gz += v[2];
                }
                pts.push([gx / 3.0, gy / 3.0, gz / 3.0]);
            }
            let tr = patch_tracks(
                &pts,
                &Tris::new(patch_v),
                step,
                Some(&support),
                row_span,
                patch.axis,
                true,
                cfg,
            );
            lines = lines_of(tr.lines, tr.spacing, tr.turned);
            race.turned(&acc, i, &lines, &mut patches, ctx);
            // Rim walls (prop/rim.js): for the regions in `opts.rimRegions` only -- the race in
            // buildProps decides -- a wall along the patch's free outline, the rows stopped short
            // of it.
            // The rim is the REGION's outline (a flat patch's edge can meet the same overhang
            // carrying on as another patch, a rounded edge's lower half), traced once: its first
            // patch takes the rim, every patch's rows stop short of it.
            if opts
                .rim_regions
                .as_ref()
                .is_some_and(|r| r.contains(&patch.region))
            {
                let first = !rims.contains_key(&patch.region);
                if first {
                    let rim = rim_tracks(
                        topo,
                        result,
                        rot,
                        &result.regions[patch.region].faces,
                        &region_tris,
                        step,
                        cfg,
                    );
                    rims.insert(patch.region, rim);
                }
                let rim = rims[&patch.region].clone();
                if rim.is_empty() {
                    continue; // no free outline: the region keeps what it had
                }
                let rows: Vec<Vec<[f64; 3]>> = lines.lines.iter().map(|l| l.pts.clone()).collect();
                // `Object.assign([...clipToRim(lines, rim), ...(first ? rim : [])], { spacing, turned })`
                // (prop.js:464). TAG: the rim tracks are tagged `rim` by rimTracks (rim.js:68,
                // `Object.assign(t, { rim: true })`) on the arrays themselves; the spread puts those
                // SAME array objects into the new `lines`, so the tag survives for them and
                // nothing copies `line` between here and prop.js:518/559/691. clipToRim's pieces
                // are fresh arrays (rim.js:76+) and carry no tag.
                let mut out_lines: Vec<Line> = clip_to_rim(&rows, &rim, cfg)
                    .into_iter()
                    .map(|pts| Line {
                        pts,
                        rim: false,
                        from: None,
                    })
                    .collect();
                if first {
                    rim_slot = Some((out_lines.len(), rim.len()));
                    out_lines.extend(rim.into_iter().map(|pts| Line {
                        pts,
                        rim: true,
                        from: None,
                    }));
                }
                lines = Lines {
                    lines: out_lines,
                    spacing: lines.spacing,
                    turned: lines.turned,
                };
            }
            // The user chose sub-cap spacing (wantSparse) AND this face actually landed a
            // multi-row gap wider than the cap. Flag it so the UI can warn (never blocks;
            // Matthew's call). Single-row faces (spacing 0) can't sag, so they don't warn.
            if want_sparse
                && !lines.lines.is_empty()
                && lines.spacing.is_some_and(|s| s > prop.max_unsupported_span)
            {
                acc.tally.sag_risk = true;
            }
        }
        if lines.lines.is_empty() {
            acc.skipped.no_line += 1;
            continue;
        }

        // An index loop, not for...of over a copy: a raster track's leftover pieces are appended
        // to `lines` mid-loop (rasterRest).
        let mut li = 0;
        while li < lines.lines.len() {
            let mut line = lines.lines[li].pts.clone();
            let line_rim = lines.lines[li].rim;
            li += 1;
            place_line(
                &env,
                &patch,
                &region_tris,
                &mut line,
                line_rim,
                raster,
                opts,
                short,
                &mut acc,
                &mut next_id,
                &mut lines,
                z_bed,
            );
            // the rim tracks are shared arrays in JS: what this line became (contourTop,
            // lowerSag and insertFloorStations mutate it in place) is what the region's later
            // patches clip against
            if let Some((start, n)) = rim_slot {
                let idx = li - 1;
                if idx >= start && idx < start + n {
                    rims.get_mut(&patch.region).unwrap()[idx - start] = line;
                }
            }
        }
        if patch.small_tube.is_some() {
            rivalry.tube_done(
                &mut acc,
                i,
                mark.expect("a small tube gets a mark"),
                &mut patches,
            );
        }
    }
    rivalry.finish(&mut acc);
    race.finish(&mut acc, ctx);

    // `served` counts REGIONS with at least one wall, because a region can now yield several --
    // subtracting a prop count from a region count would say a part with one region and three
    // walls had "-2 unserved".
    let volume = acc.props.iter().fold(0.0f64, |s, q| s + q.volume);
    PropsPass {
        triangles: acc.out,
        served: acc.served.len(),
        served_regions: acc.served.iter().copied().collect(),
        tines: acc.tally.tines,
        sag_risk: acc.tally.sag_risk,
        sag_regions: raster.map(|_| acc.tally.sag_regions.clone()),
        leader_of: traced.map(|t| t.leader_of),
        props: acc.props,
        skipped: acc.skipped,
        volume,
    }
}

/// The body of `buildPass`'s per-line loop (web/prop.js:475-698): place the wall for one track
/// `line` (mutated in place, as in JS: `contourTop`, `lowerSag` and `insertFloorStations` work on
/// it) of `patch`. `lines` is the patch's track list, which a raster track's leftover pieces are
/// appended to (`rasterRest`). `line_rim` is `line.rim`.
#[allow(clippy::too_many_arguments)]
fn place_line(
    env: &Env,
    patch: &Patch,
    region_tris: &Tris,
    line: &mut Vec<[f64; 3]>,
    line_rim: bool,
    raster: Option<&IndexSet<usize>>,
    opts: &BuildOpts,
    short: bool,
    acc: &mut Acc,
    next_id: &mut usize,
    lines: &mut Lines,
    z_bed: f64,
) {
    let (topo, rot, off, cfg, ctx) = (env.topo, env.rot, env.off, env.cfg, env.ctx);
    let prop = &cfg.prop;
    // PART-ATTACHED first: if solid part sits below this overhang, a support must stand on THAT
    // floor, not stilt to the plate through the part (the bug Matthew hit on a real hub).
    // buildPartAttached declines on an ordinary bed overhang (floorLine ~0), so the plate path
    // below is reached unchanged for the flagship parts. When there IS a floor but no safe wall
    // fits (side walls in the way, a buried column, too short) it says `floored` with the reason
    // -- counted and skipped, never stilted through the part. A bore is no longer a refusal: its
    // wall is built and flagged inBore. Works on a COPY so the plate path's own `line` is
    // untouched.
    let tri0 = acc.out.len();
    let mut pa = build_part_attached(line, env.part_tris, topo, rot, off, &mut acc.out, ctx);
    // (Plate only: undo it and take the plate path, as a bed overhang would)
    if plate_only_hits(opts.plate_only, &pa) {
        acc.out.truncate(tri0);
        acc.skipped.on_part += 1;
        pa = Attached::NoFloor;
    }
    if let (Attached::Built(_), Some(_)) = (&pa, &patch.small_tube) {
        // A small tube's line is new to this path, so hold its wall to the same measured
        // clearance the plate path demands (see the sweep below): on 3DBenchy at Y35 an unchecked
        // one fused 0.09 mm into the cabin roof.
        if let Some(hit) = clearance_of(topo, rot, off, &acc.out, tri0)
            && welds(&as_nearest(&hit), cfg)
        {
            acc.out.truncate(tri0);
            acc.skipped.weld += 1;
            return;
        }
    }
    match pa {
        Attached::Built(p) => {
            acc.served.insert(patch.region);
            let mut n_t = 0;
            let c0 = ctx.cap_len();
            let mut grip = f64::INFINITY;
            // pa.prop.line already carries the wall top (surface minus gap); add the gap back so
            // emitTines reads it as the surface, like the plate path does.
            if env.with_tines {
                let top_line: Vec<[f64; 3]> = p
                    .line
                    .iter()
                    .map(|q| [q[0], q[1], q[2] + prop.gap])
                    .collect();
                n_t = emit_tines(
                    &top_line,
                    Some(env.part_tris),
                    topo,
                    rot,
                    off,
                    &mut acc.out,
                    TineOpts {
                        step: Some(env.tine_step_eff),
                        min_top: None,
                        tine_h: Some(env.tine_height),
                        body: None,
                        grip: Some(&mut grip),
                    },
                    ctx,
                );
                acc.tally.tines += n_t;
            }
            // buildPartAttached pushed the wall starting at tri0; emitTines above pushed its
            // tines right after, so wall + tines are contiguous -> one segment.
            acc.props.push(PropRecord {
                span: p.span,
                height: p.height,
                stations: p.stations,
                volume: p.volume,
                part_attached: p.part_attached,
                in_bore: Some(p.in_bore),
                line: p.line.clone(),
                area: patch.area,
                region: Some(patch.region),
                tines: n_t,
                caps: c0.map(|c| [c, ctx.cap_len().unwrap()]),
                grip: Some(grip),
                trimmed: Some(line.len().saturating_sub(p.stations)),
                id: {
                    *next_id += 1;
                    *next_id - 1
                },
                kind: "prop",
                // (prop.js:518: `line.rim`; the tag is the one rimTracks put on this very array)
                rim_wall: line_rim,
                tri_ranges: vec![[tri0, acc.out.len()]],
                ..Default::default()
            });
            return;
        }
        Attached::Floored(why) => {
            acc.skipped.bump(why.as_str());
            return;
        }
        Attached::NoFloor => {}
    }

    // Finish the top against the WHOLE region, not just this patch: a track near a patch boundary
    // can run under a sibling patch's faces, and clearance measured against patch-only triangles
    // welded walls to geometry they could not see (gap 0.003mm, flank 0.011mm, hub_corner).
    contour_top(line, region_tris, f64::INFINITY, cfg);
    lower_sag(line, region_tris, f64::INFINITY);
    // pin the wall's low end to the squat floor, not the nearest 1mm station
    insert_floor_stations(line, cfg);
    // `if (insertFloorStations(line).length)`: JS returns `line` itself, so this is "the line is
    // not empty", not "a station was inserted"
    if !line.is_empty() {
        contour_top(line, region_tris, f64::INFINITY, cfg);
    }

    // SQUAT BED PASS: hold the near-bed stations too low for the flanged wall below (which
    // discards everything under minHeight as stub/blocked). It runs AFTER the tall wall (runSquat,
    // on every exit path) so it can skip exactly the stations that wall's low tail covered
    // (`claimed`) and nothing more. squatLine is a deep copy taken now, before the tall path's
    // settleTop mutates the shared points, so the squat pass sees the contoured line.
    let squat_line: Vec<[f64; 3]> = line.clone();
    let mut claimed = vec![false; line.len()];

    // A track is straight in XY by construction, so this gate is a tripwire rather than the
    // bowl-refusal it was for bucketed polylines -- bowls are now refused by their holes (see
    // patchTracks). Keep it: anything that trips it means the frame fit itself went wrong.
    if straightness(line) > prop.max_wander {
        acc.skipped.wanders += 1;
        run_squat(
            env,
            acc,
            next_id,
            &squat_line,
            &claimed,
            patch,
            region_tris,
            line_rim,
        );
        return;
    }

    // Trim to the longest run that can actually carry a wall, rather than discarding the track
    // over a local problem. See `longestRun`.
    let clear: Vec<bool> = line
        .iter()
        .enumerate()
        .map(|(k, p)| {
            p[2] - prop.gap >= prop.min_height_squat
                && station_is_clear(line, k, topo, rot, off, cfg)
        })
        .collect();
    let usable = with_low_tails(
        &line
            .iter()
            .enumerate()
            .map(|(k, p)| clear[k] && p[2] - prop.gap >= prop.min_height)
            .collect::<Vec<_>>(),
        &clear,
    );
    let run = longest_run(&usable);
    let Some(run) = run.filter(|r| ((r[1] - r[0]) as f64) >= prop.min_stations) else {
        acc.skipped.blocked += 1;
        run_squat(
            env,
            acc,
            next_id,
            &squat_line,
            &claimed,
            patch,
            region_tris,
            line_rim,
        );
        return;
    };
    if raster.is_some() && !patch.strut {
        // a strut takes one wall, as it would standing alone
        for rest in raster_rest(line, run, cfg) {
            // the piece runs its own squat pass; this line's must not double it
            let from = rest.from.unwrap();
            for k in from..from + rest.pts.len() {
                claimed[k] = true;
            }
            lines.lines.push(rest);
        }
    }
    let mut sub: Vec<[f64; 3]> = line[run[0]..run[1]].to_vec();

    let body = body_mask(&sub, cfg); // before settleTop -- see bodyMask
    if tall_span(&sub, &body) < min_span_for(&sub, &body, patch.small_tube.is_some(), short, cfg) {
        acc.skipped.stub += 1;
        run_squat(
            env,
            acc,
            next_id,
            &squat_line,
            &claimed,
            patch,
            region_tris,
            line_rim,
        );
        return;
    }

    // Last, on the trimmed run only: put the closest approach exactly on spec. It runs here
    // rather than earlier because trimming changes which part of the edge is closest, so settling
    // before the trim settles the wrong thing.
    settle_top(&mut sub, region_tris, 0.25, f64::INFINITY, cfg);

    // Settling can push a station that was only just tall enough below the floor, and `sweep`
    // would then throw away the whole wall -- the same all-or-nothing failure the trim exists to
    // prevent, reintroduced one step later. Re-trim against the settled line: on height, and on
    // the measured clearance to everything settleTop could not see (stationCertified). Tall
    // stations carry the wall; low ones may only extend it as its tail.
    let mut low_a: Vec<bool> = sub
        .iter()
        .enumerate()
        .map(|(k, p)| {
            p[2] - prop.gap >= prop.min_height_squat
                && station_certified(&sub, k, topo, rot, off, cfg)
        })
        .collect();
    let mut tall_a: Vec<bool> = sub
        .iter()
        .enumerate()
        .map(|(k, p)| low_a[k] && p[2] - prop.gap >= prop.min_height)
        .collect();

    // Sweep, then MEASURE the finished solid -- exact triangle-to-triangle clearance against the
    // whole part (solidClearance), because the last welds this pipeline shipped sat between
    // stations, where no per-station probe would ever look. A contact is a local problem like
    // every other: trim the station that owns it and try again, up to a few rounds, rather than
    // discarding a 90mm wall over one rib. A wall that cannot be cut clear is dropped -- "no prop"
    // is a fixable disappointment, a fused prop is a ruined print.
    let mut placed = false;
    let mut reason: Option<&str> = None;
    let mut tries = 0;
    while tries < 4 && !placed {
        tries += 1;
        let run2 = longest_run(&with_low_tails(&tall_a, &low_a));
        let Some(run2) = run2.filter(|r| ((r[1] - r[0]) as f64) >= prop.min_stations) else {
            reason = Some("blocked");
            break;
        };
        let settled: Vec<[f64; 3]> = sub[run2[0]..run2[1]].to_vec();
        let span2 = hyp2(
            settled[settled.len() - 1][0] - settled[0][0],
            settled[settled.len() - 1][1] - settled[0][1],
        );
        let settled_body: Vec<bool> = body[run2[0]..run2[1]].to_vec();
        if tall_span(&settled, &settled_body)
            < min_span_for(
                &settled,
                &settled_body,
                patch.small_tube.is_some(),
                short,
                cfg,
            )
        {
            reason = Some("stub");
            break;
        }

        let before = acc.out.len();
        if !sweep(&settled, z_bed, &mut acc.out, prop.min_height_squat, ctx) {
            acc.out.truncate(before);
            reason = Some("degenerate");
            break;
        }

        // 0.25 reach: the tightest threshold below is 0.205, and every extra tenth of reach
        // widens the broad phase for nothing
        let hit = clearance_of(topo, rot, off, &acc.out, before);
        // Same acceptance as stationCertified: an approach from above is the breakaway interface,
        // anything else is a flank. Interpenetration measures 0 and fails the flank test, which
        // is what retires the old vertex-containment `buried` check -- crossing surfaces have
        // distance 0 long before any vertex is inside.
        if let Some(hit) = hit
            && welds(&as_nearest(&hit), cfg)
        {
            acc.out.truncate(before);
            let (mut k_best, mut d_best) = (0usize, f64::INFINITY);
            for (k, p) in settled.iter().enumerate() {
                let dx = p[0] - hit.x;
                let dy = p[1] - hit.y;
                if dx * dx + dy * dy < d_best {
                    d_best = dx * dx + dy * dy;
                    k_best = k;
                }
            }
            let at = run2[0] + k_best;
            for k in [at.saturating_sub(1), at, (low_a.len() - 1).min(at + 1)] {
                low_a[k] = false;
                tall_a[k] = false;
            }
            reason = Some("weld");
            continue;
        }

        // The TALLEST point, not the lowest: this is what the wall costs to print and how far it
        // has to stand up on its own. `line` is deliberately not used here -- the wall only
        // exists over `sub`.
        let top = settled
            .iter()
            .fold(f64::NEG_INFINITY, |m, p| jsnum::max(m, p[2]))
            - prop.gap;
        // signed volume of the emitted solid (divergence theorem over its triangles): the plastic
        // this wall costs, which is the number the "less material than slicer supports" claim has
        // to be measured against
        let vol = signed_volume(&acc.out, before);
        acc.served.insert(patch.region);
        // The grip comb: nubs along this wall's settled top that bite into the part. `settled`
        // carries the surface z; emitTines subtracts the gap.
        let c0 = ctx.cap_len();
        let mut grip = f64::INFINITY;
        let wall_end = acc.out.len();
        let n_t = if env.with_tines {
            emit_tines(
                &settled,
                Some(region_tris),
                topo,
                rot,
                off,
                &mut acc.out,
                TineOpts {
                    step: Some(env.tine_step_eff),
                    min_top: None,
                    tine_h: Some(env.tine_height),
                    body: tall_body(&settled_body),
                    grip: Some(&mut grip),
                },
                ctx,
            )
        } else {
            0
        };
        acc.tally.tines += n_t;
        acc.props.push(PropRecord {
            region: Some(patch.region),
            tines: n_t,
            caps: c0.map(|c| [c, ctx.cap_len().unwrap()]),
            grip: Some(grip),
            span: span2,
            height: top - z_bed,
            area: patch.area,
            stations: settled.len(),
            trimmed: Some(line.len().saturating_sub(settled.len())),
            volume: jsnum::abs(vol),
            // the centreline, so a coverage check can ask what this wall reaches
            line: settled
                .iter()
                .map(|p| [p[0], p[1], p[2] - prop.gap])
                .collect(),
            id: {
                *next_id += 1;
                *next_id - 1
            },
            kind: "prop",
            // `before` (captured at the start of THIS try) marks where this wall's triangles begin
            // in `out`; failed weld retries roll back to it, so on the successful try it points at
            // this wall. emitTines just pushed its tines right after, so wall + tines are one
            // contiguous segment.
            tri_ranges: vec![[before, acc.out.len()]],
            wall_range: if env.downhill {
                Some([before, wall_end])
            } else {
                None
            }, // the wall without its tines
            members: patch.members.clone(), // a traced group's regions
            // a rim wall (prop/rim.js; prop.js:691: `line.rim`)
            rim_wall: line_rim,
            ..Default::default()
        });
        for k in run[0] + run2[0]..run[0] + run2[1] {
            claimed[k] = true;
        }
        placed = true;
    }
    if !placed && let Some(r) = reason {
        acc.skipped.bump(r);
    }
    run_squat(
        env,
        acc,
        next_id,
        &squat_line,
        &claimed,
        patch,
        region_tris,
        line_rim,
    );
}

/// `plateOnly && (pa.ok || pa.floored)`.
fn plate_only_hits(plate_only: bool, pa: &Attached) -> bool {
    plate_only && !matches!(pa, Attached::NoFloor)
}

/// Port of `buildProps` (web/prop.js:139-200): build a breakaway prop under every overhang region
/// that can take one.
///
/// Two passes: the normal one (tube line or split patches per region), then a raster pass over
/// the regions it left partly bare, and each region keeps the pass that holds more of it -- see
/// web/prop/raster.js. `opts.raster == Some(false)` runs the normal pass alone.
///
/// The per-build caches JS keys on the analyze result (the judge's and the circular
/// footprint's `WeakMap`s) are owned here, one of each per build.
pub fn build_props(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    ctx: &BuildCtx,
) -> PropsPass {
    let cfg = ctx.cfg;
    let judge_cache = JudgeCache::default();
    let circular_cache = CircularCache::new();
    let mut o = opts.clone();
    o.circular_frames = Some(circular_regions(topo, result, rot, &circular_cache, cfg));
    let opts = &o;
    let cap_from = ctx.cap_len().unwrap_or(0); // tests' tine capture: see withRaster
    let normal = build_pass(topo, result, rot, opts, None, ctx);
    let raster_base = if opts.raster == Some(false) {
        normal
    } else {
        with_raster(
            topo,
            result,
            rot,
            opts,
            normal,
            cap_from,
            &|regions| build_pass(topo, result, rot, opts, Some(regions), ctx),
            ctx,
        )
    };
    // Circular candidates use a local geometric frame. The race still checks the surrounding
    // faces and lowest grip; keep the original layout if they lose.
    let has_circular = opts.circular_frames.as_ref().is_some_and(|f| !f.is_empty());
    let circ = if opts.raster != Some(false) && has_circular {
        race_against(
            topo,
            result,
            rot,
            opts,
            raster_base,
            cap_from,
            &|regions| {
                build_pass(
                    topo,
                    result,
                    rot,
                    &BuildOpts {
                        only_regions: Some(regions.clone()),
                        circular_pass: true,
                        ..opts.clone()
                    },
                    None,
                    ctx,
                )
            },
            RaceTag::Circular,
            RaceArgs {
                rel: 0.0,
                equivalent: true,
                ..Default::default()
            },
            ctx,
        )
    } else {
        raster_base
    };
    // Rim walls (prop/rim.js, local issue 062): a region the patch path built also tries a wall
    // along its free outline, the rows stopped short of it, and keeps it where the judge
    // (prop/judge.js) says it holds more -- at JUDGE.price, held area per plastic added: the
    // curved edges rows can't reach, not a sliver for a wall. (Regions an earlier race swapped --
    // raster, circular -- keep their walls.)
    let swapped: IndexSet<usize> = circ
        .props
        .iter()
        .filter(|q| q.circular == Some(true))
        .filter_map(|q| q.region)
        .collect();
    let base = if opts.rim_walls == Some(false) || opts.raster == Some(false) {
        circ
    } else {
        let ruler = judge_ruler(topo, result, rot, &judge_cache, cfg);
        race_against(
            topo,
            result,
            rot,
            opts,
            circ,
            cap_from,
            &|regions| {
                build_pass(
                    topo,
                    result,
                    rot,
                    &BuildOpts {
                        only_regions: Some(regions.clone()),
                        rim_regions: Some(regions.clone()),
                        ..opts.clone()
                    },
                    None,
                    ctx,
                )
            },
            RaceTag::Rim,
            RaceArgs {
                rel: 0.0,
                tried: Some(
                    (0..result.regions.len())
                        .filter(|i| !swapped.contains(i))
                        .collect(),
                ),
                ruler: Some(&ruler),
                price: Some(cfg.judge.price),
                ..Default::default()
            },
            ctx,
        )
    };
    // Downhill walls (issue #25): walls that run down the overhang and follow it to the plate,
    // raced per region against what the passes above built -- a region swaps only when they hold
    // more. opts.downhill === true only: Auto turns it on and judges the whole build
    // (fins/downhill.js); a direct call is unchanged.
    if opts.downhill != Some(true) {
        return base;
    }
    let off0 = &result.offset;
    // A downhill wall starts at the region's top edge, where the overhang meets the part: a flank
    // lying exactly flush with a part face there reads as clear to the weld check and the station
    // probes (lbracket Y20 on the plugin path: 45 flank vertices inside the part). Any wall
    // vertex inside the part drops the wall -- the wall's own triangles (`wallRange`), not its
    // tines, which bite in by design.
    let unburied = |mut pass: PropsPass| -> PropsPass {
        let tris = &pass.triangles;
        pass.props.retain(|q| {
            let Some([a, b]) = q.wall_range else {
                return true;
            };
            for v in &tris[a..b] {
                if inside_part(topo, rot, off0, v[0], v[1], v[2]) {
                    return false;
                }
            }
            true
        });
        pass
    };
    let strict = race_against(
        topo,
        result,
        rot,
        opts,
        base,
        cap_from,
        &|regions| {
            unburied(build_pass(
                topo,
                result,
                rot,
                &BuildOpts {
                    curved_walls: None,
                    downhill_regions: Some(regions.clone()),
                    ..opts.clone()
                },
                None,
                ctx,
            ))
        },
        RaceTag::Downhill,
        RaceArgs {
            max_grow: Some(cfg.downhill.max_grow),
            rel: cfg.downhill.race,
            ..Default::default()
        },
        ctx,
    );
    if opts.curved_walls != Some(true) {
        return strict;
    }
    // Curved walls (prop/curved.js, prop/trace.js): a second race over the regions grown across
    // the slivers, where a group whose walls bend always swaps to them and every other group keeps
    // what the strict race left it -- so ticking changes nothing where no wall bends. Its walls
    // are downhill walls too (`downhill`), raced under their own tag so the tine capture
    // re-points each wall's own.
    let bend = |qb: &[&PropRecord]| bends(qb.iter().map(|q| q.line.as_slice()), cfg);
    race_against(
        topo,
        result,
        rot,
        opts,
        strict,
        cap_from,
        &|regions| {
            let mut pass = unburied(build_pass(
                topo,
                result,
                rot,
                &BuildOpts {
                    downhill_regions: Some(regions.clone()),
                    ..opts.clone()
                },
                None,
                ctx,
            ));
            for q in pass.props.iter_mut() {
                q.downhill = Some(true);
            }
            pass
        },
        RaceTag::Curved,
        RaceArgs {
            rel: cfg.downhill.race,
            lenient: Some(&bend),
            ..Default::default()
        },
        ctx,
    )
}

/// Port of `buildShortWalls` (web/prop.js:207-209): the last-resort pass (web/fins/shortwalls.js):
/// the normal placement for `regions` only, with the short-wall floor (minSpanFor `short`). No
/// raster race -- these are regions every other pass already left bare.
pub fn build_short_walls(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    regions: &IndexSet<usize>,
    ctx: &BuildCtx,
) -> PropsPass {
    build_pass(
        topo,
        result,
        rot,
        &BuildOpts {
            only_regions: Some(regions.clone()),
            short_walls: true,
            ..opts.clone()
        },
        None,
        ctx,
    )
}
