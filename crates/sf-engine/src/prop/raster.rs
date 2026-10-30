//! Port of `web/prop/raster.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Raster placement: walls across a region's WHOLE footprint, the way a slicer's grid support
//! stands, raced against the patch/tube placement region by region.
//!
//! Why. A wall's top already follows whatever surface is above it (contourTop, settleTop), so a
//! wall does not need a straight or flat overhang -- only a straight track in PLAN. What held
//! curved parts back was placement: a curved region got ONE wall under its lowest line
//! (tubeLine), and anything else was split into 15-degree patches (splitRegion) with every
//! patch under 12 mm2 dropped. The dome ceiling example got 2 walls and 34% of its area held
//! with no skips at all -- nothing refused, the placer simply never tried a second wall. Laying
//! parallel tracks at the row pitch across the whole region, the way a grid support does, took
//! it to 75% (bowl 24 -> 75, torus X30 4 -> 54, the real bear upright 0 -> 57; spike in
//! prototype/raster/, local issue 008).
//!
//! Why a race and not a replacement. Raster alone LOSES on real tubes: a pipe on its side wants
//! one wall under its lowest line, and parallel tracks across it land on its flanks (hook 98 ->
//! 36%, vase_flare X30 93 -> 59%). So buildProps builds the normal pass, then a raster pass over
//! only the regions the normal pass left partly bare (`rasterWanted`), and `raceRegions` keeps,
//! per region, whichever holds more overhang area -- by a margin, so a region the normal pass
//! already holds does not swap one clean wall for several stepped pieces for a sliver more area
//! (the real gree figure's arm, measured in the spike).
//!
//! Split out of prop.js, which only wires the two passes together.

use crate::guards::JsSort;
use indexmap::{IndexMap, IndexSet};

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::bridge::Support;
use crate::prop::build::{BuildOpts, Line, Lines, PropRecord, PropsPass};
use crate::prop::ctx::{BuildCtx, TineSeed};
use crate::prop::mold::floor_line;
use crate::prop::surface::seat;
use crate::prop::sweep::foot_for;
use crate::prop::tracks::patch_tracks;
use crate::tris::Tris;
use crate::types::Rot3;

/// `RACE_REL` (web/prop/raster.js:35): a raster region replaces the normal one only if it holds
/// this much more overhang area: this share of the normal pass's held area...
pub const RACE_REL: f64 = 0.10;
/// `RACE_ABS` (web/prop/raster.js:36): ...and at least this many mm2. Equal-ish coverage keeps
/// the normal pass's fewer, longer walls.
pub const RACE_ABS: f64 = 5.0;
/// `RASTER_SKIP_HELD` (web/prop/raster.js:39): a region the normal pass already holds this much
/// of (own faces) is not rebuilt as raster -- there is nothing left to win.
pub const RASTER_SKIP_HELD: f64 = 0.95;
/// `GRIP_RISE` (web/prop/raster.js:44): a swap may not raise a region's lowest grip by more than
/// this (mm): the lowest tine is what holds the part while it is still short and least stable,
/// and prototype/sweep/compare.js blocks on it rising the same amount.
pub const GRIP_RISE: f64 = 0.1;
/// `GRIP_RISE_REL` (web/prop/raster.js:51): ...or by this fraction of the grip's height,
/// whichever is more. The 0.1 mm matters near the bed; a grip 40 mm up already stands on 40 mm of
/// printed part, and bear/gree/dome lost a raster wall holding 2-3x the area over a 0.2-2.4 mm
/// rise there (Matthew, 2026-09-28).
pub const GRIP_RISE_REL: f64 = 0.05;

/// Port of `gripRise` (web/prop/raster.js:52).
pub fn grip_rise(z: f64) -> f64 {
    jsnum::max(GRIP_RISE, GRIP_RISE_REL * z)
}

