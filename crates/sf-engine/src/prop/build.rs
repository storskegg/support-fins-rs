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
    /// `region` (all pushes).
    pub region: usize,
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
/// `buildProps` itself adds `circularFrames` (the circular regions); that field joins this
/// struct with the circular module.
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
