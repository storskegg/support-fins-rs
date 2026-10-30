//! Port of `web/fins/fill.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! FULL COVERAGE -- the fill pass (local issue 036; Matthew 2026-10-05: "if we have red overhang we
//! have to have something, somehow"). Runs after Auto's walls, wedges and last resort, in mode
//! 'full' only, so Auto is unchanged.
//!
//! What it aims at is the coverage scoreboard's number: the MUST red (coverage.js classify -- every
//! overhang face but a tiny hole's ceiling and the near-plate strip) that no wall top reaches
//! (coverage.js reachOf). That red is sampled into points, and walls are added one at a time, each
//! built by Draw's builder (draw.js drawnWall -- part-attached, plate, squat fallback, tines: the
//! path a hand-drawn wall takes, which is the reliable one).
//!
//! CONNECTED, like a slicer's support areas. A slicer grows each layer's overhang and merges what
//! touches, so islands a few mm apart share one support; a mini drawn by hand took one long wall top
//! to bottom across several red patches and printed well. So the points are grouped by DISTANCE
//! (FILL.link in plan, and not one above the other), not by region, and a wall's line may cross a
//! gap of up to FILL.bridge between patches -- one longer wall instead of a stub per patch.
//!
//! THE LINE. Not a PCA axis (a square patch's eigenvector snaps to 45deg -- the old auto-placer's
//! diagonal walls): FILL.dirs directions are each slid across the group, and the line that keeps the
//! most red area within reach wins, x or y on a near-tie; the best few are also tried shifted a few
//! mm across themselves, and every direction at its heaviest FILL.narrow band (a hem ring: a line
//! that hugs its edge, not one across the leg inside it). Its ends are the outermost red it serves.
//! When no line across a group builds, it is tried patch by patch around its densest red
//! (FILL.local): an upright figure's group is a hem, both arms and the hands at one height, and every
//! line across all that met the first wall.
//!
//! ROWS FIRST (opts.longWalls, the Long walls box, on unless false): before any of that, rows of long
//! walls along the model's length (fins/rows.js, fins/field.js), the way a wall drawn by hand runs --
//! the patch lines here then take what the rows leave. Not with the slider under FILL.sparseBelow
//! (one wall per group is its own promise).
//!
//! THE SLIDER. Wide-face coverage (opts.coverage) sets how much gets added, so there is no
//! wall-count field: below FILL.sparseBelow every group gets at most ONE wall ("something" under
//! every red patch); from there up, walls are added until every point is within reach, the reach
//! tightening from maxUnsupportedSpan/2 (6 mm, the scoreboard's held rule) at the middle to
//! FILL.denseReach at the right.
//!
//! ISLAND STARTS (opts.islands, web/islands.js; local issue 041): a boss end or a peg tip whose first
//! layer starts in midair, no face there red. LAST, once the red has every wall it gets (taken in the
//! same pass, they cost the red room: a snowman mini lost 4.6 points), with its own FILL.islandWalls
//! budget past the caps. Each start's points are targets [x, y, z, w, island], FILL.islandW a point,
//! so a wall under one alone passes minGain. A start is held only by a wall top within
//! FILL.islandReach under it, not the 6 mm bridging reach: a layer that begins in midair can't bridge
//! to a wall beside it. Not counted in the bare mm2; a start left bare is returned
//! (`unserved.islands`), like the red.
//!
//! GUARDRAILS. A wall is kept only if it holds new red and keeps PROP.sideClear off every support
//! already built (fused walls don't break away) -- from its floor up: a wall on the part (a chin over
//! the chest) has no plate foot to keep clear. FILL.maxWalls, maxTries and maxChecks bound the work
//! -- counts, not a clock, so the output is the same on every machine. Red no wall can reach is
//! returned (`unserved`), never hidden: quality first, surface the dropped overhang.

use crate::guards::{JsSort, cmp_diff};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use indexmap::{IndexMap, IndexSet};
use rayon::prelude::*;

use crate::config::Config;
use crate::draw::{DRAW_MIN_LEN, DrawOpts, DrawnOk, DrawnWall, drawn_line, drawn_wall};
use crate::fins::build::{FinKind, FinsBuilt};
use crate::fins::coverage::{
    MUST, PLATE_Z, Reach, Targets, classify_targets, reach_of, touches_small_hole,
};
use crate::fins::field::heading_field;
use crate::fins::rows::{RowWallsArgs, row_walls};
use crate::fins::seating::seated_part_tris;
use crate::islands::Island;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::attached::PART_BAND;
use crate::prop::brace::{BraceKinds, brace_wall};
use crate::prop::ctx::BuildCtx;
use crate::prop::surface::surface_hits_at;
use crate::prop::sweep::foot_for;
use crate::prop::ties::tie_walls;
use crate::tris::Tris;
use crate::types::{Rot3, V4};

type V3 = [f64; 3];

/// `FILL` (web/fins/fill.js:66). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct FillConfig {
    // mm: leftover red is merged into points this far apart
    pub sample: f64,
    // mm in plan: points this close are one group (maxUnsupportedSpan)
    pub link: f64,
    // mm: each direction also tried at its heaviest band this narrow (a hem ring's edge)
    pub narrow: f64,
    // mm: a group no line across builds on is tried patch by patch, this radius...
    pub local: f64,
    /// JS key `localZ`.
    // ...and this far up or down (a hem, a chin, a hand)
    pub local_z: f64,
    // mm: a line may cross this much plan gap between red it serves
    pub bridge: f64,
    // line directions tried, 180 / dirs apart
    pub dirs: f64,
    /// JS key `axisBias`.
    // a diagonal must serve 1/0.97 x an x/y line's red to beat it
    pub axis_bias: f64,
    /// JS key `bestDirs`.
    // directions tried per group, best first...
    pub best_dirs: f64,
    // ...each also this many mm across itself (off a wall's foot)
    pub shifts: Vec<f64>,
    /// JS key `sparseBelow`.
    // coverage under this: one wall per group, then stop
    pub sparse_below: f64,
    /// JS key `denseReach`.
    // mm: the reach at coverage 1 (the middle is maxUnsupportedSpan/2)
    pub dense_reach: f64,
    /// JS key `minGain`.
    // mm2 of new red a wall must hold to be kept
    pub min_gain: f64,
    /// JS key `islandReach`.
    // mm in plan: a wall top this close under an island start holds it
    pub island_reach: f64,
    /// JS key `islandW`.
    // weight of one island point (a 1 mm cell of red), so one alone passes minGain
    pub island_w: f64,
    /// JS key `islandWalls`.
    // the island pass's walls past maxWalls (and 3x that in tries, 50x in checks)
    pub island_walls: f64,
    /// JS key `maxStep`.
    // mm: a line's top may climb or drop this much between stations
    pub max_step: f64,
    /// JS key `redNear`.
    // mm: a station this close to leftover red counts toward its stretch
    pub red_near: f64,
    /// JS key `maxWalls`.
    pub max_walls: f64,
    /// JS key `maxTries`.
    // drawnWall calls, kept or not
    pub max_tries: f64,
    /// JS key `groupTries`.
    // drawnWall calls one group gets before its red is given up on
    pub group_tries: f64,
    /// JS key `maxChecks`.
    // line checks (underStretch, then settledStretch; both gridded), built or not
    pub max_checks: f64,
}

impl Default for FillConfig {
    fn default() -> Self {
        FillConfig {
            sample: 1.0,
            link: 12.0,
            narrow: 1.0,
            local: 6.0,
            local_z: 3.0,
            bridge: 8.0,
            dirs: 12.0,
            axis_bias: 0.97,
            best_dirs: 3.0,
            shifts: vec![-2.0, 2.0, -4.0, 4.0],
            sparse_below: 0.25,
            dense_reach: 4.0,
            min_gain: 0.5,
            island_reach: 2.0,
            island_w: 1.0,
            island_walls: 20.0,
            max_step: 2.5,
            red_near: 1.5,
            max_walls: 40.0,
            max_tries: 120.0,
            group_tries: 12.0,
            max_checks: 2000.0,
        }
    }
}

/// `FLANGE_TOP` (web/fins/fill.js:440): the plate flange's band, a hair over its top. JS computes it
/// once when the module loads, from `PROP.baseH`'s initial value.
pub fn flange_top() -> f64 {
    crate::prop::config::PropConfig::default().base_h + 0.05
}

/// Port of `fillReach` (web/fins/fill.js:92-96): the reach the slider asks for: R(0.5) = 6 mm (the
/// held rule), R(1) = denseReach.
pub fn fill_reach(coverage: f64, cfg: &Config) -> f64 {
    let r0 = cfg.prop.max_unsupported_span / 2.0;
    if coverage <= 0.5 {
        return r0;
    }
    r0 + (cfg.fill.dense_reach - r0) * jsnum::min(1.0, (coverage - 0.5) / 0.5)
}

/// A target point: JS `[x, y, z, area]`, or `[x, y, z, w, island]` for an island start (`p.length >
/// 4`). `id` is the point's identity (JS compares the arrays themselves): the index into the pass's
/// list of every target.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pt {
    x: f64,
    y: f64,
    z: f64,
    w: f64,
    island: Option<usize>,
    id: usize,
}

/// A float as a hashable key where JS would join it into a string: equal numbers (+0 and -0 too)
/// give equal keys, every NaN one key.
fn kb(f: f64) -> u64 {
    if f == 0.0 {
        0
    } else if f.is_nan() {
        u64::MAX
    } else {
        f.to_bits()
    }
}

/// Port of `leftoverPoints` (web/fins/fill.js:104-137): leftover MUST red as points `[x, y, z,
/// area]`, merged into FILL.sample cells. A big CAD triangle is subdivided first (a cube face is two
/// triangles: its centroid alone would call the whole face held or bare). `regions` (`None`: all)
/// keeps only those regions' faces.
fn leftover_points(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    near: &dyn Fn(f64, f64, f64) -> bool,
    cls: &[u8],
    regions: Option<&IndexSet<usize>>,
    cfg: &Config,
) -> Vec<[f64; 4]> {
    let mut cells = Cells::default();
    each_leftover_point(topo, result, rot, cls, regions, cfg, &mut |x, y, z, a| {
        if z < PLATE_Z || near(x, y, z) {
            return;
        }
        cells.put(cfg.fill.sample, x, y, z, a);
    });
    cells.points()
}

/// `leftoverPoints`' FILL.sample cells: the points of one cell merged into their area-weighted
/// centroid, cells in first-touched order.
#[derive(Default)]
struct Cells(IndexMap<[u64; 3], [f64; 4], crate::fxhash::FxBuild>);