/// Port of `rasterTracks` (web/prop/raster.js:62-88): the raster tracks for one region:
/// parallel straight tracks at `row_span` across its whole footprint (patchTracks on the
/// region, not on a patch), each cut where the region has no face above it and where the floor
/// under it changes between plate and part, so every piece goes down exactly one of the plate
/// path or the part-attached path. The raster pass then keeps every usable run of a track, not
/// just the longest (see [`raster_rest`]). `support` is `{ topo, rot, offset, sway }`.
pub fn raster_tracks(
    region_pts: &[[f64; 3]],
    region_tris: &Tris,
    part_tris: &Tris,
    step: f64,
    row_span: f64,
    support: &Support,
    cfg: &Config,
) -> Lines {
    let mut lines: Vec<Line> = Vec::new();
    // never turned across the bridge: only buildPass races a turn against the old
    // walls (prop/bridgerace.js), and this pass races its tracks against those
    let tracks = patch_tracks(
        region_pts,
        region_tris,
        step,
        Some(support),
        row_span,
        None,
        false,
        cfg,
    );
    let spacing = tracks.spacing; // row pitch, for the sparse-coverage sag warning
    let min_stations = cfg.prop.min_stations;
    for t in &tracks.lines {
        // floorLine's plate/part split, the same test buildPartAttached counts with
        let floor = floor_line(t, part_tris, 1.0, false, cfg);
        let mut cur: Vec<[f64; 3]> = Vec::new();
        let mut cls: Option<bool> = None;
        for k in 0..t.len() {
            let on_part = floor[k].z > cfg.prop.gap + 0.5;
            if cls.is_some_and(|c| on_part != c) {
                if cur.len() as f64 >= min_stations {
                    lines.push(Line {
                        pts: std::mem::take(&mut cur),
                        ..Default::default()
                    });
                }
                cur = Vec::new();
            }
            cls = Some(on_part);
            cur.push(t[k]);
        }
        if cur.len() as f64 >= min_stations {
            lines.push(Line {
                pts: cur,
                ..Default::default()
            });
        }
    }
    Lines {
        lines,
        spacing,
        turned: None,
    }
}

/// Port of `rasterRest` (web/prop/raster.js:97-105): a raster track crosses the part and other
/// obstacles, so its longest usable run is rarely all of it: the pieces either side of `run` get
/// their own try. Returns them (each at least minStations long), each with `from`, the index its
/// first station had in `line` -- the caller claims those stations off `line`'s squat pass, since
/// the piece runs its own.
pub fn raster_rest(line: &[[f64; 3]], run: [usize; 2], cfg: &Config) -> Vec<Line> {
    let mut rest = Vec::new();
    for [a, b] in [[0, run[0]], [run[1], line.len()]] {
        if ((b as f64) - (a as f64)) < cfg.prop.min_stations {
            continue;
        }
        rest.push(Line {
            pts: line[a..b].to_vec(),
            from: Some(a),
            rim: false,
        });
    }
    rest
}

/// One overhang face seated once: `[cx, cy, cz, area, region, face]` (web/prop/raster.js:111-125;
/// the same shape `judge_ruler` takes).
pub type OverFace = [f64; 6];

/// Port of `overhangFaces` (web/prop/raster.js:111-125): overhang faces seated once.
pub fn overhang_faces(topo: &Topology, result: &Analysis, rot: &Rot3) -> Vec<OverFace> {
    let (pos, off) = (&topo.pos, &result.offset);
    let mut faces = Vec::new();
    for (ri, g) in result.regions.iter().enumerate() {
        for &f in &g.faces {
            let (mut cx, mut cy, mut cz) = (0.0f64, 0.0f64, 0.0f64);
            for i in 0..3 {
                let v = seat(pos, f * 9 + i * 3, rot, off);
                cx += v[0] / 3.0;
                cy += v[1] / 3.0;
                cz += v[2] / 3.0;
            }
            faces.push([cx, cy, cz, topo.area[f], ri as f64, f as f64]);
        }
    }
    faces
}

/// `lenient` of `raceAgainst`: do these second-pass walls force the swap (`bends`)?
pub type Lenient<'a> = &'a dyn Fn(&[&PropRecord]) -> bool;

/// A race ruler (`ruler` of `raceAgainst`): `(faces, tops) -> held area`.
pub type Ruler<'a> = &'a dyn Fn(&[OverFace], &[[f64; 3]]) -> f64;

