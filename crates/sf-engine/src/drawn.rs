//! Port of the geometry in `web/ui/walls.js`: the hand-placed supports (breakaway walls drawn in Draw
//! mode or by the "+ Add walls by hand" augment, and sway braces stood with one click).
//!
//! `walls.js` is a UI module, but these pieces are maths: `sweepDrawn` re-stands every hand-placed
//! support against the part's current pose, `autoSupports` turns Auto's build into the things a hand
//! brace must keep clear of, `braceAt` is the brace a click at a spot stands, and `arcPoints` is the
//! band a curve in progress shows. The pointer and the pixels (the cursor, the snap, the markers) stay
//! in the app; the ray test that turns a pointer ray into a spot on the surface (three.js's
//! `Raycaster` in the web) lives here too, so the app only has to say which face the GPU picked.
//!
//! Read `docs/AGENTS.md` before changing anything here.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::Arc;

use crate::config::Config;
use crate::draw::{DrawOpts, DrawnOk, DrawnWall, drawn_wall};
use crate::drawarc::{arc_stations, arc_through};
use crate::fins::build::{FinKind, FinsBuilt, SwayBrace};
use crate::fins::tall::PlanGrid;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::config::Iface;
use crate::prop::crest::Crest;
use crate::prop::ctx::BuildCtx;
use crate::sway::{Avoid, SwayOpts, SwayRib, face_is_upright, sway_at_face};
use crate::tris::Tris;
use crate::types::{Rot3, V4, Xyz};

type V3 = [f64; 3];

/// `BAND_MAX` (web/ui/walls.js:81): the band's buffer holds this many points.
pub const BAND_MAX: usize = 129;

/// `SNAP_TOP` (web/ui/walls.js:469): in the top quarter of what a brace could reach there it snaps to
/// the whole face.
const SNAP_TOP: f64 = 0.75;
/// `STEP_Z` (web/ui/walls.js:470): capped tops in 1 mm steps, so a creeping cursor reuses its ghost.
const STEP_Z: f64 = 1.0;

/// What a hand-placed support is (`w.kind === 'sway'` or not).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandKind {
    Wall,
    Sway,
}

/// One committed hand-placed support, as `drawnWalls` holds it (web/ui/walls.js:27): endpoints in the
/// part's LOCAL frame, so it tracks the part through later rotations.
#[derive(Clone, Debug, PartialEq)]
pub struct HandWall {
    pub kind: HandKind,
    /// `a`: a wall's first click, or a brace's spot
    pub a: V3,
    /// `b`: a wall's second end (unused by a brace)
    pub b: V3,
    /// `m`: a curve's third click
    pub m: Option<V3>,
    /// `face`: the face a brace stands on
    pub face: usize,
    /// `capped`: a brace that stops at its clicked point's height
    pub capped: bool,
}

/// The sway-brace settings `swayOpts()` reads off the page (web/ui/finbuild.js:156-170), the part of
/// them that is a control and not the build's tunables (those are read from the `Config`).
#[derive(Clone, Debug, PartialEq)]
pub struct HandSway {
    pub uncapped_depth: bool,
    pub grip_from: f64,
    pub tine_spacing: f64,
    pub reach: f64,
    pub cross_rise: f64,
}

/// The controls `sweepDrawn` reads (web/ui/walls.js:253-260 `drawOpts`, and `swayOpts()`).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawSettings {
    /// `el('tines').checked`
    pub tines: bool,
    /// `el('tine-density').valueAsNumber / 100`
    pub tine_density: f64,
    /// `el('layer-height').valueAsNumber`
    pub layer_height: f64,
    /// `el('plate-only').checked`
    pub plate_only: bool,
    /// `el('tall-ribs').checked`
    pub brace_tall: bool,
    pub sway: HandSway,
}