impl Cells {
    fn put(&mut self, s: f64, x: f64, y: f64, z: f64, a: f64) {
        let k = [
            kb(jsnum::floor(x / s)),
            kb(jsnum::floor(y / s)),
            kb(jsnum::floor(z / s)),
        ];
        if let Some(c) = self.0.get_mut(&k) {
            c[0] += x * a;
            c[1] += y * a;
            c[2] += z * a;
            c[3] += a;
        } else {
            self.0.insert(k, [x * a, y * a, z * a, a]);
        }
    }
    fn points(&self) -> Vec<[f64; 4]> {
        self.0
            .values()
            .map(|c| [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]])
            .collect()
    }
}

/// The faces `leftoverPoints` walks, in face order: the leftover MUST red faces (`regions`: only
/// those regions').
fn leftover_faces(
    topo: &Topology,
    result: &Targets,
    cls: &[u8],
    regions: Option<&IndexSet<usize>>,
) -> Vec<usize> {
    let only: Option<Vec<bool>> = regions.map(|rs| {
        let mut only = vec![false; topo.n_faces];
        for &ri in rs {
            if let Some(g) = result.analysis.regions.get(ri) {
                for &f in &g.faces {
                    only[f] = true;
                }
            }
        }
        only
    });
    (0..topo.n_faces)
        .filter(|&f| {
            result.analysis.over[f] != 0 && cls[f] == MUST && only.as_ref().is_none_or(|o| o[f])
        })
        .collect()
}

/// One face's sub-triangle centroids, `put(x, y, z, area)` in the order JS visits them
/// (web/fins/fill.js:104-137): the face subdivided into n x n sub-triangles.
///
/// `spent` counts the sub-triangles of the walk this face is part of (X-07; faces of 256 or more): a walk over
/// `cfg.limits.fill_points` unwinds with an [`OverBudget`](crate::guards::OverBudget) before this face makes any.
fn face_points(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    cfg: &Config,
    f: usize,
    spent: &AtomicU64,
    put: &mut dyn FnMut(f64, f64, f64, f64),
) {
    let pos = &topo.pos;
    let off = &result.analysis.offset;
    let seat = |o: usize| -> V3 {
        [
            rot[0] * pos[o] + rot[3] * pos[o + 1] + rot[6] * pos[o + 2] + off.x,
            rot[1] * pos[o] + rot[4] * pos[o + 1] + rot[7] * pos[o + 2] + off.y,
            rot[2] * pos[o] + rot[5] * pos[o + 1] + rot[8] * pos[o + 2] + off.z,
        ]
    };
    let s = cfg.fill.sample;
    let a_ = seat(f * 9);
    let b_ = seat(f * 9 + 3);
    let c_ = seat(f * 9 + 6);
    let edge = jsnum::max(
        jsnum::max(
            jsnum::hypot(&[b_[0] - a_[0], b_[1] - a_[1], b_[2] - a_[2]]),
            jsnum::hypot(&[c_[0] - b_[0], c_[1] - b_[1], c_[2] - b_[2]]),
        ),
        jsnum::hypot(&[a_[0] - c_[0], a_[1] - c_[1], a_[2] - c_[2]]),
    );
    let n_f = jsnum::max(1.0, jsnum::ceil(edge / s));
    // X-07: a face of fewer than SMALL_FACE sub-triangles costs O(1), so only bigger ones (where a part loaded
    // anyway runs away: 7e12 for one face of the 7.6 km coupon) are counted against the walk's budget, and the
    // walks over ordinary meshes of small faces never touch the shared counter.
    const SMALL_FACE: f64 = 256.0;
    let mine = n_f * n_f;
    if mine.is_nan() || mine >= SMALL_FACE {
        let budget = cfg.limits.fill_points;
        let over = |need: f64| crate::guards::OverBudget {
            what: "overhang",
            need,
            budget,
            across_mm: edge,
        };
        // (`mine` alone first: the walk's sum fits a u64 only while each face's does)
        if mine.is_nan() || mine > budget || n_f >= crate::guards::EXACT_INTEGERS {
            over(if n_f < crate::guards::EXACT_INTEGERS {
                mine
            } else {
                f64::INFINITY
            })
            .raise();
        }
        let total =
            spent.fetch_add(mine as u64, std::sync::atomic::Ordering::Relaxed) as f64 + mine;
        if total > budget {
            over(total).raise();
        }
    }
    let n = n_f as usize;
    let a = topo.area[f] / ((n * n) as f64);
    // n x n sub-triangles: centroids on the barycentric grid (with a cancel poll every 4096 of them: X-07)
    let mut polls: u32 = 0;
    for i in 0..n {
        let mut j = 0;
        while i + j < n {
            polls = polls.wrapping_add(1);
            if polls & 4095 == 0 {
                cfg.cancel.check();
            }
            let (fi, fj) = (i as f64, j as f64);
            let uv = [
                [fi + 1.0 / 3.0, fj + 1.0 / 3.0],
                [fi + 2.0 / 3.0, fj + 2.0 / 3.0],
            ];
            for &[u, v] in &uv[..if i + j < n - 1 { 2 } else { 1 }] {
                let bu = u / n as f64;
                let bv = v / n as f64;
                let bw = 1.0 - bu - bv;
                put(
                    bw * a_[0] + bu * b_[0] + bv * c_[0],
                    bw * a_[1] + bu * b_[1] + bv * c_[1],
                    bw * a_[2] + bu * b_[2] + bv * c_[2],
                    a,
                );
            }
            j += 1;
        }
    }
}

/// The sub-triangle centroids of `leftoverPoints` (web/fins/fill.js:104-137), as `f(x, y, z, area)`
/// in the order JS visits them.
fn each_leftover_point(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    cls: &[u8],
    regions: Option<&IndexSet<usize>>,
    cfg: &Config,
    put: &mut dyn FnMut(f64, f64, f64, f64),
) {
    let spent = AtomicU64::new(0);
    for f in leftover_faces(topo, result, cls, regions) {
        cfg.cancel.check();
        face_points(topo, result, rot, cfg, f, &spent, put);
    }
}

/// `bareArea`'s two `leftoverPoints` sums in one walk of the faces: (the area the near set leaves
/// bare, all the MUST red). Each is the sum of its cells' areas in first-touched order, exactly as
/// two separate passes would fold them (the near pass keeps a subset of the points, in order).
fn bare_sums(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    near: Option<&Reach>,
    cls: &[u8],
    cfg: &Config,
) -> (f64, f64) {
    let (mut all, mut far) = (Cells::default(), Cells::default());
    let faces = leftover_faces(topo, result, cls, None);
    let spent = AtomicU64::new(0);
    // The points of a block of faces, with whether the wall tops leave each one bare (the `near`
    // lookups are the cost, and pure): blocks are made in parallel and joined in face order, so
    // the cells below are filled in the order the serial walk fills them. [x, y, z, area, bare].
    let block = |fs: &[usize]| -> Vec<[f64; 5]> {
        let mut pts = Vec::new();
        for &f in fs {
            if cfg.cancel.is_cancelled() {
                break; // unused: the poll below unwinds
            }
            face_points(topo, result, rot, cfg, f, &spent, &mut |x, y, z, a| {
                if z < PLATE_Z {
                    return;
                }
                let bare = near.is_some_and(|r| !r.near(x, y, z));
                pts.push([x, y, z, a, f64::from(u8::from(bare))]);
            });
        }
        pts
    };
    let blocks: Vec<Vec<[f64; 5]>> = if faces.len() < 16 {
        vec![block(&faces)]
    } else {
        faces.par_chunks(4).map(block).collect()
    };
    cfg.cancel.check();
    for p in blocks.iter().flatten() {
        all.put(cfg.fill.sample, p[0], p[1], p[2], p[3]);
        if p[4] != 0.0 {
            far.put(cfg.fill.sample, p[0], p[1], p[2], p[3]);
        }
    }
    let sum = |c: &Cells| c.points().iter().fold(0.0, |s, p| s + p[3]);
    let total = sum(&all);
    (if near.is_some() { sum(&far) } else { total }, total)
}

/// Port of `islandPoints` (web/fins/fill.js:143-155): the island starts (findIslands) not yet held,
/// as targets `[x, y, z, FILL.islandW, k]` -- each start's points merged into FILL.sample cells, k
/// the start's index.
fn island_points(
    islands: &[Island],
    held: &dyn Fn(f64, f64, f64) -> bool,
    cfg: &Config,
) -> Vec<(V3, usize)> {
    let s = cfg.fill.sample;
    let mut out = Vec::new();
    for (k, isl) in islands.iter().enumerate() {
        let mut cells: HashSet<[u64; 3]> = HashSet::new();
        for p in &isl.points {
            let c = [
                kb(jsnum::floor(p[0] / s)),
                kb(jsnum::floor(p[1] / s)),
                kb(jsnum::floor(p[2] / s)),
            ];
            if cells.contains(&c) || held(p[0], p[1], p[2]) {
                continue;
            }
            cells.insert(c);
            out.push((*p, k));
        }
    }
    out
}

/// `startsIn` (web/fins/fill.js:158): how many island starts the targets `pts` come from.
fn starts_in(pts: &[Pt]) -> usize {
    pts.iter()
        .filter_map(|p| p.island)
        .collect::<HashSet<_>>()
        .len()
}

/// Port of `groupsOf` (web/fins/fill.js:165-187): groups of points (index arrays): linked when
/// within FILL.link in plan and no steeper than 45deg apart (plus a mm) -- red straight above other
/// red is a separate overhang, held by its own wall on the part below.
fn groups_of(pts: &[Pt], cfg: &Config) -> Vec<Vec<usize>> {
    let l = cfg.fill.link;
    let mut grid: HashMap<i64, Vec<usize>> = HashMap::new();
    let key = |i: f64, j: f64| (i * 100003.0 + j) as i64;
    for (i, p) in pts.iter().enumerate() {
        grid.entry(key(jsnum::floor(p.x / l), jsnum::floor(p.y / l)))
            .or_default()
            .push(i);
    }
    let mut parent: Vec<usize> = (0..pts.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            let g = parent[parent[i]];
            parent[i] = g;
            i = g;
        }
        i
    }
    for (i, p) in pts.iter().enumerate() {
        let gi = jsnum::floor(p.x / l);
        let gj = jsnum::floor(p.y / l);
        for di in -1..=1 {
            for dj in -1..=1 {
                let Some(cell) = grid.get(&key(gi + di as f64, gj + dj as f64)) else {
                    continue;
                };
                for &j in cell {
                    if j <= i {
                        continue;
                    }
                    let q = &pts[j];
                    let d = jsnum::hypot(&[p.x - q.x, p.y - q.y]);
                    if d <= l && jsnum::abs(p.z - q.z) <= d + 1.0 {
                        let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                        parent[a] = b;
                    }
                }
            }
        }
    }
    let mut by: IndexMap<usize, Vec<usize>> = IndexMap::new();
    for i in 0..pts.len() {
        let r = find(&mut parent, i);
        by.entry(r).or_default().push(i);
    }
    by.into_values().collect()
}

