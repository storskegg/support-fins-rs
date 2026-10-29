//! Port of `web/sway.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! SWAY BRACES -- buttress ribs that keep a TALL part from drifting, sagging and wobbling as it
//! grows. A sway brace is the RIB (a vertical plate standing PERPENDICULAR to an upright face,
//! tapering from deep at the bed to narrow at the top), the FOOT (a thin flange so the rib stays
//! down) and the TINES (one-layer horizontal bridges from the rib's inner edge into the part,
//! spaced EVENLY up the whole height). The gap arrives in `opts` rather than being read from
//! FIN/PROP, so the module depends on neither.

use indexmap::IndexMap;
use std::cmp::Ordering;
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::fins::build::{SwayArm, SwayLevel};
use crate::fins::seating::pose_key;
use crate::inside::inside_part;
use crate::jsnum;
use crate::kiss::{KissOpts, kiss_ends};
use crate::overhangs::{Analysis, Topology};
use crate::planes::{
    MAX_LEAN_DEG, Patch, find_wall_patches, patch_point, patch_probe, t_at_z, z_at,
};
use crate::prop::brace::brace_cross_rib;
use crate::prop::config::TineDir;
use crate::solids::{Radius, loft_extrude, ribbon, rounded_polygon};
use crate::swaycrop::crop_for_rib;
use crate::swayhit::{clip, lowest_hit};
use crate::tris::Tris;
use crate::types::{Pt3, Rot3, V4, Xyz};

type V3 = [f64; 3];

/// `SWAY` (web/sway.js:40). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayConfig {
    /// JS key `maxLeanDeg`.
    // which faces take a brace
    // a face leaning further than this is an overhang or a roof, not a side
    pub max_lean_deg: f64,
    /// JS key `maxBackLeanDeg`.
    // ...unless it faces UP: the back of a leaning arm (GH #351). A brace placed by hand
    // may anchor that, its inner edge leaning with the face as an overhang of its own,
    // so up to the most a wall leans and still prints (planes.js MAX_LEAN_DEG, 45).
    // Auto keeps to maxLeanDeg both ways.
    pub max_back_lean_deg: f64,
    /// JS key `minFaceH`.
    // mm of face height before auto-placement bothers bracing it
    pub min_face_h: f64,
    /// JS key `minTopFrac`.
    // ...and it must reach this far up the part, or it braces the easy half
    pub min_top_frac: f64,
    /// JS key `minPartH`.
    // mm; a shorter part doesn't sway enough to need this
    pub min_part_h: f64,
    /// JS key `thMin`.
    // the rib
    // wall thickness at the minimum height (the fins' own wall)
    pub th_min: f64,
    /// JS key `thPerMm`.
    // grows with height: 1.2mm at 0, 2.2mm at 250mm
    pub th_per_mm: f64,
    /// JS key `thMax`.
    pub th_max: f64,
    // DEFAULT base depth as a fraction of rib height (UI: "Brace depth")
    pub reach: f64,
    /// JS key `topDepth`.
    // mm at the top: a flat edge, not a point (a point is a retraction)
    pub top_depth: f64,
    /// JS key `minDepth`.
    pub min_depth: f64,
    /// JS key `maxDepth`.
    pub max_depth: f64,
    /// JS key `topClear`.
    // the rib stops this far below the top of the face it braces
    pub top_clear: f64,
    /// JS key `minRibH`.
    // a shorter rib isn't bracing anything
    pub min_rib_h: f64,
    /// JS key `footH`.
    // the foot
    pub foot_h: f64,
    /// JS key `footHalf`.
    // flange past the rib on each side
    pub foot_half: f64,
    /// JS key `footPad`.
    // ...and past its outer end
    pub foot_pad: f64,
    // the tines (the standoff comes from the material profile, via opts)
    pub gap: f64,
    /// JS key `tineReach`.
    // How far in from the face a tine looks for the part: placed only if the part is
    // there at half this, and its end stops on the surface (kissEnds, searching up to
    // 2x). Was the material's tine bite (PLA 0.3, PETG 0.15), when tines ran that far
    // INTO the part; with the end on the surface the reach only gates placement, and
    // the slicer merges part and brace either way (local issue 027).
    pub tine_reach: f64,
    /// JS key `tineW`.
    // one nozzle bead, as elsewhere
    pub tine_w: f64,
    /// JS key `tineOverlap`.
    // how far the tine sinks back into the rib, so they union
    pub tine_overlap: f64,
    /// JS key `tineSpanMax`.
    // past this much open air a tine is a bridge (probe_tines2.py)
    pub tine_span_max: f64,
    /// JS key `tineSpacing`.
    // DEFAULT mm between tines up the rib (UI: "Tine spacing")
    pub tine_spacing: f64,
    /// JS key `minTines`.
    pub min_tines: f64,
    /// JS key `minGripShare`.
    // ...and at least this share of the rows up the rib must find the face
    pub min_grip_share: f64,
    /// JS key `stiltMax`.
    // ...and it may not climb this far before its FIRST tine. Below its lowest grip a
    // brace is a lone wall: it prints for hours holding nothing, and nothing holds it
    // either, so it is free to wobble exactly while the part beside it is at its most
    // delicate. A part tilted up onto a corner puts every face high off the plate and
    // is the case this catches (raised by Matthew on #29, with a 50-degree post).
    // Both bounds: the fraction keeps a short rib from being mostly stilt, the
    // absolute stops a very tall one from growing an unreasonable one.
    pub stilt_max: f64,
    /// JS key `stiltMaxFrac`.
    pub stilt_max_frac: f64,
    // auto-placement
    // mm of face width per rib
    pub pitch: f64,
    /// JS key `maxPerFace`.
    pub max_per_face: f64,
    /// JS key `endInset`.
    // keep ribs near the ends of a face (FIN-SPEC: edges hide marks)
    pub end_inset: f64,
    /// JS key `maxFaces`.
    pub max_faces: f64,
    /// JS key `minBearingSep`.
    // faces this far apart in bearing, so opposite + adjacent sides both count
    pub min_bearing_sep: f64,
    pub nudges: Vec<f64>,
    // mm of air two braces must keep between them
    pub clearance: f64,
    /// JS key `levelStep`.
    // mm between the heights a clash check compares
    pub level_step: f64,
    /// JS key `wallHalf`.
    // Half-width to assume for a prop wall or wedge the braces must stay clear of.
    // Their own records carry a centreline, not a footprint, so this is the widest
    // foot either builds (PROP/PERP footHalf 3.0 + half a 1.2mm wall) rounded up.
    pub wall_half: f64,
}

impl Default for SwayConfig {
    fn default() -> Self {
        SwayConfig {
            max_lean_deg: 30.0,
            max_back_lean_deg: MAX_LEAN_DEG,
            min_face_h: 30.0,
            min_top_frac: 0.4,
            min_part_h: 30.0,
            th_min: 1.2,
            th_per_mm: 0.004,
            th_max: 2.4,
            reach: 0.15,
            top_depth: 4.0,
            min_depth: 8.0,
            max_depth: 60.0,
            top_clear: 1.0,
            min_rib_h: 20.0,
            foot_h: 0.6,
            foot_half: 3.0,
            foot_pad: 3.0,
            gap: 0.2,
            tine_reach: 0.3,
            tine_w: 0.5,
            tine_overlap: 0.3,
            tine_span_max: 1.5,
            tine_spacing: 6.0,
            min_tines: 3.0,
            min_grip_share: 0.3,
            stilt_max: 40.0,
            stilt_max_frac: 0.4,
            pitch: 100.0,
            max_per_face: 4.0,
            end_inset: 10.0,
            max_faces: 4.0,
            min_bearing_sep: 60.0,
            nudges: vec![0.0, 3.0, -3.0, 6.0, -6.0, 10.0, -10.0],
            clearance: 1.0,
            level_step: 10.0,
            wall_half: 3.6,
        }
    }
}