/// Port of `heldBy` (web/prop/raster.js:136-148): overhang area held by wall TOPS `tops`: a top
/// point within half the maxUnsupportedSpan in plan and 0-1.5 mm below the face
/// (prototype/examples/probe.js's rule). Tops, not every support vertex as rival.js counts: a
/// wall's base flange sits within 3 mm of any low face around it, and on the torus at X45 that
/// credited a low ledge to a wall 9 mm away that never reaches it. `faces` is the set judged (a
/// region's own, or every face near it).
pub fn held_by(faces: &[OverFace], tops: &[[f64; 3]], cfg: &Config) -> f64 {
    let reach = cfg.prop.max_unsupported_span / 2.0;
    let mut held = 0.0f64;
    for f in faces {
        let (cx, cy, cz, a) = (f[0], f[1], f[2], f[3]);
        for p in tops {
            let dz = cz - p[2];
            if dz >= -0.05 && dz <= 1.5 && jsnum::hypot(&[p[0] - cx, p[1] - cy]) <= reach {
                held += a;
                break;
            }
        }
    }
    held
}

/// `wallTops` (web/prop/raster.js:151): every wall's top line (surface minus gap), the points
/// heldBy measures from.
fn wall_tops(qs: &[&PropRecord]) -> Vec<[f64; 3]> {
    qs.iter().flat_map(|q| q.line.iter().copied()).collect()
}

/// `halfFoot` (web/prop/raster.js:155-156): half-width of a wall's footprint on whatever it
/// stands on: a squat wall's brim, a part-attached wall's own thickness, a plate wall's flange.
fn half_foot(q: &PropRecord, cfg: &Config) -> f64 {
    if q.squat {
        cfg.prop.squat_brim_w
    } else if q.part_attached {
        cfg.prop.th / 2.0
    } else {
        foot_for(q.height, cfg)
    }
}

/// `gripZ` (web/prop/raster.js:160): the lowest point a set of walls grips from: each wall's
/// lowest tine vertex (`grip`, recorded by buildProps as it emits them). Infinity = no tines.
fn grip_z(qs: &[&PropRecord]) -> f64 {
    let mut m = f64::INFINITY;
    for q in qs {
        m = jsnum::min(m, q.grip.unwrap_or(f64::INFINITY));
    }
    m
}

/// `planGap` (web/prop/raster.js:163-176): closest approach of two walls' centrelines in PLAN.
fn plan_gap(l1: &[[f64; 3]], l2: &[[f64; 3]]) -> f64 {
    let seg = |p: &[f64; 3], a: &[f64; 3], b: &[f64; 3]| -> f64 {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let l = dx * dx + dy * dy;
        let t = if l > 1e-12 {
            jsnum::max(
                0.0,
                jsnum::min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l),
            )
        } else {
            0.0
        };
        jsnum::hypot(&[p[0] - a[0] - t * dx, p[1] - a[1] - t * dy])
    };
    let mut d = f64::INFINITY;
    for (pp, qq) in [(l1, l2), (l2, l1)] {
        for p in pp {
            if qq.len() == 1 {
                d = jsnum::min(d, jsnum::hypot(&[p[0] - qq[0][0], p[1] - qq[0][1]]));
            }
            let mut i = 0;
            while i + 1 < qq.len() {
                d = jsnum::min(d, seg(p, &qq[i], &qq[i + 1]));
                i += 1;
            }
        }
    }
    d
}

/// `byRegion` (web/prop/raster.js:178-185): a `Map` of region -> its walls, in first-met order.
fn by_region(props: &[PropRecord]) -> IndexMap<usize, Vec<&PropRecord>> {
    let mut m: IndexMap<usize, Vec<&PropRecord>> = IndexMap::new();
    for q in props {
        if let Some(r) = q.region {
            m.entry(r).or_default().push(q);
        }
    }
    m
}

/// The tag a race writes on the walls it keeps (`[tag]: build === b`), and counts as
/// `<tag>Regions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceTag {
    Raster,
    Circular,
    Rim,
    Downhill,
    Curved,
    /// The flush end rows' race (prop/endrows.js).
    Flush,
}