/// A candidate line: `{ a, b, weight, aims, ends }` (web/fins/fill.js:195-236, 240-270). `a`, `b` are
/// `[x, y, zHint]`; `aims` the indices it serves; `ends` the lines from an island start's end.
#[derive(Clone, Debug)]
struct Cand {
    a: V3,
    b: V3,
    weight: f64,
    aims: Vec<usize>,
    ends: Option<Vec<Cand>>,
}

/// Port of `candidateLines` (web/fins/fill.js:195-236): candidate lines for a group, best first: for
/// each direction, the offset whose band (+-R across) holds the most red, split where the served red
/// has a gap wider than `bridge` along the line, the heaviest run kept.
fn candidate_lines(
    pts: &[Pt],
    group: &[usize],
    r: f64,
    bridge: f64,
    narrow: bool,
    cfg: &Config,
) -> Vec<Cand> {
    let fill = &cfg.fill;
    struct Dir {
        c: Cand,
        ux: f64,
        uy: f64,
        axis: bool,
        across: Vec<(f64, usize)>,
        best_at: f64,
    }
    let mut dirs: Vec<Dir> = Vec::new();
    let mut narrow_out: Vec<Cand> = Vec::new();
    let mut d = 0.0;
    while d < fill.dirs {
        let th = (std::f64::consts::PI * d) / fill.dirs;
        let ux = jsnum::cos(th);
        let uy = jsnum::sin(th);
        let axis = d == 0.0 || 2.0 * d == fill.dirs;
        d += 1.0;
        // slide a 2R window over the across-coordinate, heaviest window wins
        let mut across: Vec<(f64, usize)> = group
            .iter()
            .map(|&i| (-uy * pts[i].x + ux * pts[i].y, i))
            .collect();
        across.js_sort_by(|p, q| cmp_diff(p.0 - q.0));
        let (mut best, mut best_at, mut w, mut lo) = (-1.0, 0.0, 0.0, 0usize);
        for hi in 0..across.len() {
            w += pts[across[hi].1].w;
            while across[hi].0 - across[lo].0 > 2.0 * r {
                w -= pts[across[lo].1].w;
                lo += 1;
            }
            if w > best {
                best = w;
                best_at = (across[lo].0 + across[hi].0) / 2.0;
            }
        }
        let c = line_at(pts, &across, ux, uy, best_at, r, bridge, axis, cfg);
        if narrow {
            let n = fill.narrow;
            let (mut nb, mut n_at, mut w, mut lo) = (-1.0, 0.0, 0.0, 0usize);
            for hi in 0..across.len() {
                w += pts[across[hi].1].w;
                while across[hi].0 - across[lo].0 > 2.0 * n {
                    w -= pts[across[lo].1].w;
                    lo += 1;
                }
                if w > nb {
                    nb = w;
                    n_at = (across[lo].0 + across[hi].0) / 2.0;
                }
            }
            if let Some(v) = line_at(pts, &across, ux, uy, n_at, n, bridge, axis, cfg) {
                narrow_out.push(v);
            }
        }
        if let Some(c) = c {
            dirs.push(Dir {
                c,
                ux,
                uy,
                axis,
                across,
                best_at,
            });
        }
    }
    // the best directions, each also SHIFTED across itself: the heaviest band's centre
    // often sits ~5 mm off a wall already built, inside its foot (a tall wall's is 3 mm
    // a side), and a line a few mm over clears it
    dirs.js_sort_by(|p, q| cmp_diff(q.c.weight - p.c.weight));
    let mut out: Vec<Cand> = Vec::new();
    let take = {
        let n = jsnum::trunc(fill.best_dirs);
        if n < 0.0 {
            jsnum::max(0.0, dirs.len() as f64 + n) as usize
        } else {
            jsnum::min(n, dirs.len() as f64) as usize
        }
    };
    for dir in &dirs[..take] {
        out.push(dir.c.clone());
        for s in &fill.shifts {
            if let Some(v) = line_at(
                pts,
                &dir.across,
                dir.ux,
                dir.uy,
                dir.best_at + s,
                r,
                bridge,
                dir.axis,
                cfg,
            ) {
                out.push(v);
            }
        }
    }
    // all by the red they'd hold (stable: on a tie a direction's own line stays ahead of
    // its shifts; between directions, the better direction's lines come first), each
    // followed by its lines from an island start's end (lineAt's `ends`)
    out.js_sort_by(|p, q| cmp_diff(q.weight - p.weight));
    narrow_out.js_sort_by(|p, q| cmp_diff(q.weight - p.weight));
    out.extend(narrow_out);
    let mut flat = Vec::new();
    for c in out {
        let ends = c.ends.clone();
        flat.push(c);
        if let Some(e) = ends {
            flat.extend(e);
        }
    }
    flat
}

/// Port of `lineAt` (web/fins/fill.js:240-270): one candidate: the line along (ux, uy) at
/// across-offset `off`, over the red within R of it, split at plan gaps over `bridge` with the
/// heaviest run kept.
#[allow(clippy::too_many_arguments)]
fn line_at(
    pts: &[Pt],
    across: &[(f64, usize)],
    ux: f64,
    uy: f64,
    off: f64,
    r: f64,
    bridge: f64,
    axis: bool,
    cfg: &Config,
) -> Option<Cand> {
    let mut band: Vec<(f64, usize)> = across
        .iter()
        .filter(|(v, _)| jsnum::abs(v - off) <= r)
        .map(|&(_, i)| (ux * pts[i].x + uy * pts[i].y, i))
        .collect();
    band.js_sort_by(|p, q| cmp_diff(p.0 - q.0));
    #[derive(Clone, Copy)]
    struct Run {
        from: usize,
        to: usize,
        w: f64,
    }
    let mut run: Option<Run> = None;
    let mut cur = Run {
        from: 0,
        to: 0,
        w: 0.0,
    };
    for k in 0..band.len() {
        if k > 0 && band[k].0 - band[k - 1].0 > bridge {
            cur = Run {
                from: k,
                to: k,
                w: 0.0,
            };
        }
        cur.w += pts[band[k].1].w;
        cur.to = k;
        if run.is_none_or(|r| cur.w > r.w) {
            run = Some(cur);
        }
    }
    let run = run?;
    let mut t0 = band[run.from].0;
    let mut t1 = band[run.to].0;
    let short = t1 - t0 < DRAW_MIN_LEN + 0.5;
    let (s0, s1) = (t0, t1);
    if short {
        let m = (t0 + t1) / 2.0;
        t0 = m - (DRAW_MIN_LEN + 0.5) / 2.0;
        t1 = m + (DRAW_MIN_LEN + 0.5) / 2.0;
    }
    // z hint at each end: the red nearest that end
    let z_at = |t: f64| {
        let (mut z, mut bd) = (0.0, f64::INFINITY);
        for k in run.from..=run.to {
            let dd = jsnum::abs(band[k].0 - t);
            if dd < bd {
                bd = dd;
                z = pts[band[k].1].z;
            }
        }
        z
    };
    let at = |t: f64| -> V3 { [ux * t - uy * off, uy * t + ux * off, z_at(t)] };
    let aims: Vec<usize> = band[run.from..=run.to].iter().map(|&(_, i)| i).collect();
    let weight = run.w * if axis { 1.0 } else { cfg.fill.axis_bias };
    // an island start on a part's edge (an outside corner's tip): a line centred on it
    // is half in air, too short to build; one from the start each way along the line
    // keeps the side that runs up the part under it
    let ends = if short && aims.iter().all(|&i| pts[i].island.is_some()) {
        Some(
            [[s0, s0 + DRAW_MIN_LEN + 0.5], [s1 - DRAW_MIN_LEN - 0.5, s1]]
                .iter()
                .map(|&[u, v]| Cand {
                    a: at(u),
                    b: at(v),
                    weight,
                    aims: aims.clone(),
                    ends: None,
                })
                .collect(),
        )
    } else {
        None
    };
    Some(Cand {
        a: at(t0),
        b: at(t1),
        weight,
        aims,
        ends,
    })
}

/// Port of `lowest` (web/fins/fill.js:280): the lowest top a line's station may have. Pass 1 asks for
/// full walls (minHeight: the plate rows and squat walls below that are Auto's). Pass 2 (`low`, patch
/// by patch, after pass 1's walls are in) goes down to drawnWall's squat floor, minHeightSquat: a
/// figure's cleat soles at 1.2-2.4 mm over the plate that Auto dropped as slivers. Pass 1 at that
/// floor spent the 40-wall cap on low walls first and left a bust's chest bare.
fn lowest(low: bool, cfg: &Config) -> f64 {
    (if low {
        cfg.prop.min_height_squat
    } else {
        cfg.prop.min_height
    }) + cfg.prop.gap
}

/// `[minX, minY, minZ, maxX, maxY, maxZ]`.
type Box6 = [f64; 6];

/// `BOX_EDGE` (web/fins/fill.js:439): mm, largest support-triangle box boxGrid works with.
const BOX_EDGE: f64 = 2.0;

/// `triBox` (web/fins/fill.js:435-437): the box of one triangle.
fn tri_box(a: V3, b: V3, c: V3) -> Box6 {
    let mn = |i: usize| jsnum::min(jsnum::min(a[i], b[i]), c[i]);
    let mx = |i: usize| jsnum::max(jsnum::max(a[i], b[i]), c[i]);
    [mn(0), mn(1), mn(2), mx(0), mx(1), mx(2)]
}

/// Port of `boxesOf` (web/fins/fill.js:449-460): fn(box) for pieces of triangle (a, b, c) no more
/// than BOX_EDGE across: a long wall's triangles are big and, on a diagonal wall, so are their boxes
/// -- a box over a whole quadrant of the plan read every line near it as a hit. Split at the longest
/// edge's midpoint (a sliver halves along its length, not into 4^n), at most 16 deep.
fn boxes_of(mut a: V3, mut b: V3, mut c: V3, f: &mut dyn FnMut(Box6), depth: usize) {
    let bx = tri_box(a, b, c);
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let small = !(jsnum::max(jsnum::max(bx[3] - bx[0], bx[4] - bx[1]), bx[5] - bx[2]) > BOX_EDGE);
    if depth >= 16 || small {
        f(bx);
        return;
    }
    let d2 = |p: V3, q: V3| {
        jsnum::pow(p[0] - q[0], 2.0) + jsnum::pow(p[1] - q[1], 2.0) + jsnum::pow(p[2] - q[2], 2.0)
    };
    let ab = d2(a, b);
    let bc = d2(b, c);
    let ca = d2(c, a);
    // rotate so the longest edge is (a, b)
    if bc >= ab && bc >= ca {
        (a, b, c) = (b, c, a);
    } else if ca >= ab && ca >= bc {
        (a, b, c) = (c, a, b);
    }
    let m = [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ];
    boxes_of(a, m, c, f, depth + 1);
    boxes_of(m, b, c, f, depth + 1);
}