/// `SwayPatches` is the `patchCache` WeakMap's entry (web/sway.js:763-775) for one topology: the
/// patches of the last pose asked about, so a click in Draw does not re-run the face search. The
/// slot in `TopoCache` is a `OnceLock`, so the entry itself sits in a `Mutex` that replaces it
/// when the pose changes.
#[derive(Debug, Default)]
pub struct SwayPatches {
    entry: Mutex<Option<Arc<PatchEntry>>>,
}

/// `{ key, patches, byFace, partTris }` (web/sway.js:773).
#[derive(Debug)]
struct PatchEntry {
    key: String,
    patches: Vec<Patch>,
    /// `byFace`: a face to the index of the first patch that lists it.
    by_face: IndexMap<usize, usize>,
    part_tris: Arc<Tris>,
}

/// JS `a - b` as a sort comparator result: negative, positive, and 0 or NaN (both "equal").
fn num_ord(d: f64) -> Ordering {
    if d < 0.0 {
        Ordering::Less
    } else if d > 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// JS `x || y` on two numbers (0 and NaN are falsy).
fn or(x: f64, y: f64) -> f64 {
    if x != 0.0 && !x.is_nan() { x } else { y }
}

/// `opts.crossClear`: a cross rib's triangles in, whether they stand clear.
pub type CrossClear<'a> = &'a dyn Fn(&[V4]) -> bool;

/// The options `buildSwayRib` and its callers take (`opts`, web/sway.js:115-132 `settings` and
/// the keys the rest reads). `None` / `false` is the key absent. `cross_clear` is the closure
/// `buildSwayBraces` / `swayAtFace` set for each call.
#[derive(Clone, Default)]
pub struct SwayOpts<'a> {
    /// `tines` (`!== false` keeps them).
    pub tines: Option<bool>,
    pub layer_height: Option<f64>,
    pub gap: Option<f64>,
    pub tine_contact_offset: Option<f64>,
    pub grip_from: Option<f64>,
    pub tine_spacing: Option<f64>,
    pub reach: Option<f64>,
    /// `uncappedDepth === true`.
    pub uncapped_depth: bool,
    /// `roundFeet === true`.
    pub round_feet: bool,
    /// `allowStilt` (`=== true` allows it); `None` is the key absent (swayAtFace then sets it).
    pub allow_stilt: Option<bool>,
    /// `maxHeight`; only a finite one counts.
    pub max_height: Option<f64>,
    /// `iface === 'all'`.
    pub iface_all: bool,
    /// `crossAbove`; only a finite one counts.
    pub cross_above: Option<f64>,
    pub cross_clear: Option<CrossClear<'a>>,
    pub cross_rise: Option<f64>,
    /// `avoid`: braces already standing and the walls to stay clear of (buildSwayBraces reads only
    /// the walls).
    pub avoid: Avoid<'a>,
    /// `full`: swayAtFace builds up the whole face.
    pub full: bool,
}

/// What a new brace has to avoid: JS accepts a plain array of braces, or `{ braces, walls }`
/// (`avoidance`, web/sway.js:621-625).
#[derive(Clone, Copy, Default)]
pub struct Avoid<'a> {
    pub braces: &'a [SwayRib],
    /// polylines `[[x, y, z], ...]`, e.g. `built.fins[i].line`
    pub walls: &'a [Vec<V3>],
}

/// `buildSwayRib`'s `{ ok: true, ... }` result.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayRib {
    pub tris: Vec<V4>,
    pub tines: usize,
    pub height: f64,
    pub depth: f64,
    pub th: f64,
    pub stilt: f64,
    pub foot: [V3; 2],
    pub half_w: f64,
    pub levels: Vec<SwayLevel>,
    pub cross: usize,
    pub arms: Vec<SwayArm>,
    /// `profiled: true` (set only when the rib follows a stepped face).
    pub profiled: bool,
    /// `triRange`: its vertex range into `buildSwayBraces`'s `triangles` (set there).
    pub tri_range: Option<[usize; 2]>,
}

/// The `settings(opts)` result (web/sway.js:115-132).
struct Settings {
    tines: bool,
    layer_h: f64,
    gap: f64,
    tine_contact_offset: f64,
    grip_from: f64,
    spacing: f64,
    reach: f64,
    uncapped_depth: bool,
    round_feet: bool,
    allow_stilt: bool,
}

/// Port of `leanCut` (web/sway.js:107).
fn lean_cut(cfg: &Config) -> f64 {
    jsnum::sin((cfg.sway.max_lean_deg * std::f64::consts::PI) / 180.0)
}

/// Port of `backCut` (web/sway.js:110): the same, for a face whose normal points up (+z); the
/// slack is planes.js's, so a face it keeps at exactly 45 degrees isn't refused here by round-off.
fn back_cut(cfg: &Config) -> f64 {
    jsnum::sin((cfg.sway.max_back_lean_deg * std::f64::consts::PI) / 180.0) + 1e-6
}

/// Port of `takesBrace` (web/sway.js:112): can a face with this normal z (in print space) take a
/// brace placed by hand?
fn takes_brace(nz: f64, cfg: &Config) -> bool {
    nz >= -lean_cut(cfg) && nz <= back_cut(cfg)
}

/// Port of `settings` (web/sway.js:115-132): resolve the caller's options against the defaults,
/// clamped to what prints.
fn settings(opts: &SwayOpts, cfg: &Config) -> Settings {
    let num = |v: Option<f64>, d: f64| match v {
        Some(x) if x.is_finite() => x,
        _ => d,
    };
    Settings {
        tines: opts.tines != Some(false),
        layer_h: jsnum::max(0.04, num(opts.layer_height, 0.2)),
        gap: num(opts.gap, cfg.sway.gap),
        tine_contact_offset: jsnum::max(
            -0.2,
            jsnum::min(0.2, num(opts.tine_contact_offset, -0.01)),
        ),
        grip_from: jsnum::max(0.0, num(opts.grip_from, 0.0)),
        spacing: jsnum::max(1.0, num(opts.tine_spacing, cfg.sway.tine_spacing)),
        reach: jsnum::max(0.05, jsnum::min(0.5, num(opts.reach, cfg.sway.reach))),
        uncapped_depth: opts.uncapped_depth,
        round_feet: opts.round_feet && cfg.prop.tine_dir != TineDir::Vertical,
        // Auto refuses a brace that would stand a long way up before its first tine;
        // a brace placed BY HAND is built anyway and reports the stilt instead. The
        // tool suggests, the person decides -- the same split as the rest of the app.
        allow_stilt: opts.allow_stilt == Some(true),
    }
}