impl RaceTag {
    fn set(self, q: &mut PropRecord, v: bool) {
        let slot = match self {
            RaceTag::Raster => &mut q.raster,
            RaceTag::Circular => &mut q.circular,
            RaceTag::Rim => &mut q.rim,
            RaceTag::Downhill => &mut q.downhill,
            RaceTag::Curved => &mut q.curved,
            RaceTag::Flush => &mut q.flush,
        };
        *slot = Some(v);
    }
    fn get(self, q: &PropRecord) -> bool {
        match self {
            RaceTag::Raster => q.raster,
            RaceTag::Circular => q.circular,
            RaceTag::Rim => q.rim,
            RaceTag::Downhill => q.downhill,
            RaceTag::Curved => q.curved,
            RaceTag::Flush => q.flush,
        }
        .unwrap_or(false)
    }
}

/// Port of `withRaster` (web/prop/raster.js:196-198): buildProps's second half: given the normal
/// pass, run the raster pass over the regions it left partly bare (`raster_pass(regions)` is
/// buildProps's own pass builder) and race them. Returns `normal` itself when nothing swaps, so a
/// part the raster pass cannot improve builds byte-identical to the normal pass.
///
/// opts.rasterVeto (auto mode: fins/wedges.js wedgeVeto) drops raster walls that would take a
/// wedge's face before the race sees them.
///
/// The tests' tine capture (`ctx.tine_cap`, JS `globalThis.__TINECAP`, see tines.js) must end up
/// holding the KEPT walls' tines only: each pass captures into its own array, every wall records
/// its slice (`caps`), and the kept slices go back after whatever was captured before buildProps
/// began (`cap_from`). The veto runs with the capture still swapped out: it builds candidate
/// wedges, whose tines the wedge loop in fins.js emits again for real.
pub fn with_raster(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    normal: PropsPass,
    cap_from: usize,
    raster_pass: &dyn Fn(&IndexSet<usize>) -> PropsPass,
    ctx: &BuildCtx,
) -> PropsPass {
    race_against(
        topo,
        result,
        rot,
        opts,
        normal,
        cap_from,
        raster_pass,
        RaceTag::Raster,
        RaceArgs::default(),
        ctx,
    )
}

/// The trailing arguments of `raceAgainst` after `tag` (all optional in JS): `maxGrow`, `rel`,
/// `lenient`, `equivalent` and the last argument's `{ tried, ruler, price }`. `Default` gives the
/// JS defaults.
pub struct RaceArgs<'a> {
    /// `maxGrow` (null = no limit): a region whose second-pass walls weigh over this x the normal
    /// walls' volume never swaps -- a sliver more area is not worth it.
    pub max_grow: Option<f64>,
    /// `rel`: the share of the normal walls' held area the second pass must add (`RACE_REL`).
    pub rel: f64,
    /// `lenient` (null = none): a region whose second-pass walls pass it always swaps, whatever
    /// they hold or weigh, and every region is tried, not only the ones `normal` left bare
    /// (curved walls, prop/curved.js).
    pub lenient: Option<Lenient<'a>>,
    /// `equivalent`: also try fully held regions and prefer the second placement when its held
    /// area is equal. Circular candidates use this to choose a local frame even where world-axis
    /// rows already hold the circle. Unlike `lenient`, this still enforces the grip rule and
    /// retains useful, clear baseline walls.
    pub equivalent: bool,
    /// `tried` (a Set): the regions to rebuild and race instead of the ones `normal` left bare.
    pub tried: Option<IndexSet<usize>>,
    /// `ruler(faces, tops)` measures held area instead of heldBy (rim walls race on the 062
    /// judge, prop/judge.js).
    pub ruler: Option<Ruler<'a>>,
    /// `price` the held area per mm3 a swap must buy when it adds plastic.
    pub price: Option<f64>,
    /// `hold(qa, qb, ri)`: true when the region's current walls would lose something the swap
    /// keeps (a wedge).
    pub hold: Option<Hold<'a>>,
    /// `alone`: the swap replaces the region's walls and keeps none of them beside it (a layout
    /// of the same rows, not a second wall -- the flush end rows).
    pub alone: bool,
}

/// `hold(qa, qb, ri)` of `raceAgainst`'s last argument.
pub type Hold<'a> = &'a dyn Fn(&[&PropRecord], &[&PropRecord], usize) -> bool;

impl Default for RaceArgs<'_> {
    fn default() -> Self {
        RaceArgs {
            max_grow: None,
            rel: RACE_REL,
            lenient: None,
            equivalent: false,
            tried: None,
            ruler: None,
            price: None,
            hold: None,
            alone: false,
        }
    }
}