fn xyz(v: &V4) -> V3 {
    [v[0], v[1], v[2]]
}

/// A stored box and whose triangles it is: `Some(k)` is the tag `{ fill: true, k }` (fill wall k and
/// its ribs), `None` is Auto's (`null`).
type Tag = Option<usize>;

/// `isFill` (web/fins/fill.js:283): boxGrid `skip` for a joined wall: the fill pass's own walls
/// (never Auto's).
fn is_fill(tag: Tag) -> bool {
    tag.is_some()
}

/// Port of `boxGrid` (web/fins/fill.js:465-520): every support triangle's box, gridded in plan AND
/// height (a tall wall's boxes stack ~25 to a plan cell, and Auto's tines more: a plan-only grid made
/// each check walk them all, ~0.3 s a wall on a 250k-face bust), so "does this new wall touch one" is
/// cheap. The JS `Map` is only looked up, never iterated.
#[derive(Debug, Default)]
struct BoxGrid {
    boxes: Vec<(Box6, Tag)>,
    grid: HashMap<i64, Vec<usize>>,
}

const GC: f64 = 3.0;
const GCZ: f64 = 3.0;

/// 13 bits a cell index (+-12 m at 3 mm); a collision past that only adds candidates.
fn gkey(i: f64, j: f64, k: f64) -> i64 {
    (((i + 4096.0) * 8192.0 + (j + 4096.0)) * 8192.0 + (k + 4096.0)) as i64
}

fn gcells_of(b: &Box6, pad: f64, f: &mut dyn FnMut(i64)) {
    let mut i = jsnum::floor((b[0] - pad) / GC);
    while i <= jsnum::floor((b[3] + pad) / GC) {
        let mut j = jsnum::floor((b[1] - pad) / GC);
        while j <= jsnum::floor((b[4] + pad) / GC) {
            let mut k = jsnum::floor((b[2] - pad) / GCZ);
            while k <= jsnum::floor((b[5] + pad) / GCZ) {
                f(gkey(i, j, k));
                k += 1.0;
            }
            j += 1.0;
        }
        i += 1.0;
    }
}

impl BoxGrid {
    fn new() -> BoxGrid {
        BoxGrid::default()
    }

    /// `add(tris, tag = null)`: `tag` marks whose triangles these are, so a check can `skip(tag)`
    /// some -- a joined wall may meet another fill wall, never Auto's; a tie meets its own two walls.
    fn add(&mut self, tris: &[V4], tag: Tag) {
        let mut i = 0;
        while i + 2 < tris.len() {
            let (a, b, c) = (xyz(&tris[i]), xyz(&tris[i + 1]), xyz(&tris[i + 2]));
            i += 3;
            let boxes = &mut self.boxes;
            let grid = &mut self.grid;
            boxes_of(
                a,
                b,
                c,
                &mut |bx| {
                    let idx = boxes.len();
                    boxes.push((bx, tag));
                    gcells_of(&bx, 0.0, &mut |k| grid.entry(k).or_default().push(idx));
                },
                0,
            );
        }
    }

    /// `add` for triangles given as plain vertex triples (`result.excludedSupportVolumes`).
    fn add_v3(&mut self, tris: &[V3], tag: Tag) {
        let v4: Vec<V4> = tris.iter().map(|v| [v[0], v[1], v[2], 0.0]).collect();
        self.add(&v4, tag);
    }

    /// `hitsBox(b, skip = null)`.
    fn hits_box(&self, b: &Box6, skip: Option<&dyn Fn(Tag) -> bool>) -> bool {
        let mut hit = false;
        gcells_of(b, 0.0, &mut |k| {
            if hit {
                return;
            }
            for &oi in self.grid.get(&k).map(Vec::as_slice).unwrap_or(&[]) {
                let (o, tag) = &self.boxes[oi];
                if let Some(s) = skip
                    && s(*tag)
                {
                    continue;
                }
                if b[0] <= o[3]
                    && o[0] <= b[3]
                    && b[1] <= o[4]
                    && o[1] <= b[4]
                    && b[2] <= o[5]
                    && o[2] <= b[5]
                {
                    hit = true;
                    return;
                }
            }
        });
        hit
    }

    /// `hits(tris, pad, shared = false, skip = null)`. `shared`: two pieces that both start in the
    /// flange band, either one wholly within it (top at or under FLANGE_TOP), are the second pass's
    /// joined base, not a hit -- a base on the part, above the band, never is.
    fn hits(
        &self,
        tris: &[V4],
        pad: f64,
        shared: bool,
        skip: Option<&dyn Fn(Tag) -> bool>,
    ) -> bool {
        let flange_top = flange_top();
        let hit = std::cell::Cell::new(false);
        let test = |b: Box6| {
            if hit.get() {
                return;
            }
            gcells_of(&b, pad, &mut |k| {
                if hit.get() {
                    return;
                }
                for &oi in self.grid.get(&k).map(Vec::as_slice).unwrap_or(&[]) {
                    let (o, tag) = &self.boxes[oi];
                    if let Some(s) = skip
                        && s(*tag)
                    {
                        continue;
                    }
                    if b[0] - pad <= o[3]
                        && o[0] <= b[3] + pad
                        && b[1] - pad <= o[4]
                        && o[1] <= b[4] + pad
                        && b[2] - pad <= o[5]
                        && o[2] <= b[5] + pad
                        && !(shared
                            && jsnum::max(b[2], o[2]) <= flange_top
                            && jsnum::min(b[5], o[5]) <= flange_top)
                    {
                        hit.set(true);
                        return;
                    }
                }
            });
        };
        // the new wall's triangles split the same way as the stored ones
        let mut i = 0;
        while i + 2 < tris.len() && !hit.get() {
            boxes_of(
                xyz(&tris[i]),
                xyz(&tris[i + 1]),
                xyz(&tris[i + 2]),
                &mut |b| test(b),
                0,
            );
            i += 3;
        }
        hit.get()
    }
}

/// `relax` of the stretch checks (web/fins/fill.js:370-381): each part on its own: `part` -- where
/// part is under the station (a chin over the chest), from that part up, where drawnWall stands it
/// (part-attached); `flange` -- on the plate, its flange may JOIN the flanges already there (one base
/// under a crowded figure, peeled off with all its walls) and only the stem above FLANGE_TOP keeps
/// clear; `join` -- the fill pass's own walls don't count at all (a wall that crosses or butts into
/// another fill wall is one support with it: a cleat sole at 45deg needs walls down its slope, across
/// the one along it). Auto's walls always do. `plate` (Plate only, #218, on every try): any part
/// between the station and the plate refuses it here, before drawnWall is asked and refuses it at
/// length. `None` is JS `null`.
#[derive(Clone, Copy, Debug, Default)]
struct Relax {
    part: bool,
    flange: bool,
    join: bool,
    plate: bool,
}

/// A stretch `{ a, b, red }` (`a`, `b` surface points).
#[derive(Clone, Copy, Debug)]
struct Pick {
    a: V3,
    b: V3,
    red: usize,
}

/// Port of `stationClear` (web/fins/fill.js:382-401): does the wall at station p (its top, [x, y, z])
/// keep PROP.sideClear off every support already built? Its stem runs from its floor to its top, with
/// a foot footFor(h) a side at the plate. See [`Relax`].
fn station_clear(p: V3, tris: &Tris, others: &BoxGrid, relax: Option<Relax>, cfg: &Config) -> bool {
    let prop = &cfg.prop;
    let w = prop.th / 2.0 + prop.side_clear;
    let fill_only: &dyn Fn(Tag) -> bool = &is_fill;
    let skip: Option<&dyn Fn(Tag) -> bool> = if relax.is_some_and(|r| r.join) {
        Some(fill_only)
    } else {
        None
    };
    if relax.is_some_and(|r| r.plate) {
        for h in surface_hits_at(tris, p[0], p[1]) {
            if h[0] < p[2] - 1.0 && h[0] > prop.gap + 0.5 {
                return false;
            }
        }
    }
    if relax.is_some_and(|r| r.part) {
        // the nearest surface below, if it faces UP (a down-facing one below is another
        // overhang's underside, with air under it)
        let mut below: Option<[f64; 2]> = None;
        for h in surface_hits_at(tris, p[0], p[1]) {
            if h[0] < p[2] - 1.0 && below.is_none_or(|b| h[0] > b[0]) {
                below = Some(h);
            }
        }
        if let Some(b) = below
            && b[1] > 0.0
            && b[0] > prop.gap + 0.5
        {
            return !others.hits_box(&[p[0] - w, p[1] - w, b[0], p[0] + w, p[1] + w, p[2]], skip);
        }
    }
    if relax.is_some_and(|r| r.flange) {
        return !others.hits_box(
            &[p[0] - w, p[1] - w, flange_top(), p[0] + w, p[1] + w, p[2]],
            skip,
        );
    }
    let wf = foot_for(p[2] - prop.gap, cfg) + prop.side_clear;
    !others.hits_box(
        &[p[0] - w, p[1] - w, prop.base_h, p[0] + w, p[1] + w, p[2]],
        skip,
    ) && !others.hits_box(
        &[
            p[0] - wf,
            p[1] - wf,
            0.0,
            p[0] + wf,
            p[1] + wf,
            prop.base_h + prop.gap,
        ],
        skip,
    )
}