/// Port of `printTriangles` (web/sway.js:135-145): the part's triangles in print space, as a flat
/// array.
fn print_triangles(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Vec<f64> {
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

/// Port of `pushSolid` (web/sway.js:163-172): push a closed solid, flipping the winding outward
/// if its signed volume is negative.
fn push_solid(local: &[V4], out: &mut Vec<V4>) {
    let mut v = 0.0;
    let mut i = 0;
    while i < local.len() {
        let (a, b, c) = (local[i], local[i + 1], local[i + 2]);
        v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        i += 3;
    }
    if v < 0.0 {
        let mut i = 0;
        while i < local.len() {
            out.extend([local[i], local[i + 2], local[i + 1]]);
            i += 3;
        }
    } else {
        out.extend(local.iter().copied());
    }
}

/// Port of `prism` (web/sway.js:175-186): a convex polygon `poly` ([[a, b], ...]) extruded from
/// c = lo to c = hi, mapped by P(a, b, c).
fn prism(poly: &[[f64; 2]], lo: f64, hi: f64, p: impl Fn(f64, f64, f64) -> V3, out: &mut Vec<V4>) {
    let n = poly.len();
    let l: Vec<V4> = poly.iter().map(|&[a, b]| p(a, b, lo).v4()).collect();
    let h: Vec<V4> = poly.iter().map(|&[a, b]| p(a, b, hi).v4()).collect();
    let mut local = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        local.extend([l[i], l[j], h[j], l[i], h[j], h[i]]);
    }
    for i in 1..n.saturating_sub(1) {
        local.extend([h[0], h[i], h[i + 1], l[0], l[i + 1], l[i]]);
    }
    push_solid(&local, out);
}

/// The rib's frame on a patch (`frameOf`, web/sway.js:193-198, plus the `uDir` that
/// `buildSwayRib` adds): `s` is horizontal distance out of the face (along the face normal's
/// horizontal part), `uu` is across the face (the patch's own u, so patchProbe reads it
/// directly), z is z. A world point is s*nh + uu*u.
pub struct Frame {
    pub nh: (f64, f64),
    pub u: (f64, f64),
}

impl Frame {
    fn of(p: &Patch) -> Frame {
        Frame {
            nh: (p.n.x / p.h, p.n.y / p.h),
            u: (p.u.x, p.u.y),
        }
    }
    fn to_world(&self, s: f64, uu: f64, z: f64) -> V3 {
        [
            self.nh.0 * s + self.u.0 * uu,
            self.nh.1 * s + self.u.1 * uu,
            z,
        ]
    }
    fn s_of(&self, q: V3) -> f64 {
        q[0] * self.nh.0 + q[1] * self.nh.1
    }
}

/// The mutable shape state of `buildSwayRib` (`th`, `H`, `wIn`, `sIn0`, `sInSlope`, `D0`,
/// `insets`, `heights` and the closures over them: `thFor`, `shiftIn`, `shape`, `sIn`, `depthAt`,
/// `sOut`).
struct Shape<'a> {
    p: &'a Patch,
    fr: &'a Frame,
    uc: f64,
    cfg: &'a Config,
    s: &'a Settings,
    max_height: Option<f64>,
    uncapped: bool,
    h: f64,
    th: f64,
    w_in: f64,
    s_in0: f64,
    s_in_slope: f64,
    d0: f64,
    insets: Vec<f64>,
    heights: Vec<f64>,
}

impl Shape<'_> {
    fn th_for(&self, h: f64) -> f64 {
        let sw = &self.cfg.sway;
        jsnum::min(sw.th_max, sw.th_min + sw.th_per_mm * h)
    }

    /// `shiftIn(half, zLo = 0, zHi = H)`: seat the rib's inner edge on the outermost point of the
    /// face INSIDE its own slab, as the old fins.js leaning fin did per window: the patch plane
    /// touches the patch's global high point, which may be elsewhere on the face, and the gap has
    /// to be a floor here, not somewhere else on the same side.
    fn shift_in(&self, half: f64, z_lo: f64, z_hi: f64) -> f64 {
        let p = self.p;
        let uc = self.uc;
        let mut m = f64::NEG_INFINITY;
        let mut i = 0;
        while i < p.tris.len() {
            let t = &p.tris;
            let mut poly: Vec<V3> = vec![
                [t[i], t[i + 1], t[i + 2]],
                [t[i + 3], t[i + 4], t[i + 5]],
                [t[i + 6], t[i + 7], t[i + 8]],
            ];
            i += 9;
            poly = clip(&poly, &|q: &V3| q[0] - (uc - half));
            poly = clip(&poly, &|q: &V3| (uc + half) - q[0]);
            if self.max_height.is_some_and(|h| h.is_finite()) {
                poly = clip(&poly, &|q: &V3| z_at(p, q[2], q[1]) - z_lo);
                poly = clip(&poly, &|q: &V3| z_hi - z_at(p, q[2], q[1]));
            }
            for q in &poly {
                if q[2] > m {
                    m = q[2];
                }
            }
        }
        if m == f64::NEG_INFINITY { 0.0 } else { m }
    }

    /// `shape()`: the outline in (s, z). The inner edge follows the (possibly leaning) face plane
    /// at the gap; the outer edge tapers from `D0` at the bed to topDepth.
    fn shape(&mut self) {
        let sw = &self.cfg.sway;
        let p = self.p;
        self.th = self.th_for(self.h);
        let half = self.th / 2.0 + sw.tine_w;
        self.w_in = self.shift_in(half, 0.0, self.h) + self.s.gap;
        let a = self.fr.s_of(patch_point(
            p,
            self.w_in,
            self.uc,
            t_at_z(p, self.w_in, 0.0),
        ));
        let b = self.fr.s_of(patch_point(
            p,
            self.w_in,
            self.uc,
            t_at_z(p, self.w_in, 1.0),
        ));
        self.s_in0 = a;
        self.s_in_slope = b - a;
        self.d0 = jsnum::max(
            sw.min_depth,
            if self.uncapped {
                self.s.reach * self.h
            } else {
                jsnum::min(sw.max_depth, self.s.reach * self.h)
            },
        );
        self.insets = Vec::new();
        self.heights = vec![0.0, self.h];
        if self.max_height.is_some_and(|h| h.is_finite()) {
            // a top on a band edge (16 mm) gives 2 bands, not 3 from a round-trip's 1e-15
            let n = jsnum::ceil(self.h / 8.0 - 1e-6);
            let ni = n as usize;
            self.heights = (0..=ni).map(|i| (self.h * i as f64) / n).collect();
            let bands: Vec<f64> = (0..ni)
                .map(|i| self.shift_in(half, self.heights[i], self.heights[i + 1]) + self.s.gap)
                .collect();
            // Both ends of each segment stay outside that entire band's surface.
            self.insets = (0..=ni)
                .map(|i| {
                    (jsnum::max(
                        bands[jsnum::max(0.0, i as f64 - 1.0) as usize],
                        bands[jsnum::min((ni - 1) as f64, i as f64) as usize],
                    ) - self.w_in)
                        / p.h
                })
                .collect();
        }
    }

    fn s_in(&self, z: f64) -> f64 {
        let base = self.s_in0 + self.s_in_slope * z;
        if self.insets.is_empty() {
            return base;
        }
        let t = jsnum::max(0.0, jsnum::min(1.0, z / self.h)) * (self.insets.len() - 1) as f64;
        let i = jsnum::min((self.insets.len() - 2) as f64, jsnum::floor(t)) as usize;
        base + self.insets[i] + (self.insets[i + 1] - self.insets[i]) * (t - i as f64)
    }

    fn depth_at(&self, z: f64) -> f64 {
        self.d0 + (self.cfg.sway.top_depth - self.d0) * jsnum::min(1.0, jsnum::max(0.0, z / self.h))
    }

    fn s_out(&self, z: f64) -> f64 {
        self.s_in0 + self.s_in_slope * z + self.depth_at(z)
    }
}

