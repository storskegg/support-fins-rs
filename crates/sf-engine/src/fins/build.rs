//! Port of `web/fins.js` (`buildFins`, `applyTunables` lives in `crate::config`). Read `docs/AGENTS.md` before changing anything here.

use indexmap::IndexMap;

use crate::fins::pad::PadStyle;
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

/// `built.pad`, the stats object `buildPad` returns (web/fins/pad.js:249, 520): the sure-hold
/// oval carries `autoSure`, `smallFoot` and `outline` (the light/custom one gets `smallFoot` and
/// `outline` from `tag`, never `autoSure`).
#[derive(Clone, Debug, PartialEq)]
pub struct PadInfo {
    pub r1: f64,
    pub r2: f64,
    pub cells: usize,
    pub height: f64,
    pub points: usize,
    pub oval: bool,
    pub style: PadStyle,
    pub auto_sure: Option<bool>,
    pub small_foot: Option<bool>,
    pub outline: Option<usize>,
}

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
    pub ribs: usize,
    pub ties: usize,
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