/// Port of `underStretch` (web/fins/fill.js:297-329): the stretch of candidate line `c` a wall can
/// follow, checked BEFORE drawnWall (which costs ~100 ms on a 250k-face part): stations every
/// PROP.stationStep at the surface nearest the line's height there (drawnLine's pick, through prop's
/// gridded surfaceHitsAt), cut wherever that surface faces UP (not an underside), or the top jumps
/// more than FILL.maxStep (a line grouped across a figure ran over its head: the top climbed to 32 mm
/// on a 32 mm mini), or the station's own wall would come within sideClear of a support already built
/// (`others`), so a line across an Auto wall splits there instead of fusing to it. Of the stretches
/// left, the one passing most leftover red (`redAt`), as { a, b } surface points; `None` when none is
/// DRAW_MIN_LEN long. Between patches the underside still carries a bridge.
fn under_stretch(
    c: (V3, V3),
    tris: &Tris,
    red_at: &RedGrid,
    others: &BoxGrid,
    relax: Option<Relax>,
    low: bool,
    cfg: &Config,
) -> Option<Pick> {
    let prop = &cfg.prop;
    let (ca, cb) = c;
    let dx = cb[0] - ca[0];
    let dy = cb[1] - ca[1];
    let len = jsnum::hypot(&[dx, dy]);
    let n = jsnum::max(
        prop.min_stations - 1.0,
        jsnum::ceil(len / prop.station_step),
    );
    let mut top: Vec<Option<[f64; 4]>> = Vec::new();
    let mut k = 0.0;
    while k <= n {
        let t = k / n;
        let x = ca[0] + dx * t;
        let y = ca[1] + dy * t;
        let hint = ca[2] + (cb[2] - ca[2]) * t;
        let mut hit: Option<[f64; 2]> = None;
        // the underside nearest the line's height here (drawnLine's `under` pick)
        for h in surface_hits_at(tris, x, y) {
            if h[1] < 0.0 && hit.is_none_or(|b| jsnum::abs(h[0] - hint) < jsnum::abs(b[0] - hint)) {
                hit = Some(h);
            }
        }
        top.push(hit.map(|h| [x, y, h[0], h[1]]));
        k += 1.0;
    }
    let ok: Vec<bool> = top
        .iter()
        .map(|p| {
            // too low for the wall asked for (`low`, settledStretch's): drawnWall refuses a
            // whole line for one station under minHeightSquat (a line down a cube's 40deg
            // underside to its plate edge)
            let Some(p) = p else { return false };
            if p[3] >= 0.0 || p[2] < lowest(low, cfg) {
                return false;
            }
            station_clear([p[0], p[1], p[2]], tris, others, relax, cfg)
        })
        .collect();
    let mut pick: Option<Pick> = None;
    let mut from: i64 = -1;
    let mut red = 0usize;
    let close = |to: i64, from: i64, red: usize, pick: &mut Option<Pick>| {
        if from < 0 || to < from {
            return;
        }
        let (f, t) = (top[from as usize].unwrap(), top[to as usize].unwrap());
        let l = jsnum::hypot(&[t[0] - f[0], t[1] - f[1]]);
        if l >= DRAW_MIN_LEN && red > 0 && pick.is_none_or(|p| red > p.red) {
            *pick = Some(Pick {
                a: [f[0], f[1], f[2]],
                b: [t[0], t[1], t[2]],
                red,
            });
        }
    };
    for i in 0..top.len() {
        let ii = i as i64;
        if !ok[i] {
            close(ii - 1, from, red, &mut pick);
            from = -1;
            continue;
        }
        let ti = top[i].unwrap();
        if from >= 0 && jsnum::abs(ti[2] - top[i - 1].unwrap()[2]) > cfg.fill.max_step {
            close(ii - 1, from, red, &mut pick);
            from = -1;
        }
        if from < 0 {
            from = ii;
            red = 0;
        }
        if red_at.at([ti[0], ti[1], ti[2]]) {
            red += 1;
        }
    }
    close(top.len() as i64 - 1, from, red, &mut pick);
    pick
}

/// Port of `settledStretch` (web/fins/fill.js:342-366): the same cut on drawnLine's SETTLED stations
/// -- the top drawnWall actually builds to, which the raw stations above only approximate (on a thin
/// chair shell the raw stretch's ends sent drawnWall to refuse three walls in four) -- cut wherever a
/// station's surface faces UP or the top jumps more than FILL.maxStep -- a line grouped across a
/// figure ran over its head (the top climbed to 32 mm on a 32 mm mini) -- or where the station's own
/// wall would come within sideClear of a support already built (`others`), so a line across an Auto
/// wall splits there instead of fusing to it. Of the stretches left, the one passing most leftover red
/// (`redAt`), as { a, b } surface points; `None` when none is DRAW_MIN_LEN long. Between patches the
/// underside still carries a bridge.
fn settled_stretch(
    c: &Pick,
    tris: &Tris,
    red_at: &RedGrid,
    others: &BoxGrid,
    relax: Option<Relax>,
    low: bool,
    cfg: &Config,
) -> Option<Pick> {
    let top = drawn_line(
        c.a,
        c.b,
        tris,
        cfg.prop.station_step,
        PART_BAND,
        true,
        None,
        cfg,
    )?;
    let down: Vec<bool> = top
        .iter()
        .map(|p| {
            let mut best: Option<[f64; 2]> = None;
            for h in surface_hits_at(tris, p[0], p[1]) {
                let d = jsnum::abs(h[0] - p[2]);
                if d < 1.0 && best.is_none_or(|b| d < jsnum::abs(b[0] - p[2])) {
                    best = Some(h);
                }
            }
            let Some(best) = best else { return false };
            if best[1] >= 0.0 || p[2] < lowest(low, cfg) {
                return false;
            }
            station_clear(*p, tris, others, relax, cfg)
        })
        .collect();
    let mut pick: Option<Pick> = None;
    let mut from = 0usize;
    let mut red = 0usize;
    for i in 0..=top.len() {
        let breaks = i == top.len()
            || !down[i]
            || (i > from && jsnum::abs(top[i][2] - top[i - 1][2]) > cfg.fill.max_step);
        if breaks {
            let len = if i > from {
                jsnum::hypot(&[top[i - 1][0] - top[from][0], top[i - 1][1] - top[from][1]])
            } else {
                0.0
            };
            if len >= DRAW_MIN_LEN && red > 0 && pick.is_none_or(|p| red > p.red) {
                pick = Some(Pick {
                    a: top[from],
                    b: top[i - 1],
                    red,
                });
            }
            // a jump starts the next stretch at this station; a face looking up skips it
            from = if i < top.len() && down[i] { i } else { i + 1 };
            red = if from == i && i < top.len() && red_at.at(top[i]) {
                1
            } else {
                0
            };
        } else if red_at.at(top[i]) {
            red += 1;
        }
    }
    pick
}

/// Port of `localPatch` (web/fins/fill.js:405-413): the points of group g (indices into pts) within
/// FILL.local in plan and FILL.localZ in height of its densest point (by a coarse grid: a group can
/// be thousands of points).
fn local_patch(pts: &[Pt], g: &[usize], cfg: &Config) -> Vec<usize> {
    let l = cfg.fill.local;
    let lz = cfg.fill.local_z;
    let key = |p: &Pt| {
        [
            kb(jsnum::floor(p.x / l)),
            kb(jsnum::floor(p.y / l)),
            kb(jsnum::floor(p.z / lz)),
        ]
    };
    let mut cell: HashMap<[u64; 3], f64> = HashMap::new();
    for &i in g {
        *cell.entry(key(&pts[i])).or_insert(0.0) += pts[i].w;
    }
    let (mut seed, mut best) = (g[0], -1.0);
    for &i in g {
        let w = cell[&key(&pts[i])];
        if w > best {
            best = w;
            seed = i;
        }
    }
    let s = &pts[seed];
    g.iter()
        .copied()
        .filter(|&i| {
            jsnum::hypot(&[pts[i].x - s.x, pts[i].y - s.y]) <= l && jsnum::abs(pts[i].z - s.z) <= lz
        })
        .collect()
}

/// Port of `redGrid` (web/fins/fill.js:416-432): is there leftover red within FILL.redNear of p (plan
/// and height)? Gridded.
struct RedGrid {
    c: f64,
    grid: HashMap<i64, Vec<V3>>,
}

impl RedGrid {
    fn new(pts: &[Pt], cfg: &Config) -> RedGrid {
        let c = cfg.fill.red_near;
        let mut grid: HashMap<i64, Vec<V3>> = HashMap::new();
        for p in pts {
            let k = ((jsnum::floor(p.x / c)) * 100003.0 + jsnum::floor(p.y / c)) as i64;
            grid.entry(k).or_default().push([p.x, p.y, p.z]);
        }
        RedGrid { c, grid }
    }