/// `arr.slice(a, b)` on a capture: clamped.
fn cap_slice(v: &[TineSeed], r: [usize; 2]) -> Vec<TineSeed> {
    let a = r[0].min(v.len());
    let b = r[1].min(v.len()).max(a);
    v[a..b].to_vec()
}

/// Port of `raceAgainst` (web/prop/raster.js:221-250): withRaster's race, for any second
/// placement: `pass(regions)` builds it over the regions `normal` left partly bare, and each
/// region keeps whichever holds more. `tag` names it on the kept walls (`q[tag]`) and in the
/// count (`<tag>Regions`). Downhill lines (prop/downhill.js) race through here as 'downhill'.
pub fn race_against(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    normal: PropsPass,
    cap_from: usize,
    pass: &dyn Fn(&IndexSet<usize>) -> PropsPass,
    tag: RaceTag,
    args: RaceArgs,
    ctx: &BuildCtx,
) -> PropsPass {
    race_against_counted(
        topo, result, rot, opts, normal, cap_from, pass, tag, args, ctx,
    )
    .0
}

/// [`race_against`], and the `<tag>Regions` count JS puts on its result: how many regions swapped
/// (`None` when `normal` came back untouched, where JS's result has no such key).
pub fn race_against_counted(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    normal: PropsPass,
    cap_from: usize,
    pass: &dyn Fn(&IndexSet<usize>) -> PropsPass,
    tag: RaceTag,
    args: RaceArgs,
    ctx: &BuildCtx,
) -> (PropsPass, Option<usize>) {
    let cfg = ctx.cfg;
    cfg.cancel.check();
    let faces = overhang_faces(topo, result, rot);
    let default_ruler = |f: &[OverFace], t: &[[f64; 3]]| held_by(f, t, cfg);
    let ruler: Ruler = args.ruler.unwrap_or(&default_ruler);
    let only = args.tried;
    let want = match &only {
        Some(o) => o.clone(),
        None => raster_wanted(&faces, result, &normal, cfg),
    };
    // Curved walls: a region the walls above already hold may still take a curve
    let tried: IndexSet<usize> = match &only {
        Some(o) => o.clone(),
        None if args.lenient.is_some() || args.equivalent => (0..result.regions.len()).collect(),
        None => want.clone(),
    };
    if tried.is_empty() {
        return (normal, None);
    }
    // `cap` is the capture array as it stands, `cap_normal` a copy of it; the pass captures into
    // its own array
    let cap: Option<Vec<TineSeed>> = ctx.tine_cap.borrow().clone();
    let cap_normal: Option<Vec<TineSeed>> = cap.clone();
    if cap.is_some() {
        *ctx.tine_cap.borrow_mut() = Some(Vec::new());
    }
    let mut raster = pass(&tried);
    if let Some(veto) = &opts.raster_veto {
        let kept: Vec<PropRecord> = raster
            .props
            .iter()
            .filter(|q| !veto(q, &normal.props))
            .cloned()
            .collect();
        raster.props = kept;
    }
    let cap_raster = std::mem::replace(&mut *ctx.tine_cap.borrow_mut(), cap);
    let wanted = if args.equivalent { None } else { Some(&want) };
    let (mut raced, swapped) = race_regions(
        &faces,
        result,
        &normal,
        &raster,
        tag,
        args.max_grow,
        args.rel,
        args.lenient,
        wanted,
        args.equivalent,
        ruler,
        args.price,
        args.hold,
        args.alone,
        cfg,
    );
    if swapped == 0 {
        return (normal, None);
    }
    if let (Some(cap), Some(cap_normal), Some(cap_raster)) = (
        ctx.tine_cap.borrow_mut().as_mut(),
        cap_normal.as_ref(),
        cap_raster.as_ref(),
    ) {
        cap.truncate(cap_from);
        // ...and each kept wall's `caps` re-pointed into the rebuilt capture, so a second
        // race (downhill, after raster) slices the right tines out of it
        for q in &mut raced.props {
            let Some(caps) = q.caps else { continue };
            let from = cap.len();
            let src = if tag.get(q) { cap_raster } else { cap_normal };
            cap.extend(cap_slice(src, caps));
            q.caps = Some([from, cap.len()]);
        }
    }
    (raced, Some(swapped))
}