/// Port of `buildSwayRib` (web/sway.js:238-482): build one sway brace against patch `p`, centred
/// across the face at `uc`. `Err` is JS's `{ ok: false, reason }`.
pub fn build_sway_rib(
    p: &Patch,
    uc: f64,
    part_tris: &Tris,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    opts: &SwayOpts,
    cfg: &Config,
) -> Result<SwayRib, String> {
    let sw = &cfg.sway;
    let s = settings(opts, cfg);
    if !takes_brace(p.n.z, cfg) {
        return Err(
            "that face leans too far to stand a brace against — pick an upright side".to_string(),
        );
    }
    let fr = Frame::of(p);
    let max_height = opts.max_height;

    // How much of the face this column actually crosses, 1mm at a time.
    let (mut fz0, mut fz1) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut z = jsnum::max(0.0, p.z0);
    while z <= p.z1 + 1e-6 {
        if patch_probe(p, uc, t_at_z(p, 0.0, z)).is_some() {
            if z < fz0 {
                fz0 = z;
            }
            if z > fz1 {
                fz1 = z;
            }
        }
        z += 1.0;
    }
    if fz1 == f64::NEG_INFINITY {
        return Err("there is no face under that spot to brace".to_string());
    }

    let mut h = fz1 - sw.top_clear;
    if h < sw.min_rib_h {
        return Err(format!(
            "that face only reaches {}mm up — too short to need a brace",
            jsnum::to_fixed(fz1, 0)
        ));
    }
    if let Some(mh) = max_height.filter(|x| x.is_finite()) {
        h = jsnum::min(h, mh);
        if h < sw.min_rib_h {
            return Err("that’s too low for a brace — move up the side".to_string());
        }
    }
    let mut st = Shape {
        p,
        fr: &fr,
        uc,
        cfg,
        s: &s,
        max_height,
        uncapped: s.uncapped_depth,
        h,
        th: 0.0,
        w_in: 0.0,
        s_in0: 0.0,
        s_in_slope: 0.0,
        d0: 0.0,
        insets: Vec::new(),
        heights: vec![0.0, h],
    };
    st.th = st.th_for(st.h);

    // Shrink the rib until nothing on the part crosses it: a feature sticking out
    // above the face (a ledge, a shoulder) caps it below that feature.
    let mut hit = f64::INFINITY;
    for _iter in 0..6 {
        st.shape();
        hit = f64::INFINITY;
        for i in 0..st.heights.len() - 1 {
            let lo = st.heights[i];
            let hi = st.heights[i + 1];
            let a = st.s_in(lo);
            let slope = (st.s_in(hi) - a) / (hi - lo);
            let last = i == st.heights.len() - 2;
            let first = i == 0;
            let h_top = st.h;
            let stp = &st;
            let o0 = move |q: &V3| q[2] - if first { -0.1 } else { lo };
            let o1 = move |q: &V3| (if last { h_top + 0.3 } else { hi }) - q[2];
            let o2 = move |q: &V3| q[0] - (a + slope * (q[2] - lo) - 0.05);
            let o3 = move |q: &V3| (stp.s_out(q[2]) + 0.1) - q[0];
            // s bounds of this band's outline: its inner edge is a straight line, its outer edge
            // piecewise linear between the band heights, so the ends and knots are enough.
            let z_a = if first { -0.1 } else { lo };
            let z_b = if last { h_top + 0.3 } else { hi };
            let inner = |z: f64| a + slope * (z - lo) - 0.05;
            let mut s_hi = jsnum::max(stp.s_out(z_a), stp.s_out(z_b));
            for &z in &stp.heights {
                if z > z_a && z < z_b {
                    s_hi = jsnum::max(s_hi, stp.s_out(z));
                }
            }
            hit = jsnum::min(
                hit,
                lowest_hit(
                    part_tris,
                    &fr,
                    uc - st.th / 2.0 - 0.3,
                    uc + st.th / 2.0 + 0.3,
                    &[&o0, &o1, &o2, &o3],
                    jsnum::min(inner(z_a), inner(z_b)),
                    s_hi + 0.1,
                    z_a,
                    z_b,
                ),
            );
        }
        if hit == f64::INFINITY {
            break;
        }
        st.h = hit - 1.0;
        if st.h < sw.min_rib_h {
            break;
        }
    }
    if hit != f64::INFINITY {
        return Err("the part sticks out over that spot, so a brace standing on the plate can’t reach up the face".to_string());
    }
    let h = st.h;
    let th = st.th;

    // The foot has its own, wider footprint on the plate.
    let s_foot_out = st.s_out(0.0) + sw.foot_pad;
    let foot_half_w = th / 2.0 + sw.foot_half;
    let s_in_bed = st.s_in(0.0);
    let f0 = |q: &V3| q[2] + 0.1;
    let f1 = |q: &V3| (sw.foot_h + 0.2) - q[2];
    let f2 = |q: &V3| q[0] - (s_in_bed - 0.05);
    let f3 = |q: &V3| (s_foot_out + 0.1) - q[0];
    let foot_hit = lowest_hit(
        part_tris,
        &fr,
        uc - foot_half_w - 0.2,
        uc + foot_half_w + 0.2,
        &[&f0, &f1, &f2, &f3],
        s_in_bed - 0.05,
        s_foot_out + 0.1,
        -0.1,
        sw.foot_h + 0.2,
    );
    if foot_hit != f64::INFINITY {
        return Err("the part’s base spreads out under this face, so there is no room on the plate for the brace’s foot".to_string());
    }

    let mut out: Vec<V4> = Vec::new();
    let pw = |s_: f64, z_: f64, uu: f64| fr.to_world(s_, uu, z_);
    if !st.insets.is_empty() {
        let mut local = Vec::new();
        let secs: Vec<Vec<V3>> = st
            .heights
            .iter()
            .map(|&z| {
                vec![
                    pw(st.s_in(z), z, uc - th / 2.0),
                    pw(st.s_out(z), z, uc - th / 2.0),
                    pw(st.s_out(z), z, uc + th / 2.0),
                    pw(st.s_in(z), z, uc + th / 2.0),
                ]
            })
            .collect();
        ribbon(&secs, &mut local);
        push_solid(&local, &mut out);
    } else {
        prism(
            &[
                [st.s_in(0.0), 0.0],
                [st.s_out(0.0), 0.0],
                [st.s_out(h), h],
                [st.s_in(h), h],
            ],
            uc - th / 2.0,
            uc + th / 2.0,
            pw,
            &mut out,
        );
    }
    let foot_poly = [
        [st.s_in(0.0), uc - foot_half_w],
        [s_foot_out, uc - foot_half_w],
        [s_foot_out, uc + foot_half_w],
        [st.s_in(0.0), uc + foot_half_w],
    ];
    let foot_poly: Vec<[f64; 2]> = if s.round_feet {
        rounded_polygon(&foot_poly, Radius::All(foot_half_w), 8.0)
    } else {
        foot_poly.to_vec()
    };
    prism(
        &foot_poly,
        0.0,
        sw.foot_h,
        |s_, uu, z| fr.to_world(s_, uu, z),
        &mut out,
    );

    // Tines: evenly spaced up the face, each snapped into exactly one layer cell.
    let (mut tines, mut first_grip, mut stilt) = (0usize, f64::INFINITY, 0.0);
    if s.tines {
        let z_start = jsnum::max(jsnum::max(fz0 + 0.5, s.grip_from), sw.foot_h + 0.5);
        let z_end = jsnum::min(fz1, h) - 0.5;
        let mut z = z_start;
        while z <= z_end + 1e-6 {
            let z_here = z;
            z += s.spacing;
            let bot = jsnum::round(z_here / s.layer_h) * s.layer_h;
            let top = bot + s.layer_h;
            if top > h {
                break;
            }
            let z_mid = bot + s.layer_h / 2.0;
            let Some(mut dev) = patch_probe(p, uc, t_at_z(p, 0.0, z_mid)) else {
                continue;
            };
            if let Some(refined) = patch_probe(p, uc, t_at_z(p, dev, z_mid)) {
                dev = refined;
            }
            let s_part = fr.s_of(patch_point(p, dev, uc, t_at_z(p, dev, z_mid)));
            let s_wall = if !st.insets.is_empty() {
                let mut xs = vec![st.s_in(bot), st.s_in(top)];
                xs.extend(
                    st.heights
                        .iter()
                        .filter(|&&hh| hh > bot && hh < top)
                        .map(|&hh| st.s_in(hh)),
                );
                jsnum::max_of(&xs)
            } else {
                st.s_in(z_mid)
            };
            if s_wall - s_part > sw.tine_span_max {
                continue; // a bridge, not a tine
            }
            let bx = fr.to_world(s_part - sw.tine_reach / 2.0, uc, z_mid);
            if !inside_part(topo, rot, offset, bx[0], bx[1], bx[2]) {
                continue; // grips nothing
            }
            // the tine runs from inside the rib to where it meets the part, its end
            // following the face between bottom and top at the contact offset (kissEnds).
            // Frame: along = into the part (-s) from the face point, across = z x along.
            let o = fr.to_world(s_part, uc, 0.0);
            let (ax, ay) = (-fr.nh.0, -fr.nh.1);
            let half = sw.tine_w / 2.0;
            // measured from the face point, not the tine's back: `back` only bounds how far
            // out of the face the end may pull back (0.25 mm), well short of the rib (ov)
            let e = kiss_ends(
                topo,
                rot,
                offset,
                o[0],
                o[1],
                ax,
                ay,
                bot,
                top,
                &KissOpts {
                    half,
                    back: sw.tine_overlap,
                    reach: sw.tine_reach,
                    contact_offset: s.tine_contact_offset,
                },
            );
            let ov = -(s_wall + sw.tine_overlap - s_part);
            let pt = |a: f64, b: f64, z: f64| [o[0] + ax * a - ay * b, o[1] + ay * a + ax * b, z];
            let outline =
                |end: [f64; 2]| [[ov, -half], [end[0], -half], [end[1], half], [ov, half]];
            let mut local = Vec::new();
            loft_extrude(&outline(e.bot), &outline(e.top), bot, top, pt, &mut local);
            // with Interface material on everywhere, a tine prints in the interface
            // filament, tagged the way prop/crest.js tags the walls' (a 4th vertex
            // component, v[3] = 1); not under 'flat', which leaves sideways grip alone
            if opts.iface_all {
                for v in &mut local {
                    v[3] = 1.0;
                }
            }
            push_solid(&local, &mut out);
            tines += 1;
            if bot < first_grip {
                first_grip = bot;
            }
        }
        // A tall rib tied on at a handful of points still lets the part wave about
        // between them, so the grip has to cover a real share of the height too.
        let wanted = jsnum::max(
            sw.min_tines,
            jsnum::floor((sw.min_grip_share * (z_end - z_start)) / s.spacing),
        );
        if (tines as f64) < wanted {
            return Err("too little of this face lines up with the brace for its tines to grip — try a flatter part of the side".to_string());
        }
        // Everything below the lowest tine is a lone wall holding nothing, and held by
        // nothing. Past this much of it the brace is its own liability, so refuse rather
        // than print a stilt: "Brace grip from" raises this deliberately, and a face that
        // starts high off the plate (a part tilted onto a corner) reaches it by itself.
        // Measured from the plate, or from "Brace grip from" when that is higher: a user
        // who asks to grip only above 80mm has chosen that stilt, and this is not the
        // place to overrule them. What it catches is the stilt the GEOMETRY imposes.
        stilt = jsnum::max(0.0, first_grip - jsnum::max(sw.foot_h, s.grip_from));
        let max_stilt = jsnum::min(sw.stilt_max, sw.stilt_max_frac * h);
        if !s.allow_stilt && stilt > max_stilt {
            return Err(format!(
                "this side only starts {}mm up, so the brace would stand {}mm holding nothing before it grips (max {}mm) — rotate so this side reaches the plate",
                jsnum::to_fixed(first_grip, 0),
                jsnum::to_fixed(stilt, 0),
                jsnum::to_fixed(max_stilt, 0)
            ));
        }
    }

    let foot = [
        fr.to_world(st.s_in(0.0), uc, 0.0),
        fr.to_world(s_foot_out, uc, 0.0),
    ];
    // The rib's footprint at a ladder of heights, so a clash check can compare two
    // ribs at the SAME z: two ribs on facing walls of a channel reach toward each
    // other, and whether they meet depends on both depths at that height.
    let mut level_heights: Vec<f64> = Vec::new();
    let add = |v: f64, set: &mut Vec<f64>| {
        // a Set: SameValueZero, so +0 and -0 are one value
        if !set.contains(&v) && !(v.is_nan() && set.iter().any(|x| x.is_nan())) {
            set.push(v);
        }
    };
    if !st.insets.is_empty() {
        for &hh in &st.heights {
            add(hh, &mut level_heights);
        }
    }
    let mut z = 0.0;
    loop {
        add(z, &mut level_heights);
        if z >= h {
            break;
        }
        z = jsnum::min(h, z + sw.level_step);
    }
    level_heights.sort_by(|a, b| num_ord(a - b));
    let levels: Vec<SwayLevel> = level_heights
        .iter()
        .map(|&z| SwayLevel {
            z,
            a: fr.to_world(st.s_in(z), uc, z),
            b: fr.to_world(st.s_out(z), uc, z),
        })
        .collect();
    // CROSS RIBS ("Brace tall walls"): over the start height the brace is a tall,
    // thin plate again -- deep toward the face, barely a bead across it -- so it
    // gets the plate ribs the prop walls get, one each way ALONG the side
    // (prop/brace.js). They stand on the brace's outer edge, clear of the part by
    // the brace's whole depth, and taper out to the plate.
    let mut cross = 0usize;
    let mut arms: Vec<SwayArm> = Vec::new();
    if let Some(ca) = opts.cross_above.filter(|x| x.is_finite())
        && h > ca
    {
        let inset = th / 2.0 + 0.1;
        let o = fr.to_world(st.s_out(0.0) - inset, uc, 0.0);
        let slope = st.s_in_slope + (sw.top_depth - st.d0) / h;
        let lean = [fr.nh.0 * slope, fr.nh.1 * slope];
        // ...sheared to follow the outer edge, which leans by the face's lean AND the
        // taper: a thin plate past 45 degrees sags. An anchor on a steep back (#351) or a
        // deep brace gets none rather than one that overhangs; skipped, not clamped, since
        // a rib leaning less than the edge would part from it toward the top.
        let cross_ok = jsnum::abs(slope) <= 1.0 + 1e-6;
        let clear_arm = |t: &[V4]| opts.cross_clear.is_none_or(|f| f(t));
        // Only the part near the foot matters to the arms; the square covers the longest reach
        // plus the clearances and the flange width (see swaycrop.rs).
        let cropped;
        let near: &Tris = if cross_ok {
            cropped = crop_for_rib(
                &part_tris.v,
                [o[0], o[1]],
                jsnum::max(cfg.brace.min_reach, st.d0)
                    + 2.0 * cfg.prop.side_clear
                    + cfg.prop.foot_min
                    + th
                    + 1.0,
                lean,
                0.0,
                h + cfg.prop.side_clear,
            );
            &cropped
        } else {
            part_tris
        };
        let sgns: &[f64] = if cross_ok { &[1.0, -1.0] } else { &[] };
        for &sgn in sgns {
            let d = [p.u.x * sgn, p.u.y * sgn];
            let start = out.len();
            let Some(c) = brace_cross_rib(
                &mut out,
                near,
                [o[0], o[1]],
                d,
                0.0,
                h,
                st.d0,
                lean,
                clear_arm,
                opts.cross_rise.unwrap_or(f64::NAN),
                th,
                cfg,
            ) else {
                continue;
            };
            cross += 1;
            arms.push(SwayArm {
                a: o,
                b: [o[0] + d[0] * c.reach, o[1] + d[1] * c.reach, 0.0],
                th: c.th,
                plan: plan_hull_v4(&out[start..]),
            });
        }
    }
    // `stilt`: how far it stands holding nothing before its first tine. Auto keeps this
    // small by refusing; a hand-placed brace reports it so the readout can say so.
    Ok(SwayRib {
        tris: out,
        tines,
        height: h,
        depth: st.d0,
        th,
        stilt,
        foot,
        half_w: foot_half_w,
        levels,
        cross,
        arms,
        profiled: !st.insets.is_empty(),
        tri_range: None,
    })
}