    fn at(&self, p: V3) -> bool {
        let c = self.c;
        let gi = jsnum::floor(p[0] / c);
        let gj = jsnum::floor(p[1] / c);
        for di in -1..=1 {
            for dj in -1..=1 {
                let k = ((gi + di as f64) * 100003.0 + (gj + dj as f64)) as i64;
                for q in self.grid.get(&k).map(Vec::as_slice).unwrap_or(&[]) {
                    if jsnum::hypot(&[p[0] - q[0], p[1] - q[1]]) <= c
                        && jsnum::abs(p[2] - q[2]) <= c
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// One wall the fill pass kept (the object `keep` pushes into `props`, web/fins/fill.js:`keep`, with
/// the keys braceFill and tieFill add later).
#[derive(Clone, Debug, PartialEq)]
pub struct FillProp {
    pub line: Vec<V3>,
    pub tri_ranges: Vec<[usize; 2]>,
    pub height: f64,
    pub span: f64,
    pub tines: usize,
    pub part_attached: bool,
    pub squat: bool,
    /// `fill: true`, always.
    pub fill: bool,
    /// `joined: joined || undefined`.
    pub joined: bool,
    /// `braces` (braceFill): the ribs built on this wall.
    pub braces: Option<usize>,
    /// `ties` (tieFill).
    pub ties: Option<usize>,
}

/// Port of `braceFill` (web/fins/fill.js:530-563): BRACES, last: every wall is placed first, exactly
/// as without them -- ribs built as the walls went in took the plate the next walls needed (Isaac: 12
/// walls -> 10, bare red 71 -> 85 mm2). Then each wall that stands tall and short on the plate gets
/// the ribs that fit (prop/brace.js), clear of Auto's supports and of every other fill wall and its
/// ribs (its own wall excepted; joined flanges allowed, as for the walls). Appends to `triangles`,
/// adds each wall's ribs as a second triRange; returns the count.
fn brace_fill(
    props: &mut [FillProp],
    floors: &[Option<Vec<f64>>],
    triangles: &mut Vec<V4>,
    tris: &Tris,
    auto_tris: &[V4],
    shared: bool,
    cfg: &Config,
) -> usize {
    let mut auto_grid: RefCell<Option<BoxGrid>> = RefCell::new(None);
    let grids: RefCell<Vec<Option<BoxGrid>>> =
        RefCell::new((0..props.len()).map(|_| None).collect());
    let mut n = 0;
    for k in 0..props.len() {
        cfg.cancel.check();
        let Some(floor) = &floors[k] else { continue };
        let mut out: Vec<V4> = Vec::new();
        let built = {
            let props_ro: &[FillProp] = props;
            let tri_ro: &[V4] = triangles;
            let grid_of = |j: usize, f: &mut dyn FnMut(&mut BoxGrid)| {
                let mut gs = grids.borrow_mut();
                if gs[j].is_none() {
                    let mut g = BoxGrid::new();
                    for [a, b] in &props_ro[j].tri_ranges {
                        g.add(&tri_ro[*a..*b], None);
                    }
                    gs[j] = Some(g);
                }
                f(gs[j].as_mut().unwrap());
            };
            let clear = |t: &[V4]| -> bool {
                if auto_grid.borrow().is_none() {
                    let mut g = BoxGrid::new();
                    g.add(auto_tris, None);
                    *auto_grid.borrow_mut() = Some(g);
                }
                if auto_grid
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .hits(t, cfg.prop.side_clear, shared, None)
                {
                    return false;
                }
                for j in 0..props_ro.len() {
                    if j != k {
                        let mut hit = false;
                        grid_of(j, &mut |g| {
                            hit = g.hits(t, cfg.prop.side_clear, shared, None)
                        });
                        if hit {
                            return false;
                        }
                    }
                }
                true
            };
            brace_wall(
                &props_ro[k].line,
                |i| floor[i],
                tris,
                0.0,
                &mut out,
                |t| {
                    if !clear(t) {
                        return false;
                    }
                    grid_of(k, &mut |g| g.add(t, None)); // the next wall's ribs keep clear of these
                    true
                },
                BraceKinds {
                    slim: true,
                    tall: false,
                },
                cfg,
            )
        };
        if built == 0 {
            continue;
        }
        let at = triangles.len();
        triangles.extend(out);
        props[k].tri_ranges.push([at, triangles.len()]);
        props[k].braces = Some(built);
        n += built;
    }
    let _ = &mut auto_grid;
    n
}

/// Port of `tieFill` (web/fins/fill.js:572-605): TIES, after the ribs: each fill wall standing tall
/// on the plate is tied to its nearest neighbours (prop/ties.js), closest pairs first, TIE.perWall
/// each -- a forest of thin walls becomes a frame. A tie keeps sideClear off every support but its own
/// two walls (Auto's, the other fill walls, their ribs and ties). Appends to `triangles`; the struts
/// join wall A's triRanges. Returns how many ties.
fn tie_fill(
    props: &mut [FillProp],
    floors: &[Option<Vec<f64>>],
    triangles: &mut Vec<V4>,
    tris: &Tris,
    others: &mut BoxGrid,
    shared: bool,
    cfg: &Config,
) -> usize {
    let (tie, prop, brace) = (&cfg.tie, &cfg.prop, &cfg.brace);
    // per wall: its plate stations as [x, y, zMax, floor], zMax where its ties must stop
    let plate: Vec<Vec<[f64; 4]>> = props
        .iter()
        .enumerate()
        .map(|(k, q)| {
            let Some(floor) = &floors[k] else {
                return Vec::new();
            };
            if q.height < tie.min_height {
                return Vec::new();
            }
            q.line
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    [
                        p[0],
                        p[1],
                        jsnum::min(
                            tie.rise * (p[2] - prop.gap),
                            p[2] - prop.gap - prop.tip_h - 1.0,
                        ),
                        floor[i],
                    ]
                })
                .filter(|p| p[3] <= brace.on_plate)
                .collect()
        })
        .collect();
    struct PairD {
        d: f64,
        a: [f64; 4],
        b: [f64; 4],
        k: usize,
        j: usize,
    }
    let mut pairs: Vec<PairD> = Vec::new();
    for k in 0..props.len() {
        for j in k + 1..props.len() {
            let mut best: Option<PairD> = None;
            for a in &plate[k] {
                for b in &plate[j] {
                    let d = jsnum::hypot(&[b[0] - a[0], b[1] - a[1]]);
                    if d >= tie.min_span
                        && d <= tie.max_span
                        && best.as_ref().is_none_or(|bb| d < bb.d)
                    {
                        best = Some(PairD {
                            d,
                            a: *a,
                            b: *b,
                            k,
                            j,
                        });
                    }
                }
            }
            if let Some(b) = best {
                pairs.push(b);
            }
        }
    }
    pairs.js_sort_by(|p, q| cmp_diff(p.d - q.d));
    let mut count = vec![0.0f64; props.len()];
    let mut n = 0;
    for PairD { a, b, k, j, .. } in pairs {
        if count[k] >= tie.per_wall || count[j] >= tie.per_wall {
            continue;
        }
        let mut out: Vec<V4> = Vec::new();
        if tie_walls(
            [a[0], a[1]],
            [b[0], b[1]],
            jsnum::min(a[2], b[2]),
            tris,
            0.0,
            &mut out,
            cfg,
        ) == 0
        {
            continue;
        }
        let own: &dyn Fn(Tag) -> bool = &|t| t.is_some_and(|tk| tk == k || tk == j);
        if others.hits(&out, prop.side_clear, shared, Some(own)) {
            continue;
        }
        let at = triangles.len();
        triangles.extend(out.iter().copied());
        props[k].tri_ranges.push([at, triangles.len()]);
        others.add(&out, Some(k));
        props[k].ties = Some(props[k].ties.unwrap_or(0) + 1);
        count[k] += 1.0;
        count[j] += 1.0;
        n += 1;
    }
    n
}

/// The `opts` the fill pass reads (web/fins/fill.js:`fillOnce`, `fillCoverage`, `bareArea`): the
/// `buildFins` options, only those keys. `None` / `false` is the key absent.
#[derive(Clone, Debug, Default)]
pub struct FillOpts<'a> {
    /// `coverage` (the slider; JS `?? 0.5`).
    pub coverage: Option<f64>,
    /// `longWalls` (only `=== false` turns the rows off).
    pub long_walls: Option<bool>,
    /// `fillRegions`: Auto's curved-walls fill is about the red a curve won.
    pub fill_regions: Option<IndexSet<usize>>,
    /// `islands`: `findIslands`' result (JS `?? []`).
    pub islands: &'a [Island],
    /// `plateOnly === true`.
    pub plate_only: bool,
    /// `separateWalls === true`.
    pub separate_walls: bool,
    /// `tines` (JS `?? true`).
    pub tines: Option<bool>,
    pub tine_density: Option<f64>,
    pub layer_height: Option<f64>,
}

/// `unserved` of the fill's result: `{ area, pts, islands, islandPts }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unserved {
    pub area: f64,
    pub pts: Vec<V3>,
    pub islands: usize,
    pub island_pts: Vec<V3>,
}

/// `stats` of the fill's result. Keys JS sets only sometimes are `Option`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FillRun {
    pub walls: usize,
    pub tries: usize,
    pub checks: usize,
    pub capped: bool,
    /// `refused`: why-not message to count, in first-seen order.
    pub refused: IndexMap<String, usize>,
    pub start_area: f64,
    pub islands: usize,
    /// `joined` (set by the first joined wall).
    pub joined: Option<usize>,
    pub rows: Option<usize>,
    pub row_builds: Option<usize>,
    /// `ribs`, `ties`: set at the end of a run that found red.
    pub ribs: Option<usize>,
    pub ties: Option<usize>,
    pub islands_capped: Option<bool>,
    /// `rowsLost` (fillCoverage): mm2 more bare for the rows that lost.
    pub rows_lost: Option<f64>,
}

/// What `fillCoverage` returns: `{ triangles, props, tines, unserved, stats }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FillResult {
    pub triangles: Vec<V4>,
    pub props: Vec<FillProp>,
    pub tines: usize,
    pub unserved: Unserved,
    pub stats: FillRun,
}

/// What `wallFor` returns: `true` (added in place), `CAP`, or the indices the best line was aiming at.
enum WallRes {
    Built,
    Cap,
    Aims(Vec<usize>),
}

/// What `tryLines` returns: `true`, `CAP`, `aims` or `null`.
enum TryRes {
    Built,
    Cap,
    Aims(Vec<usize>),
    NoneBuilt,
}

/// The locals of one `wallFor` call that `tryLines` reads and writes.
struct WallState {
    aims: Option<Vec<usize>>,
    builds: usize,
    part_ok: bool,
    flange_ok: bool,
    join_ok: bool,
}

/// `fillOnce`'s state (the locals its closures `keep`, `wallFor`, `full` and `bare` share).
struct Fill<'a> {
    cfg: &'a Config,
    targets: &'a Targets,
    r: f64,
    tris: Arc<Tris>,
    draw_opts: DrawOpts<'a>,
    near: crate::fins::coverage::Reach,
    near_island: crate::fins::coverage::Reach,
    others: BoxGrid,
    triangles: Vec<V4>,
    props: Vec<FillProp>,
    floors: Vec<Option<Vec<f64>>>,
    tines: usize,
    dead: HashSet<usize>,
    past: f64,
    stats: FillRun,
    plate_only: bool,
    strict: Option<Relax>,
    apart: bool,
    shared: bool,
}