/// Port of `rasterWanted` (web/prop/raster.js:259-270): the regions worth a raster pass: those
/// the normal pass `a` holds less than RASTER_SKIP_HELD of (own faces, by its own walls and every
/// other wall).
fn raster_wanted(
    faces: &[OverFace],
    result: &Analysis,
    a: &PropsPass,
    cfg: &Config,
) -> IndexSet<usize> {
    let all: Vec<&PropRecord> = a.props.iter().collect();
    let tops = wall_tops(&all);
    let mut want = IndexSet::new();
    for ri in 0..result.regions.len() {
        cfg.cancel.check();
        let own: Vec<OverFace> = faces
            .iter()
            .filter(|f| f[4] == ri as f64)
            .copied()
            .collect();
        let mut area = 0.0f64;
        for f in &own {
            area += f[3];
        }
        if area > 0.0 && held_by(&own, &tops, cfg) < RASTER_SKIP_HELD * area {
            want.insert(ri);
        }
    }
    want
}

/// Port of `nearRegion` (web/prop/raster.js:276-287): every overhang face within reach of region
/// `ri` (its bbox grown by two spans), as indices into `faces`: a wall under one region holds its
/// neighbours too, so the race judges them all.
fn near_region(faces: &[OverFace], ri: usize, cfg: &Config) -> Vec<usize> {
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for f in faces {
        if f[4] != ri as f64 {
            continue;
        }
        x0 = jsnum::min(x0, f[0]);
        x1 = jsnum::max(x1, f[0]);
        y0 = jsnum::min(y0, f[1]);
        y1 = jsnum::max(y1, f[1]);
    }
    let reach = 2.0 * cfg.prop.max_unsupported_span;
    (0..faces.len())
        .filter(|&i| {
            let f = &faces[i];
            f[0] >= x0 - reach && f[0] <= x1 + reach && f[1] >= y0 - reach && f[1] <= y1 + reach
        })
        .collect()
}