/// Port of `segDist` (web/sway.js:485-496): closest distance between two 2D segments.
fn seg_dist(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    let pt = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        let dx = r[0] - q[0];
        let dy = r[1] - q[1];
        let l = dx * dx + dy * dy;
        let t = if l > 0.0 {
            jsnum::max(
                0.0,
                jsnum::min(1.0, ((p[0] - q[0]) * dx + (p[1] - q[1]) * dy) / l),
            )
        } else {
            0.0
        };
        jsnum::hypot(&[p[0] - q[0] - dx * t, p[1] - q[1] - dy * t])
    };
    let cross = |o: [f64; 2], p: [f64; 2], q: [f64; 2]| {
        (p[0] - o[0]) * (q[1] - o[1]) - (p[1] - o[1]) * (q[0] - o[0])
    };
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) {
        return 0.0;
    }
    jsnum::min(
        jsnum::min(pt(a, c, d), pt(b, c, d)),
        jsnum::min(pt(c, a, b), pt(d, a, b)),
    )
}

/// Port of `planHull` (web/sway.js:498-513): the convex hull of a point set in plan (Andrew's
/// monotone chain).
fn plan_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut sorted: Vec<[f64; 2]> = points.iter().map(|p| [p[0], p[1]]).collect();
    sorted.sort_by(|a, b| num_ord(or(a[0] - b[0], a[1] - b[1])));
    let pts: Vec<[f64; 2]> = sorted
        .iter()
        .enumerate()
        .filter(|&(i, p)| i == 0 || p[0] != sorted[i - 1][0] || p[1] != sorted[i - 1][1])
        .map(|(_, p)| *p)
        .collect();
    if pts.len() < 3 {
        return pts;
    }
    let half = |list: &[[f64; 2]]| {
        let mut out: Vec<[f64; 2]> = Vec::new();
        for &p in list {
            while out.len() > 1 && cross(out[out.len() - 2], out[out.len() - 1], p) <= 0.0 {
                out.pop();
            }
            out.push(p);
        }
        out.pop();
        out
    };
    let mut rev = pts.clone();
    rev.reverse();
    let mut hull = half(&pts);
    hull.extend(half(&rev));
    hull
}