impl Fill<'_> {
    /// `full()`: the caps (counts, not a clock).
    fn full(&self) -> bool {
        let f = &self.cfg.fill;
        self.stats.walls as f64 >= f.max_walls + self.past
            || self.stats.tries as f64 >= f.max_tries + 3.0 * self.past
            || self.stats.checks as f64 >= f.max_checks + 50.0 * self.past
    }

    /// `held(p)`: an island start's point is held only from right under it (FILL.islandReach).
    fn held(&self, p: &Pt) -> bool {
        if p.island.is_some() {
            self.near_island.near(p.x, p.y, p.z)
        } else {
            self.near.near(p.x, p.y, p.z)
        }
    }

    /// `bare(p)`.
    fn bare(&self, p: &Pt) -> bool {
        !self.dead.contains(&p.id) && !self.held(p)
    }

    /// `keep(r, joined = false)`: a built wall goes in: its triangles, its tops into the reach set,
    /// its record.
    fn keep(&mut self, r: &DrawnOk, joined: bool) {
        let at = self.triangles.len();
        self.triangles.extend(r.tris.iter().copied());
        self.others.add(&r.tris, Some(self.props.len()));
        for p in &r.top {
            self.near.add(*p);
            self.near_island.add(*p);
        }
        self.props.push(FillProp {
            line: r.top.clone(),
            tri_ranges: vec![[at, self.triangles.len()]],
            height: r.height,
            span: r.length,
            tines: r.tines,
            part_attached: r.part_attached == Some(true),
            squat: r.squat,
            fill: true,
            joined,
            braces: None,
            ties: None,
        });
        if joined {
            self.stats.joined = Some(self.stats.joined.unwrap_or(0) + 1);
        }
        self.floors.push(r.floors.clone());
        self.tines += r.tines;
        self.stats.walls += 1;
    }

    fn refuse(&mut self, why: &str) {
        *self.stats.refused.entry(why.to_string()).or_insert(0) += 1;
    }

    /// `tryLines(cands)`: the first of `cands` that builds, keeps off every support and holds new
    /// red: `Built`; `Cap`; `Aims` (this group's tries are used up); or `NoneBuilt`.
    #[allow(clippy::too_many_arguments)]
    fn try_lines(
        &mut self,
        cands: &[Cand],
        live: &[Pt],
        g: &[usize],
        local: bool,
        red_at: &RedGrid,
        st: &mut WallState,
        ctx: &mut BuildCtx,
    ) -> TryRes {
        let cfg = self.cfg;
        for c0 in cands {
            if self.full() {
                return TryRes::Cap;
            }
            if st.aims.is_none() {
                st.aims = Some(c0.aims.clone());
            }
            self.stats.checks += 1;
            let mut c = under_stretch(
                (c0.a, c0.b),
                &self.tris,
                red_at,
                &self.others,
                self.strict,
                local,
                cfg,
            )
            .and_then(|q| {
                settled_stretch(
                    &q,
                    &self.tris,
                    red_at,
                    &self.others,
                    self.strict,
                    local,
                    cfg,
                )
            });
            // ...and relaxed (stationClear): standing on the part where there is part under
            // it (a chin over the chest, its plate below taken), its flange joining the
            // flanges on the plate. Tried second, each only until a wall it let through
            // builds into a support (the knuckle's mixed lines did, build after build)
            let mut relaxed = false;
            if c.is_none() && (st.part_ok || st.flange_ok) {
                self.stats.checks += 1;
                let relax = Some(Relax {
                    part: st.part_ok,
                    flange: st.flange_ok,
                    join: false,
                    plate: self.plate_only,
                });
                c = under_stretch(
                    (c0.a, c0.b),
                    &self.tris,
                    red_at,
                    &self.others,
                    relax,
                    local,
                    cfg,
                )
                .and_then(|q| {
                    settled_stretch(&q, &self.tris, red_at, &self.others, relax, local, cfg)
                });
                relaxed = c.is_some();
            }
            // ...and last, JOINED: crossing or meeting another fill wall (never Auto's or
            // the part), until one it lets through builds into Auto's
            let mut joined = false;
            if c.is_none() && st.join_ok {
                self.stats.checks += 1;
                let relax = Some(Relax {
                    part: st.part_ok,
                    flange: true,
                    join: true,
                    plate: self.plate_only,
                });
                c = under_stretch(
                    (c0.a, c0.b),
                    &self.tris,
                    red_at,
                    &self.others,
                    relax,
                    local,
                    cfg,
                )
                .and_then(|q| {
                    settled_stretch(&q, &self.tris, red_at, &self.others, relax, local, cfg)
                });
                joined = c.is_some();
            }
            let Some(c) = c else {
                self.refuse("no underside to follow");
                continue;
            };
            // a group whose lines keep passing the checks but won't build (part under it,
            // no headroom) stops here: each drawnWall is ~0.2 s on a 250k-face part
            let b = st.builds;
            st.builds += 1;
            if b as f64 >= cfg.fill.group_tries {
                return TryRes::Aims(st.aims.clone().unwrap());
            }
            self.stats.tries += 1;
            let r = drawn_wall(c.a, c.b, &self.tris, 0.0, &self.draw_opts, ctx);
            // Candidates aimed at neighbouring MUST faces may cross an exempt roof.
            // An intentional exclusion is neither a failed build nor unserved red.
            if let DrawnWall::Ok(ok) = &r
                && touches_small_hole(self.targets, &ok.top)
            {
                continue;
            }
            let why: Option<String> = match &r {
                DrawnWall::Refused { reason } => Some(
                    reason
                        .split(" — ")
                        .next()
                        .unwrap_or("")
                        .split(" -- ")
                        .next()
                        .unwrap_or("")
                        .to_string(),
                ),
                DrawnWall::Ok(ok) => {
                    let skip: Option<&dyn Fn(Tag) -> bool> =
                        if joined { Some(&is_fill) } else { None };
                    if self
                        .others
                        .hits(&ok.tris, cfg.prop.side_clear, local && self.shared, skip)
                    {
                        Some("touches a support".to_string())
                    } else {
                        None
                    }
                }
            };
            if let Some(why) = why {
                self.refuse(&why);
                // (the relaxation that let it through: on the part, on the plate, or joined)
                if joined && why == "touches a support" {
                    st.join_ok = false;
                } else if relaxed && why == "touches a support" {
                    let DrawnWall::Ok(ok) = &r else {
                        unreachable!()
                    };
                    if ok.part_attached == Some(true) {
                        st.part_ok = false;
                    } else {
                        st.flange_ok = false;
                    }
                }
                continue;
            }
            let DrawnWall::Ok(ok) = &r else {
                unreachable!()
            };
            let reach = reach_of(&ok.top, self.r);
            let reach_island = reach_of(&ok.top, cfg.fill.island_reach);
            let mut gain = 0.0;
            for &i in g {
                let p = &live[i];
                let hit = if p.island.is_some() {
                    reach_island.near(p.x, p.y, p.z)
                } else {
                    reach.near(p.x, p.y, p.z)
                };
                if hit {
                    gain += p.w;
                }
            }
            if gain < cfg.fill.min_gain {
                self.refuse("holds no new red");
                continue;
            }
            self.keep(ok, joined);
            return TryRes::Built;
        }
        TryRes::NoneBuilt
    }

    /// `wallFor(live, g, local = false)`: one wall for group `g` of `live`: the first candidate
    /// (bridged lines first, then the same directions without crossing gaps) that builds, keeps off
    /// every support and holds new red. Added in place: `Built`; on a cap, `Cap`; otherwise the
    /// indices the best line was aiming at, so only that red is given up on and the rest of the
    /// group gets a try.
    fn wall_for(&mut self, live: &[Pt], g: &[usize], local: bool, ctx: &mut BuildCtx) -> WallRes {
        let cfg = self.cfg;
        cfg.cancel.check();
        let red_pts: Vec<Pt> = g.iter().map(|&i| live[i]).collect();
        let red_at = RedGrid::new(&red_pts, cfg);
        let mut st = WallState {
            aims: None,
            builds: 0,
            part_ok: local && !self.plate_only,
            flange_ok: local && !self.apart,
            join_ok: local && !self.apart,
        };
        // a group of island starts alone: lines over them, not R beside them
        let line_r = |ix: &[usize], r: f64| {
            if ix.iter().all(|&i| live[i].island.is_some()) {
                cfg.fill.island_reach
            } else {
                r
            }
        };
        if !local {
            for bridge in [cfg.fill.bridge, 2.0 * cfg.fill.sample] {
                let cands = candidate_lines(live, g, line_r(g, self.r), bridge, false, cfg);
                match self.try_lines(&cands, live, g, local, &red_at, &mut st, ctx) {
                    TryRes::Built => return WallRes::Built,
                    TryRes::Cap => return WallRes::Cap,
                    TryRes::Aims(a) => return WallRes::Aims(a),
                    TryRes::NoneBuilt => {}
                }
            }
            return WallRes::Aims(st.aims.unwrap_or_else(|| g.to_vec()));
        }
        // LOCAL (the second pass): patch by patch, densest red first -- lines over just
        // that patch, narrow ones too, standing on the part where there is part under
        // them. A patch none builds on is given up on here, and the next gets its turn.
        let mut rest: Vec<usize> = g.to_vec();
        while !rest.is_empty() {
            let sub = local_patch(live, &rest, cfg);
            st.builds = 0;
            st.part_ok = !self.plate_only;
            st.flange_ok = !self.apart;
            st.join_ok = !self.apart;
            // (one bridge: a patch is at most 2 x FILL.local across)
            let cands =
                candidate_lines(live, &sub, line_r(&sub, self.r), cfg.fill.bridge, true, cfg);
            // (the gain is counted over the whole group `g`, as `tryLines` closes over it)
            match self.try_lines(&cands, live, g, local, &red_at, &mut st, ctx) {
                TryRes::Built => return WallRes::Built,
                TryRes::Cap => return WallRes::Cap,
                _ => {}
            }
            for &i in &sub {
                self.dead.insert(live[i].id);
            }
            let gone: HashSet<usize> = sub.iter().copied().collect();
            rest.retain(|i| !gone.contains(i));
        }
        WallRes::Aims(st.aims.unwrap_or_else(|| g.to_vec()))
    }

    /// `heaviest(live)`: the groups of `live`, heaviest red first (stable).
    fn heaviest(&self, live: &[Pt]) -> Vec<Vec<usize>> {
        let mut gs: Vec<(Vec<usize>, f64)> = groups_of(live, self.cfg)
            .into_iter()
            .map(|g| {
                let w = g.iter().fold(0.0, |s, &i| s + live[i].w);
                (g, w)
            })
            .collect();
        gs.js_sort_by(|p, q| cmp_diff(q.1 - p.1));
        gs.into_iter().map(|x| x.0).collect()
    }

    /// One pass of "a wall under the heaviest bare group, regroup, again" over the targets
    /// `pool`: until nothing is bare or a cap stops it. `local` is the second pass.
    fn passes(&mut self, pool: &[Pt], local: bool, ctx: &mut BuildCtx) {
        while !self.full() {
            let live: Vec<Pt> = pool.iter().filter(|p| self.bare(p)).copied().collect();
            if live.is_empty() {
                break;
            }
            let g = self.heaviest(&live).remove(0);
            match self.wall_for(&live, &g, local, ctx) {
                WallRes::Cap => break,
                WallRes::Built => {}
                WallRes::Aims(a) => {
                    for i in a {
                        self.dead.insert(live[i].id);
                    }
                }
            }
        }
    }
}