/// Port of `swayOpts` (web/ui/finbuild.js:156-170): the options a hand-placed brace is built under.
/// `FIN.roundFeet`, `PROP.gap`, `PROP.iface` and `BRACE.tallH` come from `cfg`, which the caller has
/// applied the page's tunables to (as the page's modules hold them).
pub fn sway_opts<'a>(s: &DrawSettings, cfg: &Config) -> SwayOpts<'a> {
    SwayOpts {
        round_feet: cfg.fin.round_feet,
        uncapped_depth: s.sway.uncapped_depth,
        grip_from: Some(s.sway.grip_from),
        tine_spacing: Some(s.sway.tine_spacing),
        reach: Some(s.sway.reach),
        cross_rise: Some(s.sway.cross_rise),
        gap: Some(cfg.prop.gap),
        tines: Some(s.tines),
        iface_all: cfg.prop.iface == Iface::All,
        layer_height: Some(s.layer_height),
        // "Brace tall walls" cross-ribs a brace over its start height, the same threshold the drawn
        // walls use; unticked leaves the brace plain.
        cross_above: Some(if s.brace_tall {
            cfg.brace.tall_h
        } else {
            f64::INFINITY
        }),
        ..Default::default()
    }
}

/// Port of `printTriangles` (web/ui/walls.js:121-131): `topology` posed `rot` and seated at `offset`
/// (analyze's), as one flat array.
pub fn print_triangles(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Vec<f64> {
    let pos = &topo.pos;
    let mut a = vec![0.0; topo.n_faces * 9];
    let mut i = 0;
    while i < a.len() {
        let (x, y, z) = (pos[i], pos[i + 1], pos[i + 2]);
        a[i] = rot[0] * x + rot[3] * y + rot[6] * z + offset.x;
        a[i + 1] = rot[1] * x + rot[4] * y + rot[7] * z + offset.y;
        a[i + 2] = rot[2] * x + rot[5] * y + rot[8] * z + offset.z;
        i += 3;
    }
    a
}

/// `part.localToWorld(v)`: the part's frame to print space (`rot * v + offset`).
pub fn to_world(rot: &Rot3, offset: &Xyz, v: V3) -> V3 {
    [
        rot[0] * v[0] + rot[3] * v[1] + rot[6] * v[2] + offset.x,
        rot[1] * v[0] + rot[4] * v[1] + rot[7] * v[2] + offset.y,
        rot[2] * v[0] + rot[5] * v[1] + rot[8] * v[2] + offset.z,
    ]
}

/// `part.worldToLocal(v)`: print space back to the part's frame (`rot^T * (v - offset)`; `rot` is a
/// pure rotation).
pub fn to_local(rot: &Rot3, offset: &Xyz, v: V3) -> V3 {
    let d = [v[0] - offset.x, v[1] - offset.y, v[2] - offset.z];
    [
        rot[0] * d[0] + rot[1] * d[1] + rot[2] * d[2],
        rot[3] * d[0] + rot[4] * d[1] + rot[5] * d[2],
        rot[6] * d[0] + rot[7] * d[1] + rot[8] * d[2],
    ]
}

/// What Auto's build contributes for a hand brace to stay clear of: `autoSupports`' result.
#[derive(Clone, Debug, Default)]
pub struct AutoAvoid {
    pub braces: Vec<SwayRib>,
    pub walls: Vec<Vec<V3>>,
}

/// An outline of Auto's brace (`built.sway.braces[k]`) as the `avoid.braces` entry the clash tests
/// read: JS passes the plain object (`foot`, `halfW`, `th`, `height`, `levels`, `arms`) as it is.
fn outline_rib(b: &SwayBrace) -> SwayRib {
    SwayRib {
        tris: Vec::new(),
        tines: 0,
        height: b.height,
        depth: 0.0,
        th: b.th,
        stilt: 0.0,
        foot: b.foot,
        half_w: b.half_w,
        levels: b.levels.clone(),
        cross: 0,
        arms: b.arms.clone(),
        profiled: false,
        tri_range: None,
    }
}

/// Port of `autoSupports` (web/ui/walls.js:432-446): the supports Auto has ALREADY placed, as things
/// a hand-placed brace must avoid. Auto builds in a worker, so the page has no other way to know where
/// its braces and walls stand: without this, a brace you click can land on top of an Auto one (or face
/// it across a channel) and the two fuse into one piece that won't break away. Empty unless Auto's
/// supports are actually on screen (the caller passes `None`), and fins the user has removed are left
/// out -- they aren't there to hit.
pub fn auto_supports(built: Option<&FinsBuilt>, removed: &HashSet<usize>) -> AutoAvoid {
    let Some(built) = built else {
        return AutoAvoid::default();
    };
    let outlines: &[SwayBrace] = built.sway.as_ref().map_or(&[], |s| &s.braces);
    let mut out = AutoAvoid::default();
    let mut k = 0; // sway records and their outlines are emitted in the same order
    for rec in &built.fins {
        let gone = removed.contains(&rec.id);
        if rec.kind == FinKind::Sway {
            let outline = outlines.get(k);
            k += 1;
            if let Some(outline) = outline
                && !gone
            {
                out.braces.push(outline_rib(outline));
            }
        } else if !gone && !rec.line.is_empty() {
            out.walls.push(rec.line.clone());
        }
    }
    out
}

/// What became of one hand-placed support in a sweep: `w.ok`, `w.info`, and its triangles' range.
#[derive(Clone, Debug, PartialEq)]
pub struct HandOutcome {
    /// `w.ok`
    pub ok: bool,
    /// `w.triStart` / `w.triEnd`: triangle indices into the sweep's triangles
    pub tri_start: usize,
    pub tri_end: usize,
    pub info: HandInfo,
}

/// `w.info`: `drawnWall`'s or `swayAtFace`'s result.
#[derive(Clone, Debug, PartialEq)]
pub enum HandInfo {
    Wall(Box<DrawnOk>),
    Brace(Box<SwayRib>),
    /// not built: why (`info.reason`)
    Refused(String),
}

/// What `sweepDrawn` returns plus the `w.ok` / `w.info` / range it sets on each wall.
#[derive(Clone, Debug, Default)]
pub struct Sweep {
    /// the vertices (`out`), three to a triangle
    pub tris: Vec<V4>,
    pub walls: Vec<HandOutcome>,
}

/// Port of `sweepDrawn` (web/ui/walls.js:214-299): stand a model's hand-placed supports for its pose,
/// under the settings on the page. `auto` is Auto's build on screen (or `None`), `gone` its removed fin
/// ids; `tris` the part in print space ([`print_triangles`]).
pub fn sweep_drawn(
    topo: &Topology,
    res: &Analysis,
    rot: &Rot3,
    tris: &Arc<Tris>,
    walls: &[HandWall],
    settings: &DrawSettings,
    auto: Option<&FinsBuilt>,
    gone: &HashSet<usize>,
    cfg: &Config,
) -> Sweep {
    let out: RefCell<Vec<V4>> = RefCell::new(Vec::new());
    // Both Draw and the Suggest "+ Add" augment place the SAME thing now: hand-drawn under-overhang
    // breakaway walls. Each wall is stored as two local endpoints and re-swept against the part's
    // current pose, so a wall that no longer reaches the part (rotated away) is flagged by drawnWall
    // rather than dropped silently. Drawn walls grip with the same tine comb the auto fins use when
    // Tines is on.
    // A tall drawn wall's ribs keep sideClear off the walls drawn before it and off Auto's supports and
    // pad when they're on screen (draw.js braceTall): a rib fused into a neighbour no longer breaks
    // away by itself. (Walls drawn later don't check back -- drawn walls never check each other -- and
    // Auto's ribs, built in the Worker, can't see drawn walls.)
    let rib_grid = RefCell::new(PlanGrid::new());
    if let Some(built) = auto {
        for rec in &built.fins {
            if gone.contains(&rec.id) {
                continue;
            }
            for &[a, b] in &rec.tri_ranges {
                rib_grid
                    .borrow_mut()
                    .add(&built.triangles[a..b], Some(0), |_, _, _| true);
            }
        }
        if !built.pad_triangles.is_empty() {
            rib_grid
                .borrow_mut()
                .add(&built.pad_triangles, Some(0), |_, _, _| true);
        }
    }
    let gridded = Cell::new(0usize);
    let brace_clear = |rib: &[V4]| -> bool {
        // walls drawn since the last rib
        let o = out.borrow();
        rib_grid
            .borrow_mut()
            .add(&o[gridded.get()..], None, |_, _, _| true);
        gridded.set(o.len());
        !rib_grid.borrow().hits(rib, None, cfg)
    };
    let draw_opts = DrawOpts {
        tines: settings.tines,
        tine_density: Some(settings.tine_density),
        layer_height: Some(settings.layer_height),
        plate_only: settings.plate_only,
        brace_tall: settings.brace_tall,
        brace_clear: Some(&brace_clear),
        topo: Some(topo),
        rot: Some(*rot),
        offset: Some(res.offset),
        ..Default::default()
    };
    // Everything a hand-placed brace has to keep clear of: Auto's braces and walls (when Auto's
    // supports are on screen), then each hand-placed brace as it is re-stood, so a rotation that brings
    // two together is reported rather than fused.
    let avoid = auto_supports(auto, gone);
    let mut braces: Vec<SwayRib> = avoid.braces.clone();
    let mut outcomes: Vec<HandOutcome> = Vec::with_capacity(walls.len());
    for w in walls {
        // Each support remembers which triangles of the merged mesh are its own, so a click on the mesh
        // can be traced back to the support to select / remove.
        let tri_start = out.borrow().len() / 3;
        if w.kind == HandKind::Sway {
            // A hand-placed sway brace: re-stood on the same face at the same spot, so it follows the
            // part when it turns (and says why if that face no longer stands upright, or now runs
            // into an earlier brace). A capped one stops at its clicked point's height; the rest run
            // up the whole face.
            let wa = to_world(rot, &res.offset, w.a);
            let mut o = sway_opts(settings, cfg);
            o.full = !w.capped;
            let r = sway_at_face(
                topo,
                res,
                rot,
                w.face,
                wa,
                &o,
                &Avoid {
                    braces: &braces,
                    walls: &avoid.walls,
                },
                cfg,
            );
            let (ok, info) = match r {
                Ok(rib) => {
                    braces.push(rib.clone());
                    out.borrow_mut().extend(rib.tris.iter().copied());
                    (true, HandInfo::Brace(Box::new(rib)))
                }
                Err(reason) => (false, HandInfo::Refused(reason)),
            };
            outcomes.push(HandOutcome {
                ok,
                tri_start,
                tri_end: out.borrow().len() / 3,
                info,
            });
            continue;
        }
        let wa = to_world(rot, &res.offset, w.a);
        let wb = to_world(rot, &res.offset, w.b);
        let mut opts = draw_opts.clone();
        if let Some(m) = w.m {
            opts.through = Some(to_world(rot, &res.offset, m));
        }
        let mut ctx = BuildCtx::new(cfg, Crest::new(None, 0.0, cfg));
        let r = drawn_wall(wa, wb, tris, 0.0, &opts, &mut ctx);
        let (ok, info) = match r {
            DrawnWall::Ok(d) => {
                out.borrow_mut().extend(d.tris.iter().copied());
                (true, HandInfo::Wall(Box::new(d)))
            }
            DrawnWall::Refused { reason } => (false, HandInfo::Refused(reason)),
        };
        outcomes.push(HandOutcome {
            ok,
            tri_start,
            tri_end: out.borrow().len() / 3,
            info,
        });
    }
    Sweep {
        tris: out.into_inner(),
        walls: outcomes,
    }
}

/// The brace a click stands (`braceAt`'s result, web/ui/walls.js:497-513): `{ r, full, point, key }`.
#[derive(Clone, Debug)]
pub struct BraceSpot {
    /// `r`: `swayAtFace`'s result
    pub rib: Result<SwayRib, String>,
    pub full: bool,
    /// `point`: the spot it stands at (x, y to the mm, a capped top to `STEP_Z`), which a placed brace stores
    pub point: V3,
    pub key: String,
}

/// Port of `braceAt` (web/ui/walls.js:471-513) without its memo (a cache, not maths): the brace a click
/// at print-space point `p` on face `face_index` stands. `drawn` are the hand braces already standing
/// (`drawnWalls.filter(sway && ok).map(info)`), `auto` Auto's supports (`autoSupports`).
pub fn brace_at(
    topo: &Topology,
    res: &Analysis,
    rot: &Rot3,
    face_index: usize,
    p: V3,
    settings: &DrawSettings,
    drawn: &[SwayRib],
    auto: &AutoAvoid,
    cfg: &Config,
) -> BraceSpot {
    let opts = sway_opts(settings, cfg);
    let mut braces: Vec<SwayRib> = auto.braces.clone();
    braces.extend(drawn.iter().cloned());
    let avoid = Avoid {
        braces: &braces,
        walls: &auto.walls,
    };
    // x, y to the mm, slid back onto the face's plane: the stored point has to stay on the face, or a
    // reopened session's nearestFace can pick the one beside it
    let n = &topo.nrm;
    let f = face_index * 3;
    let nx = rot[0] * n[f] + rot[3] * n[f + 1] + rot[6] * n[f + 2];
    let ny = rot[1] * n[f] + rot[4] * n[f + 1] + rot[7] * n[f + 2];
    let rx = jsnum::round(p[0]);
    let ry = jsnum::round(p[1]);
    let denom = nx * nx + ny * ny;
    // `(nx * nx + ny * ny || 1)`: 0 and NaN fall to 1
    let denom = if denom == 0.0 || denom.is_nan() {
        1.0
    } else {
        denom
    };
    let off = ((rx - p[0]) * nx + (ry - p[1]) * ny) / denom;
    let x = rx - off * nx;
    let y = ry - off * ny;
    let at = format!(
        "{}|{}|{}",
        face_index,
        jsnum::to_js_string(rx),
        jsnum::to_js_string(ry)
    );
    let mut full_opts = opts.clone();
    full_opts.full = true;
    let full = sway_at_face(
        topo,
        res,
        rot,
        face_index,
        [x, y, p[2]],
        &full_opts,
        &avoid,
        cfg,
    );
    let full_height = match &full {
        Ok(r) => Some(r.height),
        Err(_) => None,
    };
    // `!full.ok || p[2] >= SNAP_TOP * full.height`
    if full_height.is_none_or(|h| p[2] >= SNAP_TOP * h) {
        return BraceSpot {
            rib: full,
            full: true,
            point: [x, y, p[2]],
            key: at,
        };
    }
    let z = jsnum::max(STEP_Z, jsnum::round(p[2] / STEP_Z) * STEP_Z);
    let key = format!("{at}|{}", jsnum::to_js_string(z));
    BraceSpot {
        rib: sway_at_face(topo, res, rot, face_index, [x, y, z], &opts, &avoid, cfg),
        full: false,
        point: [x, y, z],
        key,
    }
}

/// `faceIsUpright(topology, rot, faceIndex)` as the walls.js `swayHit` asks it: a click on an upright
/// side stands a brace (web/ui/walls.js:716).
pub fn face_takes_brace(topo: &Topology, rot: &Rot3, face_index: usize, cfg: &Config) -> bool {
    face_is_upright(topo, rot, face_index, cfg)
}

/// Port of `arcPoints` (web/ui/walls.js:154-160): the points round the arc `a` -> `b` through `m`
/// (world), for the band; `[a, b]` when the three are in line. At most [`BAND_MAX`].
pub fn arc_points(a: V3, b: V3, m: V3) -> Vec<V3> {
    let Some(c) = arc_through([a[0], a[1]], [b[0], b[1]], [m[0], m[1]]) else {
        return vec![a, b];
    };
    let step = jsnum::max(0.25, (jsnum::abs(c.sweep) * c.r) / ((BAND_MAX - 1) as f64));
    let mut pts = arc_stations(a, b, m, step, 3.0).unwrap_or_default();
    pts.truncate(BAND_MAX);
    pts
}

// ---- the pointer ray -------------------------------------------------------

/// Where a ray meets a triangle (Moller-Trumbore, both sides, as three.js's `Mesh.raycast` does for a
/// double-sided material): the distance along `dir`, or `None`.
pub fn ray_triangle(origin: V3, dir: V3, a: V3, b: V3, c: V3) -> Option<f64> {
    let sub = |p: V3, q: V3| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let cross = |p: V3, q: V3| {
        [
            p[1] * q[2] - p[2] * q[1],
            p[2] * q[0] - p[0] * q[2],
            p[0] * q[1] - p[1] * q[0],
        ]
    };
    let dot = |p: V3, q: V3| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let t0 = sub(origin, a);
    let u = dot(t0, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(t0, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t > 0.0).then_some(t)
}

/// A spot on the part's surface under a pointer ray (`pickFace`'s `hit`, web/ui/pose.js:264-272):
/// where, and how far along the ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    /// print space
    pub point: V3,
    /// the ray parameter (unit `dir`: mm)
    pub distance: f64,
}

/// The point on face `face` (of the part posed `rot`, seated at `offset`) a print-space ray meets.
/// The GPU's id pass says WHICH face is under the pointer; this says where on it, which is what the
/// web's raycast returns as `hit.point`. `None` when the ray misses that triangle (the pass and the ray
/// disagreeing by a pixel at an edge): the caller keeps the last spot.
pub fn surface_hit(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    face: usize,
    origin: V3,
    dir: V3,
) -> Option<SurfaceHit> {
    if face >= topo.n_faces {
        return None;
    }
    let v = |k: usize| {
        let i = face * 9 + k * 3;
        to_world(rot, offset, [topo.pos[i], topo.pos[i + 1], topo.pos[i + 2]])
    };
    let t = ray_triangle(origin, dir, v(0), v(1), v(2))?;
    Some(SurfaceHit {
        point: [
            origin[0] + dir[0] * t,
            origin[1] + dir[1] * t,
            origin[2] + dir[2] * t,
        ],
        distance: t,
    })
}

/// Port of `pickSupport` (web/ui/walls.js:339-350): the hand-placed support under a pointer ray, if it
/// is nearer than the part (`part_distance`, the part's own hit along the same ray, if any). Returns
/// its index in `sweep.walls`. The first nearest triangle decides, as the raycaster's sorted hits do.
pub fn pick_support(
    sweep: &Sweep,
    origin: V3,
    dir: V3,
    part_distance: Option<f64>,
) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for (i, t) in sweep.tris.as_chunks::<3>().0.iter().enumerate() {
        let p = |k: usize| [t[k][0], t[k][1], t[k][2]];
        if let Some(d) = ray_triangle(origin, dir, p(0), p(1), p(2))
            && best.is_none_or(|(bd, _)| d < bd)
        {
            best = Some((d, i));
        }
    }
    let (d, tri) = best?;
    if part_distance.is_some_and(|pd| pd < d) {
        return None; // the part is in front
    }
    sweep
        .walls
        .iter()
        .position(|w| w.ok && tri >= w.tri_start && tri < w.tri_end)
}