fn plan_hull_v4(points: &[V4]) -> Vec<[f64; 2]> {
    let xy: Vec<[f64; 2]> = points.iter().map(|p| [p[0], p[1]]).collect();
    plan_hull(&xy)
}

/// Port of `planDist` (web/sway.js:515-526): the distance between two plan polygons (0 when one
/// holds the other's first point). A "polygon" of one or two points is a point or a segment.
fn plan_dist(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    let inside = |p: [f64; 2], poly: &[[f64; 2]]| {
        poly.len() > 2
            && poly.iter().enumerate().all(|(i, v)| {
                let w = poly[(i + 1) % poly.len()];
                (w[0] - v[0]) * (p[1] - v[1]) - (w[1] - v[1]) * (p[0] - v[0]) >= -1e-9
            })
    };
    if inside(a[0], b) || inside(b[0], a) {
        return 0.0;
    }
    let mut dist = f64::INFINITY;
    for i in 0..a.len() {
        for j in 0..b.len() {
            dist = jsnum::min(
                dist,
                seg_dist(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()]),
            );
        }
    }
    dist
}

/// Port of `stripPlan` (web/sway.js:529-534): the rectangular footprint of a rib section, without
/// rounded end caps.
fn strip_plan(a: V3, b: V3, half: f64) -> Vec<[f64; 2]> {
    let len = jsnum::hypot(&[b[0] - a[0], b[1] - a[1]]);
    let dx = -(b[1] - a[1]) / len;
    let dy = (b[0] - a[0]) / len;
    let mut pts = Vec::new();
    for p in [a, b] {
        pts.push([p[0] + dx * half, p[1] + dy * half]);
        pts.push([p[0] - dx * half, p[1] - dy * half]);
    }
    plan_hull(&pts)
}

/// Port of `bodyPlan` (web/sway.js:536-539).
fn body_plan(rib: &SwayRib) -> Vec<[f64; 2]> {
    let mut pts = strip_plan(rib.foot[0], rib.foot[1], rib.half_w);
    for l in &rib.levels {
        pts.extend(strip_plan(l.a, l.b, rib.th / 2.0));
    }
    plan_hull(&pts)
}

/// Port of `swayClashes` (web/sway.js:548-572): would `rib` run into any of `ribs`? Two tests: the
/// FEET on the plate (the widest part, flange included), and the RIBS themselves compared level by
/// level up to the shorter one's top -- ribs on opposite walls of a channel point at each other
/// and would fuse into one bar across it, which neither snaps off nor breaks away. Needs
/// SWAY.clearance of air between them, so the slicer keeps them apart.
pub fn sway_clashes(rib: &SwayRib, ribs: &[SwayRib], cfg: &Config) -> bool {
    let clearance = cfg.sway.clearance;
    for r in ribs {
        // Segment distance plus half-width treats each end as a semicircle. The
        // actual feet end square: opposite braces can be clear across a thin part.
        if plan_dist(
            &strip_plan(rib.foot[0], rib.foot[1], rib.half_w),
            &strip_plan(r.foot[0], r.foot[1], r.half_w),
        ) < clearance
        {
            return true;
        }
        let top = jsnum::min(rib.height, r.height);
        let mut zs: Vec<f64> = Vec::new();
        for l in rib
            .levels
            .iter()
            .chain(r.levels.iter())
            .filter(|l| l.z <= top)
        {
            if !zs.contains(&l.z) {
                zs.push(l.z);
            }
        }
        if !zs.contains(&top) {
            zs.push(top);
        }
        for z in zs {
            let l = level_at(&rib.levels, z);
            let m = level_at(&r.levels, z);
            if plan_dist(
                &strip_plan(l.0, l.1, rib.th / 2.0),
                &strip_plan(m.0, m.1, r.th / 2.0),
            ) < clearance
            {
                return true;
            }
        }
        for arm in &rib.arms {
            if plan_dist(&arm.plan, &body_plan(r)) < clearance {
                return true;
            }
            for other in &r.arms {
                if plan_dist(&arm.plan, &other.plan) < clearance {
                    return true;
                }
            }
        }
        for arm in &r.arms {
            if plan_dist(&body_plan(rib), &arm.plan) < clearance {
                return true;
            }
        }
    }
    false
}

/// Port of `levelAt` (web/sway.js:575-582): a rib's footprint at height z, interpolated between
/// its sampled levels.
fn level_at(levels: &[SwayLevel], z: f64) -> (V3, V3) {
    let mut k = 0;
    while k < levels.len() - 2 && levels[k + 1].z < z {
        k += 1;
    }
    let a = &levels[k];
    let b = &levels[(k + 1).min(levels.len() - 1)];
    let f = if b.z > a.z {
        jsnum::max(0.0, jsnum::min(1.0, (z - a.z) / (b.z - a.z)))
    } else {
        0.0
    };
    let mix = |p: V3, q: V3| [p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f, 0.0];
    (mix(a.a, b.a), mix(a.b, b.b))
}