/// Port of `fillOnce` (web/fins/fill.js:651-889): one run of the fill pass. `built` is Auto's result
/// (its fins' contact lines, and every support triangle in `triangles`); `targets` is the augmented
/// analyze result `automaticTargets` returned.
fn fill_once(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    opts: &FillOpts,
    built: &FinsBuilt,
    ctx: &mut BuildCtx,
) -> FillResult {
    let cfg = ctx.cfg;
    let fill = &cfg.fill;
    let coverage = jsnum::max(0.0, jsnum::min(1.0, opts.coverage.unwrap_or(0.5)));
    let r = fill_reach(coverage, cfg);
    let one_each = coverage < fill.sparse_below;
    let mut tops: Vec<V3> = Vec::new();
    for w in &built.fins {
        tops.extend(w.line.iter().copied());
    }
    let near = reach_of(&tops, r);
    let near_island = reach_of(&tops, fill.island_reach);
    let cls = classify_targets(result);
    let red = leftover_points(
        topo,
        result,
        rot,
        &|x, y, z| near.near(x, y, z),
        &cls,
        opts.fill_regions.as_ref(),
        cfg,
    );
    // (not under fillRegions: Auto's curved-walls fill is about the red a curve won)
    let isl: Vec<(V3, usize)> = if opts.fill_regions.is_some() {
        Vec::new()
    } else {
        island_points(opts.islands, &|x, y, z| near_island.near(x, y, z), cfg)
    };
    // every target, red first: a point's id is its index here
    let mut all: Vec<Pt> = red
        .iter()
        .enumerate()
        .map(|(i, p)| Pt {
            x: p[0],
            y: p[1],
            z: p[2],
            w: p[3],
            island: None,
            id: i,
        })
        .collect();
    let n_red = all.len();
    for (j, (p, k)) in isl.iter().enumerate() {
        all.push(Pt {
            x: p[0],
            y: p[1],
            z: p[2],
            w: fill.island_w,
            island: Some(*k),
            id: n_red + j,
        });
    }
    let pts: Vec<Pt> = all[..n_red].to_vec();
    let isl_pts: Vec<Pt> = all[n_red..].to_vec();
    let stats = FillRun {
        start_area: red.iter().fold(0.0, |s, p| s + p[3]),
        islands: starts_in(&isl_pts),
        ..Default::default()
    };
    if red.is_empty() && isl.is_empty() {
        return FillResult {
            stats,
            ..Default::default()
        };
    }

    let tris = seated_part_tris(topo, rot, &result.analysis.offset);
    // the model's length (opts.longWalls, on unless false: the Long walls box), for the rows
    let field = if opts.long_walls != Some(false) {
        Some(heading_field(topo, &result.analysis, rot, cfg))
    } else {
        None
    };
    let mut others = BoxGrid::new();
    others.add(&built.triangles, None);
    // Reserve the excluded cavity as well as excluding its roof as a target.
    // Otherwise removing its old wall opens a route for Full's fill candidates,
    // including walls aimed at a neighbouring overhang. This is collision-only;
    // it neither emits a support nor claims that any red area is held.
    others.add_v3(&result.excluded_support_volumes, None);
    // Plate only (#218): no wall on the part -- never the retry that stands one there,
    // and every try refuses a station with part under it (stationClear's `plate`)
    let plate_only = opts.plate_only;
    let strict = if plate_only {
        Some(Relax {
            plate: true,
            ..Default::default()
        })
    } else {
        None
    };
    // Separate walls (#309, the resin default): no wall crosses another or joins its
    // flange -- resin cures crossing walls and shared bases into one block that won't
    // break apart. Never the flange or joined retries, and no flange band is shared.
    let apart = opts.separate_walls;
    let shared = !apart;
    let draw_opts = DrawOpts {
        under: true,
        tines: opts.tines.unwrap_or(true),
        tine_density: opts.tine_density,
        layer_height: opts.layer_height,
        plate_only,
        topo: Some(topo),
        rot: Some(*rot),
        offset: Some(result.analysis.offset),
        ..Default::default()
    };
    let mut st = Fill {
        cfg,
        targets: result,
        r,
        tris: tris.clone(),
        draw_opts,
        near,
        near_island,
        others,
        triangles: Vec::new(),
        props: Vec::new(),
        floors: Vec::new(),
        tines: 0,
        dead: HashSet::new(),
        past: 0.0, // the island pass's budget past the caps (FILL.islandWalls)
        stats,
        plate_only,
        strict,
        apart,
        shared,
    };

    if one_each {
        // sparse: each group of the red Auto left gets ONE wall, against what is still bare
        for g0 in st.heaviest(&pts) {
            if st.full() {
                break;
            }
            // tries until it gets its wall: a failed line gives up only the red it aimed at
            loop {
                let live: Vec<Pt> = g0.iter().map(|&i| pts[i]).filter(|p| st.bare(p)).collect();
                if live.is_empty() || st.full() {
                    break;
                }
                let g: Vec<usize> = (0..live.len()).collect();
                match st.wall_for(&live, &g, false, ctx) {
                    WallRes::Built | WallRes::Cap => break,
                    WallRes::Aims(a) => {
                        for i in a {
                            st.dead.insert(live[i].id);
                        }
                    }
                }
            }
        }
    } else {
        // ROWS first (opts.longWalls): long walls along the model's length (fins/rows.js);
        // the patch lines below take what they leave
        if let Some(field) = &field
            && !red.is_empty()
        {
            // one heading for every row: the length at the bare red's centroid (none: x)
            let (mut w, mut x, mut y) = (0.0, 0.0, 0.0);
            for p in &red {
                w += p[3];
                x += p[0] * p[3];
                y += p[1] * p[3];
            }
            let heading = field(x / w, y / w).unwrap_or(0.0);
            // (the red only: a row is along the model's length, an island start is a point)
            let draw_opts = st.draw_opts.clone();
            let rw = {
                let cell = RefCell::new(&mut st);
                let n_faces = topo.n_faces as f64;
                row_walls(
                    RowWallsArgs {
                        pts: &red,
                        heading,
                        r,
                        tris: &tris,
                        draw_opts: &draw_opts,
                        min_gain: fill.min_gain,
                        fits: &mut |t| {
                            let s = cell.borrow();
                            !touches_small_hole(s.targets, &t.top)
                                && !s.others.hits(&t.tris, cfg.prop.side_clear, s.shared, None)
                        },
                        keep: &mut |t| cell.borrow_mut().keep(t, false),
                        room: &mut || {
                            let s = cell.borrow();
                            (s.stats.walls as f64) < s.cfg.fill.max_walls
                        },
                        n_faces,
                    },
                    ctx,
                )
            };
            st.stats.rows = Some(rw.walls);
            st.stats.row_builds = Some(rw.builds);
        }
        // until every point is within reach: a wall under the heaviest bare group, regroup, again
        st.passes(&pts, false, ctx);
        // ...then, with what the caps leave, the red still bare again, patch by patch
        // (wallFor's LOCAL): an upright figure's groups are a hem, both arms and the
        // hands at one height, and every line across all that met the first wall
        // (only if pass 1 left room: after a cap, what it gave up on stays given up, or
        // `capped` would count it as stopped-on)
        if !st.full() {
            st.dead.clear();
        }
        st.passes(&pts, true, ctx);
    }
    st.stats.capped = st.full() && pts.iter().any(|p| !st.held(p) && !st.dead.contains(&p.id));
    // the red's bare as it stands now, before the island walls (which hold some too):
    // what fillCoverage's rows-or-not pick reads, so that pick stays main's
    let left_red: Vec<Pt> = pts.iter().filter(|p| !st.held(p)).copied().collect();
    // ISLAND STARTS, last (see the header): lines over each group, then patch by patch
    if !isl_pts.is_empty() {
        st.past = fill.island_walls;
        for local in [false, true] {
            for p in &isl_pts {
                st.dead.remove(&p.id);
            }
            st.passes(&isl_pts, local, ctx);
        }
        // stopped on its budget with a start still untried: the card says so
        st.stats.islands_capped = Some(
            st.full()
                && isl_pts
                    .iter()
                    .any(|p| !st.held(p) && !st.dead.contains(&p.id)),
        );
    }
    {
        let st = &mut st;
        st.stats.ribs = Some(brace_fill(
            &mut st.props,
            &st.floors,
            &mut st.triangles,
            &tris,
            &built.triangles,
            shared,
            cfg,
        ));
        for k in 0..st.props.len() {
            let ranges: Vec<[usize; 2]> = st.props[k].tri_ranges.iter().skip(1).copied().collect();
            for [a, b] in ranges {
                let t: Vec<V4> = st.triangles[a..b].to_vec();
                st.others.add(&t, Some(k));
            }
        }
        st.stats.ties = Some(tie_fill(
            &mut st.props,
            &st.floors,
            &mut st.triangles,
            &tris,
            &mut st.others,
            shared,
            cfg,
        ));
    }
    let left_isl: Vec<Pt> = isl_pts.iter().filter(|p| !st.held(p)).copied().collect();
    FillResult {
        tines: st.tines,
        unserved: Unserved {
            area: left_red.iter().fold(0.0, |s, p| s + p.w),
            pts: left_red.iter().map(|p| [p.x, p.y, p.z]).collect(),
            islands: starts_in(&left_isl),
            island_pts: left_isl.iter().map(|p| [p.x, p.y, p.z]).collect(),
        },
        triangles: st.triangles,
        props: st.props,
        stats: st.stats,
    }
}

/// Port of `fillCoverage` (web/fins/fill.js:619-632): the fill pass. `built` is Auto's result (fins
/// with their contact lines, and every support triangle in `triangles`). Returns the added walls:
/// `{ triangles, props, tines, unserved: { area, pts }, stats }`.
///
/// With Long walls (opts.longWalls, on unless false) it is built twice, with the rows and without,
/// and the rows are kept only if they leave no more red bare than the patch lines alone: rows laid
/// first can take the room a patch line needed (a wedge at X40: 17 -> 47 mm2 bare), and every red
/// face must get something. A tie keeps the rows -- the walls a hand would draw. `stats.rowsLost`
/// (mm2 more bare) says when they lost.
pub fn fill_coverage(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    opts: &FillOpts,
    built: &FinsBuilt,
    ctx: &mut BuildCtx,
) -> FillResult {
    // (no rows under the sparse slider: one wall per group is its own promise)
    let mut plain_opts = opts.clone();
    plain_opts.long_walls = Some(false);
    if opts.long_walls == Some(false) || opts.coverage.unwrap_or(0.5) < ctx.cfg.fill.sparse_below {
        return fill_once(topo, result, rot, &plain_opts, built, ctx);
    }
    let mut plain = fill_once(topo, result, rot, &plain_opts, built, ctx);
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(plain.unserved.area > 0.0) {
        return plain; // nothing left for rows to win
    }
    let rows = fill_once(topo, result, rot, opts, built, ctx);
    if rows.stats.rows.unwrap_or(0) == 0 || rows.unserved.area > plain.unserved.area + 1e-6 {
        plain.stats.rows_lost = Some(if rows.stats.rows.unwrap_or(0) != 0 {
            rows.unserved.area - plain.unserved.area
        } else {
            0.0
        });
        return plain;
    }
    rows
}

/// Port of `bareArea` (web/fins/fill.js:640-649): how much MUST red `fins`' walls leave bare,
/// measured the way fillOnce measures it (a point is held within fillReach of a wall top): `(area,
/// total)` in mm2, `total` being all the MUST red. No walls are placed. Auto's readout says it, so a
/// build that holds little doesn't read as done (fins.js sets built.bare). `fins` is JS's
/// `built.fins` (`bareArea(.., { fins: [] })` is an empty slice).
pub fn bare_area(
    topo: &Topology,
    result: &Targets,
    rot: &Rot3,
    opts: &FillOpts,
    fins: &[crate::fins::build::FinRecord],
    cfg: &Config,
) -> (f64, f64) {
    let mut tops: Vec<V3> = Vec::new();
    // (a sway brace's line is its foot on the plate, and it holds no red)
    for w in fins {
        if w.kind != FinKind::Sway {
            tops.extend(w.line.iter().copied());
        }
    }
    let near = reach_of(
        &tops,
        fill_reach(
            jsnum::max(0.0, jsnum::min(1.0, opts.coverage.unwrap_or(0.5))),
            cfg,
        ),
    );
    let cls = classify_targets(result);
    // (no walls: nothing is near, and the area is the total)
    let (area, total) = bare_sums(
        topo,
        result,
        rot,
        if tops.is_empty() { None } else { Some(&near) },
        &cls,
        cfg,
    );
    (area, total)
}