/// A JS comparator number as an `Ordering` (NaN counts as equal).
fn ordering(d: f64) -> std::cmp::Ordering {
    if d < 0.0 {
        std::cmp::Ordering::Less
    } else if d > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// Port of `keepers` (web/prop/raster.js:300-317): the normal-pass walls `qa` a region swapped
/// to raster (`qb`) still keeps, or `None` when the swap must not happen.
///
/// Kept: any normal wall that stands clear of every kept wall (feet + 1 mm apart in plan, so no
/// two fuse) and either holds area they miss or grips lower than they do. Torus X45: the raster
/// rows missed a low ledge the normal pass's squat wall held (69 mm2); bore_bracket X45: they
/// dropped the wall whose tines gripped 1.2 mm off the bed for one gripping from 22.9 mm. The
/// swap must trade neither. If the region's lowest grip still rises past gripRise, no swap --
/// unless `force` (a curved swap, raceRegions's `lenient`): then the swap stands with what it
/// kept.
fn keepers<'a>(
    qa: &[&'a PropRecord],
    qb: &[&'a PropRecord],
    near: &[OverFace],
    force: bool,
    alone: bool,
    cfg: &Config,
) -> Option<Vec<&'a PropRecord>> {
    let mut kept: Vec<&PropRecord> = qb.to_vec();
    let mut extra: Vec<&PropRecord> = Vec::new();
    let mut bare: Vec<OverFace> = {
        let tops = wall_tops(&kept);
        near.iter()
            .filter(|f| held_by(&[**f], &tops, cfg) == 0.0 || held_by(&[**f], &tops, cfg).is_nan())
            .copied()
            .collect()
    };
    // lowest-gripping first, so a low wall claims its spot before a neighbour
    let mut order: Vec<&PropRecord> = qa.to_vec();
    order.js_sort_by(|x, y| ordering(grip_z(&[x]) - grip_z(&[y])));
    for q in order {
        // (`alone`: the swap replaces the region's walls, none stay beside it)
        if alone || q.line.is_empty() {
            continue;
        }
        let clear = kept
            .iter()
            .all(|r| plan_gap(&q.line, &r.line) >= half_foot(q, cfg) + half_foot(r, cfg) + 1.0);
        if !clear {
            continue;
        }
        let adds_area = held_by(&bare, &q.line, cfg) >= RACE_ABS;
        let adds_grip = grip_z(&[q]) < grip_z(&kept) - GRIP_RISE;
        if !adds_area && !adds_grip {
            continue;
        }
        extra.push(q);
        kept.push(q);
        let mut i = bare.len();
        while i > 0 {
            i -= 1;
            let h = held_by(&[bare[i]], &q.line, cfg);
            if h != 0.0 && !h.is_nan() {
                bare.remove(i);
            }
        }
    }
    let za = grip_z(qa);
    if !force && grip_z(&kept) > za + grip_rise(za) {
        None
    } else {
        Some(extra)
    }
}

/// Port of `raceRegions` (web/prop/raster.js:333-392): per region, keep the normal pass `a` or
/// the raster pass `b` (plus the normal walls `keepers` saves): the raster pass must hold
/// RACE_REL / RACE_ABS more of the faces near the region. Returns buildProps's shape and
/// `<tag>Regions` = how many regions swapped.
///
/// `skipped` stays the normal pass's: a region the raster pass rescued may still be counted as
/// skipped there. Over-reporting a skip is the safe direction -- the readout never claims a
/// region was served that wasn't.
///
/// `lenient` regions swap whenever their `b` walls hold anything; any other region races as
/// usual, and only if it is in `wanted` (None = all) -- the ones `a` left bare.
///
/// `b.leaderOf` (Curved walls, prop/trace.js): regions traced as one surface race as one, at
/// their leader's turn -- every member's `a` walls against the group's `b` walls, over the faces
/// near any member -- since a line traced across the slivers runs under its neighbours' red,
/// where their own walls stand. Such a pass only forces curves: a group whose walls don't bend
/// keeps its `a` walls.
fn race_regions(
    faces: &[OverFace],
    result: &Analysis,
    a: &PropsPass,
    b: &PropsPass,
    tag: RaceTag,
    max_grow: Option<f64>,
    rel: f64,
    lenient: Option<Lenient>,
    wanted: Option<&IndexSet<usize>>,
    equivalent: bool,
    ruler: Ruler,
    price: Option<f64>,
    hold: Option<Hold>,
    alone: bool,
    cfg: &Config,
) -> (PropsPass, usize) {
    let (aa, bb) = (by_region(&a.props), by_region(&b.props));
    let mut out: Vec<crate::types::V4> = Vec::new();
    let mut props: Vec<PropRecord> = Vec::new();
    let mut served: IndexSet<usize> = IndexSet::new();
    let (mut tines, mut volume, mut swapped) = (0usize, 0.0f64, 0usize);
    let mut sag_risk = a.sag_risk;
    // `take(build, q)`
    let mut take = |from_b: bool,
                    q: &PropRecord,
                    out: &mut Vec<crate::types::V4>,
                    props: &mut Vec<PropRecord>,
                    served: &mut IndexSet<usize>| {
        let build = if from_b { b } else { a };
        let mut tri_ranges = Vec::new();
        for [s, e] in &q.tri_ranges {
            let r0 = out.len();
            for i in *s..*e {
                out.push(build.triangles[i]);
            }
            tri_ranges.push([r0, out.len()]);
        }
        let mut np = q.clone();
        np.id = props.len();
        np.tri_ranges = tri_ranges;
        tag.set(&mut np, from_b);
        props.push(np);
        served.extend(q.region);
        tines += q.tines;
        volume += q.volume;
    };
    let mut members: IndexMap<usize, Vec<usize>> = IndexMap::new();
    if let Some(lo) = &b.leader_of {
        for (&ri, &l) in lo {
            members.entry(l).or_default().push(ri);
        }
    }
    // Each group traces on its own, so two groups' lines can meet in plan (stacked surfaces,
    // bean on its side: 0.15-0.43 mm apart): a group's wall that comes within a foot + 1 mm of a
    // wall an earlier group swapped in is dropped, the rest stand
    let mut forced: Vec<&PropRecord> = Vec::new();
    for ri in 0..result.regions.len() {
        cfg.cancel.check();
        let group = members.get(&ri);
        if b.leader_of.as_ref().is_some_and(|l| l.contains_key(&ri)) && group.is_none() {
            continue; // raced with its group's leader
        }
        let qa: Vec<&PropRecord> = match group {
            Some(g) => g
                .iter()
                .flat_map(|m| aa.get(m).cloned().unwrap_or_default())
                .collect(),
            None => aa.get(&ri).cloned().unwrap_or_default(),
        };
        let clear_of_forced = |q: &PropRecord, forced: &[&PropRecord]| {
            forced
                .iter()
                .all(|r| plan_gap(&q.line, &r.line) >= half_foot(q, cfg) + half_foot(r, cfg) + 1.0)
        };
        let qb: Vec<&PropRecord> = bb
            .get(&ri)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|q| group.is_none() || clear_of_forced(q, &forced))
            .collect();
        let mut extra: Option<Vec<&PropRecord>> = None;
        if !qb.is_empty() {
            let near_idx: Vec<usize> = match group {
                Some(g) if g.len() > 1 => {
                    let mut set: IndexSet<usize> = IndexSet::new();
                    for m in g {
                        set.extend(near_region(faces, *m, cfg));
                    }
                    set.into_iter().collect()
                }
                _ => near_region(faces, ri, cfg),
            };
            let near: Vec<OverFace> = near_idx.iter().map(|&i| faces[i]).collect();
            let ha = if qa.is_empty() {
                0.0
            } else {
                ruler(&near, &wall_tops(&qa))
            };
            let hb = ruler(&near, &wall_tops(&qb));
            let vol = |qs: &[&PropRecord]| qs.iter().fold(0.0f64, |v, q| v + q.volume);
            if let Some(len) = lenient.filter(|l| l(&qb)) {
                let _ = len;
                // forced (raceAgainst's `lenient`): the user asked for the curve; the normal
                // walls that hold what it misses, or grip lower, stand beside it (keepers)
                if hb > 0.0 {
                    extra = keepers(&qa, &qb, &near, true, false, cfg);
                }
            } else if b.leader_of.is_none() && wanted.is_none_or(|w| w.contains(&ri)) {
                // a traced group only forces a curve
                // (`price`: held area per mm3 the swap must buy when it adds plastic)
                let too_heavy = (max_grow
                    .is_some_and(|g| !qa.is_empty() && vol(&qb) > g * vol(&qa)))
                    || price.is_some_and(|p| hb - ha < p * (vol(&qb) - vol(&qa)));
                // (`hold`: the walls `a` would lose something the swap keeps -- a wedge --
                // so it swaps whatever the ruler says)
                if (!too_heavy
                    && (hb > ha + jsnum::max(RACE_ABS, rel * ha)
                        || (equivalent && hb > 0.0 && hb >= ha - 1e-6)))
                    || hold.is_some_and(|h| hb > 0.0 && h(&qa, &qb, ri))
                {
                    extra = keepers(&qa, &qb, &near, false, alone, cfg);
                }
            }
        }
        let Some(extra) = extra else {
            for q in &qa {
                take(false, q, &mut out, &mut props, &mut served);
            }
            continue;
        };
        swapped += 1;
        sag_risk = sag_risk || b.sag_regions.as_ref().is_some_and(|s| s.contains(&ri));
        for q in &qb {
            take(true, q, &mut out, &mut props, &mut served);
        }
        if group.is_some() {
            forced.extend(qb.iter().copied());
        }
        // a group's other regions are served where its walls reach their own faces (take counts
        // the leader only); one the curves miss stays bare for the last resort
        for m in group.into_iter().flatten() {
            if *m != ri {
                let own: Vec<OverFace> = faces
                    .iter()
                    .filter(|f| f[4] == *m as f64)
                    .copied()
                    .collect();
                if held_by(&own, &wall_tops(&qb), cfg) > 0.0 {
                    served.insert(*m);
                }
            }
        }
        for q in &extra {
            take(false, q, &mut out, &mut props, &mut served);
        }
    }
    (
        PropsPass {
            triangles: out,
            props,
            skipped: a.skipped,
            served: served.len(),
            served_regions: served.into_iter().collect(),
            tines,
            sag_risk,
            volume,
            ..Default::default()
        },
        swapped,
    )
}