/// Port of `swayClashesWall` (web/sway.js:597-615): would `rib` run into a support that is
/// already there -- a prop wall or a wedge?
///
/// Braces keep clear of the part and of each other, but the props and wedges Auto places are
/// neither: a rib standing beside a prop under a ledge on the same side merges with it into one
/// piece that no longer snaps off in two halves. Their records carry a centreline (`line`, world
/// points), so this compares the rib's FOOT against that line in XY -- the bed is where both are
/// widest, and a support that clears there is clear the whole way up, since both taper inward
/// with height. Profiled ribs also check their full body projection: the inner edge can bend.
pub fn sway_clashes_wall(rib: &SwayRib, walls: &[Vec<V3>], cfg: &Config) -> bool {
    if walls.is_empty() {
        return false;
    }
    let sw = &cfg.sway;
    let need = rib.half_w + sw.wall_half + sw.clearance;
    let plan = if rib.profiled {
        Some(body_plan(rib))
    } else {
        None
    };
    let xy = |p: V3| [p[0], p[1]];
    let f0 = xy(rib.foot[0]);
    let f1 = xy(rib.foot[1]);
    for line in walls {
        if line.is_empty() {
            continue;
        }
        if line.len() == 1 {
            let p = xy(line[0]);
            if let Some(plan) = &plan
                && plan_dist(plan, &[p]) < sw.wall_half + sw.clearance
            {
                return true;
            }
            if seg_dist(f0, f1, p, p) < need {
                return true;
            }
            continue;
        }
        for i in 1..line.len() {
            if let Some(plan) = &plan
                && plan_dist(plan, &[xy(line[i - 1]), xy(line[i])]) < sw.wall_half + sw.clearance
            {
                return true;
            }
            if seg_dist(f0, f1, xy(line[i - 1]), xy(line[i])) < need {
                return true;
            }
        }
    }
    false
}

/// Port of `armClear` (web/sway.js:632-651, the `crossClear` for braceCrossRib): a cross
/// rib's plan footprint (its own triangles, flange included) has to stay SWAY.clearance off the
/// braces and prop walls already standing. Convex projections include the leaning stem and flange.
fn arm_clear<'a>(
    braces: &'a [SwayRib],
    walls: &'a [Vec<V3>],
    cfg: &'a Config,
) -> impl Fn(&[V4]) -> bool + 'a {
    move |tris: &[V4]| {
        let sw = &cfg.sway;
        let plan = plan_hull_v4(tris);
        for r in braces {
            if plan_dist(&plan, &body_plan(r)) < sw.clearance {
                return false;
            }
            for arm in &r.arms {
                if plan_dist(&plan, &arm.plan) < sw.clearance {
                    return false;
                }
            }
        }
        for line in walls {
            if line.is_empty() {
                continue;
            }
            let pts: Vec<[f64; 2]> = if line.len() == 1 {
                vec![[line[0][0], line[0][1]], [line[0][0], line[0][1]]]
            } else {
                line.iter().map(|p| [p[0], p[1]]).collect()
            };
            for i in 1..pts.len() {
                if plan_dist(&plan, &[pts[i - 1], pts[i]]) < sw.wall_half + sw.clearance {
                    return false;
                }
            }
        }
        true
    }
}

/// Port of `columnTop` (web/sway.js:656-662): how high the face reaches at `u` -- the tallest rib
/// a column there could carry.
fn column_top(p: &Patch, u: f64) -> f64 {
    let mut top = f64::NEG_INFINITY;
    let mut z = jsnum::max(0.0, p.z0);
    while z <= p.z1 + 1e-6 {
        if patch_probe(p, u, t_at_z(p, 0.0, z)).is_some() {
            top = z;
        }
        z += 2.0;
    }
    top
}

/// Port of `columnsFor` (web/sway.js:672-694): where along a face to stand its ribs: at the
/// TALLEST columns, kept apart. A face's outline is rarely a rectangle -- a gable, a sloped top, a
/// notch -- and the sway lives at the top, so a rib at the face's low end braces the half of the
/// part that was never moving. Evenly spaced columns put the fence-cap sample's ribs 160mm up a
/// 249mm side; picking the tall columns reaches 234mm. Ties go to the ends (FIN-SPEC: edges hide
/// the tine marks).
fn columns_for(p: &Patch, cfg: &Config) -> Vec<f64> {
    let sw = &cfg.sway;
    let w = p.u1 - p.u0;
    let n = jsnum::max(1.0, jsnum::min(sw.max_per_face, jsnum::round(w / sw.pitch)));
    let inset = jsnum::min(sw.end_inset, w / 4.0);
    let a = p.u0 + inset;
    let b = p.u1 - inset;
    let steps = jsnum::max(1.0, jsnum::min(24.0, jsnum::round((b - a) / 5.0)));
    let mid = (a + b) / 2.0;
    struct Sample {
        u: f64,
        top: f64,
        edge: f64,
    }
    let mut samples = Vec::new();
    let mut k = 0.0;
    while k <= steps {
        let u = a + ((b - a) * k) / steps;
        samples.push(Sample {
            u,
            top: column_top(p, u),
            edge: jsnum::abs(u - mid),
        });
        k += 1.0;
    }
    samples.sort_by(|x, y| num_ord(or(y.top - x.top, y.edge - x.edge)));
    let apart = jsnum::max(sw.pitch / 2.0, w / (n + 1.0));
    let mut cols: Vec<f64> = Vec::new();
    for s in &samples {
        if cols.len() as f64 >= n {
            break;
        }
        if s.top == f64::NEG_INFINITY {
            break;
        }
        if cols.iter().any(|&c| jsnum::abs(c - s.u) < apart) {
            continue;
        }
        cols.push(s.u);
    }
    if cols.is_empty() { vec![mid] } else { cols }
}

/// What `buildSwayBraces` returns: `{ triangles, count, tines, skipped, ribs, reason }`.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayBraces {
    pub triangles: Vec<V4>,
    pub count: usize,
    pub tines: usize,
    pub skipped: usize,
    pub ribs: Vec<SwayRib>,
    /// `reason`: set when nothing was placed.
    pub reason: Option<String>,
}

/// Port of `buildSwayBraces` (web/sway.js:705-760): AUTO: brace the tallest upright sides of the
/// part, a few faces facing different ways so it is held in both directions (FIN-SPEC's "two
/// fins, opposite sides", extended to both axes of a tall part).
///
/// `reason` is set when nothing was placed, so the readout can say why. Each rib carries
/// `tri_range`, its vertex range into `triangles`, so the UI can remove one brace by itself.
pub fn build_sway_braces(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &SwayOpts,
    cfg: &Config,
) -> SwayBraces {
    let sw = &cfg.sway;
    // `ribs` is always an array, even when nothing was placed: callers map over it
    // to build their own records, and an undefined here threw on the first part too
    // short to brace.
    let none = |reason: String| SwayBraces {
        triangles: Vec::new(),
        count: 0,
        tines: 0,
        skipped: 0,
        ribs: Vec::new(),
        reason: Some(reason),
    };
    let part_tris = Tris::new(print_triangles(topo, rot, &result.offset));
    let mut part_top = 0.0;
    let mut i = 2;
    while i < part_tris.v.len() {
        if part_tris.v[i] > part_top {
            part_top = part_tris.v[i];
        }
        i += 3;
    }
    if part_top < sw.min_part_h {
        return none(format!(
            "the part is only {}mm tall, too short to sway",
            jsnum::to_fixed(part_top, 0)
        ));
    }

    let cut = lean_cut(cfg);
    struct Cand {
        p: Patch,
        score: f64,
        bearing: f64,
    }
    let mut cands: Vec<Cand> = find_wall_patches(topo, rot, &result.offset, None, None)
        .into_iter()
        .filter(|p| {
            jsnum::abs(p.n.z) <= cut
                && p.z1 - jsnum::max(0.0, p.z0) >= sw.min_face_h
                && p.z1 >= sw.min_top_frac * part_top
        })
        .map(|p| Cand {
            score: (p.z1 - jsnum::max(0.0, p.z0)) * (p.u1 - p.u0),
            bearing: jsnum::atan2(p.n.y, p.n.x),
            p,
        })
        .collect();
    cands.sort_by(|a, b| num_ord(b.score - a.score));
    if cands.is_empty() {
        return none("no upright side is tall and flat enough to brace in this pose".to_string());
    }

    let two_pi = 2.0 * std::f64::consts::PI;
    let sep = (sw.min_bearing_sep * std::f64::consts::PI) / 180.0;
    let ang_gap = |a: f64, b: f64| {
        let d = jsnum::abs(a - b) % two_pi;
        jsnum::min(d, two_pi - d)
    };
    let mut faces: Vec<&Cand> = Vec::new();
    for c in &cands {
        cfg.cancel.check();
        if faces.len() as f64 >= sw.max_faces {
            break;
        }
        if faces.iter().any(|f| ang_gap(f.bearing, c.bearing) < sep) {
            continue;
        }
        faces.push(c);
    }

    // Auto has usually placed props and wedges before this runs, and a rib that
    // merges with one of them is a support that no longer breaks away in pieces.
    let walls = opts.avoid.walls;
    let mut ribs: Vec<SwayRib> = Vec::new();
    let mut out: Vec<V4> = Vec::new();
    let (mut tines, mut skipped) = (0usize, 0usize);
    for c in &faces {
        cfg.cancel.check();
        let p = &c.p;
        for u in columns_for(p, cfg) {
            let mut placed: Option<SwayRib> = None;
            for &du in &sw.nudges {
                let uc = u + du;
                if uc < p.u0 || uc > p.u1 {
                    continue;
                }
                let clear = arm_clear(&ribs, walls, cfg);
                let mut o = opts.clone();
                o.cross_clear = Some(&clear);
                let r = build_sway_rib(p, uc, &part_tris, topo, rot, &result.offset, &o, cfg);
                if let Ok(r) = r
                    && !sway_clashes(&r, &ribs, cfg)
                    && !sway_clashes_wall(&r, walls, cfg)
                {
                    placed = Some(r);
                    break;
                }
            }
            let Some(mut placed) = placed else {
                skipped += 1;
                continue;
            };
            placed.tri_range = Some([out.len(), out.len() + placed.tris.len()]);
            out.extend(placed.tris.iter().copied());
            tines += placed.tines;
            ribs.push(placed);
        }
    }
    SwayBraces {
        triangles: out,
        count: ribs.len(),
        tines,
        skipped,
        reason: if ribs.is_empty() {
            Some(
                "the upright sides are blocked by other parts of the model in this pose"
                    .to_string(),
            )
        } else {
            None
        },
        ribs,
    }
}

/// Port of `patchesFor` (web/sway.js:764-775): patches per pose, so a click in Draw doesn't re-run
/// the face search. Cached on the topology (`patchCache`).
fn patches_for(topo: &Topology, rot: &Rot3, offset: &Xyz) -> Arc<PatchEntry> {
    let key = pose_key(rot, offset);
    let slot = topo.cache.sway_patches.get_or_init(SwayPatches::default);
    let mut entry = slot.entry.lock().unwrap();
    if let Some(hit) = entry.as_ref()
        && hit.key == key
    {
        return hit.clone();
    }
    let patches = find_wall_patches(topo, rot, offset, None, None);
    let mut by_face: IndexMap<usize, usize> = IndexMap::new();
    for (i, p) in patches.iter().enumerate() {
        for &f in &p.faces {
            by_face.entry(f).or_insert(i);
        }
    }
    let part_tris = Arc::new(Tris::new(print_triangles(topo, rot, offset)));
    let e = Arc::new(PatchEntry {
        key,
        patches,
        by_face,
        part_tris,
    });
    *entry = Some(e.clone());
    e
}

/// Port of `faceIsUpright` (web/sway.js:778-783): is this face (by index) upright enough, in this
/// pose, to take a sway brace?
pub fn face_is_upright(topo: &Topology, rot: &Rot3, face_index: usize, cfg: &Config) -> bool {
    let i = face_index * 3;
    let (x, y, z) = (topo.nrm[i], topo.nrm[i + 1], topo.nrm[i + 2]);
    let nz = rot[2] * x + rot[5] * y + rot[8] * z;
    takes_brace(nz, cfg)
}

/// Port of `swayAtFace` (web/sway.js:793-822): DRAW: a sway brace on the face the user clicked, at
/// the spot they clicked. `point` is in print space; its z caps the brace's height, unless
/// `opts.full` (the brace runs up the whole face, as it always did before the cap). `avoid` is the
/// braces already standing (each a buildSwayRib result) and the walls; the new one may shift a few
/// mm to clear them, and is refused -- with the reason at the spot clicked -- if it can't.
/// Returns buildSwayRib's result.
pub fn sway_at_face(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    face_index: usize,
    point: V3,
    opts: &SwayOpts,
    avoid: &Avoid,
    cfg: &Config,
) -> Result<SwayRib, String> {
    // A brace you clicked is a brace you meant, so the stilt limit Auto obeys is
    // advisory here: it builds and reports `stilt` for the readout to mention.
    let cap = match opts.max_height {
        Some(h) if h.is_finite() => h,
        _ => f64::INFINITY,
    };
    let mut opts = opts.clone();
    opts.allow_stilt = Some(opts.allow_stilt.unwrap_or(true));
    opts.max_height = Some(if opts.full {
        cap
    } else {
        jsnum::min(point[2], cap)
    });
    let entry = patches_for(topo, rot, &result.offset);
    let Some(&pi) = entry.by_face.get(&face_index) else {
        return Err("that face is too small or curved to stand a brace against".to_string());
    };
    let p = &entry.patches[pi];
    let u = point[0] * p.u.x + point[1] * p.u.y;
    let (braces, walls) = (avoid.braces, avoid.walls);
    let mut last: Option<Result<SwayRib, String>> = None;
    let (mut hit_brace, mut hit_wall) = (false, false);
    for du in [0.0, 2.0, -2.0, 4.0, -4.0] {
        let uc = jsnum::max(p.u0, jsnum::min(p.u1, u + du));
        let clear = arm_clear(braces, walls, cfg);
        let mut o = opts.clone();
        o.cross_clear = Some(&clear);
        let r = build_sway_rib(p, uc, &entry.part_tris, topo, rot, &result.offset, &o, cfg);
        if let Ok(rib) = &r {
            if sway_clashes(rib, braces, cfg) {
                hit_brace = true;
                continue;
            }
            if sway_clashes_wall(rib, walls, cfg) {
                hit_wall = true;
                continue;
            }
            return r;
        }
        // the clicked spot's own reason, not the last nudge's
        if last.is_none() {
            last = Some(r);
        }
    }
    if hit_brace {
        return Err("it would run into another brace (on the facing wall, or right beside it) — click a spot staggered from it".to_string());
    }
    if hit_wall {
        return Err("a support already stands there, and the two would fuse into one piece — click a spot clear of it".to_string());
    }
    last.unwrap_or_else(|| Err(String::new()))
}
