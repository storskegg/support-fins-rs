//! Port of `web/cutout.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! WALL CUTOUTS (issue #34) -- holes through a breakaway wall to save filament.
//!
//! prop/sweep.js sweeps each wall as per-station cross-sections and hands them here;
//! cutWall either emits the wall with holes or returns false, and the caller
//! emits it solid as before. Everything is in the wall's own (s, z) plane: s runs
//! along the wall, z up. The pattern choice lives here (CUT.pattern), set by the
//! page and carried to the Worker in opts.tunables (fins.js applyTunables).

use crate::config::Config;
use crate::jsnum;
use crate::solids::{box_extrude, ribbon};

// Issue #34: a wall only has to carry the part along its TOP and stand on its
// FOOT, so the middle of a tall wall can be opened up to save filament. The
// holes are shaped so the wall still prints with no support of its own: every
// edge that roofs a hole climbs at CUT.slope (~55deg from horizontal) or
// steeper, so no layer ever bridges open air. That rules out circles, flat-topped
// hexagons and slots with flat tops, and leaves the shapes below (the honeycomb's
// hexagons stand on a corner, stretched until their roofs are that steep).
//
// A cut wall is still built the repo's way: overlapping closed solids that the
// slicer unions. A solid TOP BAND (the breakaway tip + a rail under it, so the
// contact line and the tine comb are untouched), a solid BOTTOM BAND (the
// flange or floor taper + a rail), a solid END POST at each end, and between
// the posts a row of cells, each a rectangle of wall with one hole (or a
// vertical stack of them) cut out of it. A cell whose hole wouldn't fit stays
// solid, so a low or short wall comes out exactly as it did before.

type P2 = [f64; 2];
type V3 = [f64; 3];

/// The `HOLES` table entry for `kind` (web/cutout.js:57-72): the hole shapes, as their LEFT
/// boundary from bottom to top: [half-width, z] with z as a fraction of the hole's height.
/// The right side mirrors it. Each shape's roof slopes at least CUT.slope, which `holeFits`
/// enforces by making the hole tall enough for its width.
///   diamond  -- the lattice look; roof and floor both slope
///   triangle -- flat floor, pointed roof (a truss)
///   arch     -- a slot with a pointed, gothic roof; the most open of the three
/// plus 'lattice' and 'honeycomb' (latticeHoles, honeycombHoles), which aren't one
/// hole per cell.
struct Shape {
    /// `prof`: `(f, t)` pairs; `t = None` is JS's `null` ("set by slope").
    prof: &'static [(f64, Option<f64>)],
    /// `roof`: `None` is JS's `null`.
    roof: Option<f64>,
}

/// `HOLES[kind]` (web/cutout.js:68-72); `None` for a kind that is not a key.
fn hole_shape(kind: &str) -> Option<Shape> {
    match kind {
        "diamond" => Some(Shape {
            prof: &[(0.0, Some(0.0)), (1.0, Some(0.5)), (0.0, Some(1.0))],
            roof: Some(0.5),
        }),
        "triangle" => Some(Shape {
            prof: &[(1.0, Some(0.0)), (0.0, Some(1.0))],
            roof: Some(1.0),
        }),
        // null: set by slope
        "arch" => Some(Shape {
            prof: &[(1.0, Some(0.0)), (1.0, None), (0.0, Some(1.0))],
            roof: None,
        }),
        _ => None,
    }
}

/// One hole of `holesFor`: `{a, z0, z1}` (half-width, bottom, top).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub a: f64,
    pub z0: f64,
    pub z1: f64,
}

/// `CUTOUT_PATTERNS` (web/cutout.js:73): `['none', ...Object.keys(HOLES), 'lattice', 'honeycomb']`.
/// `HOLES`' keys, in order, are `diamond`, `triangle`, `arch` (web/cutout.js:68-72).
pub const CUTOUT_PATTERNS: [&str; 6] = [
    "none",
    "diamond",
    "triangle",
    "arch",
    "lattice",
    "honeycomb",
];

/// `CUT` (web/cutout.js:27). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct CutConfig {
    // which holes to cut: 'none' or one of CUTOUT_PATTERNS
    pub pattern: String,
    // target cell width along the wall, mm (one hole per cell)
    pub pitch: f64,
    // solid left between neighbouring holes, mm -- the strut width
    pub web: f64,
    // solid kept above the bottom taper and below the top tip, mm
    pub rail: f64,
    // solid end post at each end of the wall, mm
    pub post: f64,
    /// JS key `minHalf`.
    // narrowest hole worth cutting (half-width), mm
    pub min_half: f64,
    /// JS key `minHole`.
    // shortest hole worth cutting, mm
    pub min_hole: f64,
    // a hole's roof rises >= slope per unit across (~55deg): no bridging
    pub slope: f64,
    /// JS key `maxSlope`.
    // a hole taller than this (x half-width) is stacked into several
    pub max_slope: f64,
    // overlap into the neighbouring solid so the union is clean
    pub eps: f64,
    /// JS key `latPitch`.
    // LATTICE: small diamonds in staggered rows filling the whole wall between the
    // bands -- a truss of crossing diagonal struts rather than a row of big holes.
    // centre-to-centre along a row, mm
    pub lat_pitch: f64,
    /// JS key `latStrut`.
    // strut width, measured square to the strut, mm
    pub lat_strut: f64,
    /// JS key `latSlope`.
    // strut rise per unit across (~56deg), steeper than `slope`
    pub lat_slope: f64,
    /// JS key `roofMin`.
    // a clipped lattice hole's roof may be as flat as 45deg, no flatter
    pub roof_min: f64,
    /// JS key `minArea`.
    // smallest lattice hole worth cutting, mm2
    pub min_area: f64,
    /// JS key `hexPitch`.
    // HONEYCOMB: hexagons stood on a corner in staggered rows, the solid left an even
    // strut between every pair -- the lattice's diagonals plus short VERTICAL struts
    // between neighbours in a row (each under the next row's hexagon, so the load
    // still zig-zags down: whether that is stiffer is the cutout coupon's question).
    // centre-to-centre along a row, mm
    pub hex_pitch: f64,
    /// JS key `hexStrut`.
    // strut width, measured square to the strut, mm
    pub hex_strut: f64,
    /// JS key `hexSlope`.
    // roof rise per unit across (~55deg); 1/sqrt(3) is a regular hexagon,
    // whose 30deg roofs the cutout coupon's HEX30 ledge prints -- below
    // roofMin it is honoured as set, so only the coupon ever sets it
    pub hex_slope: f64,
    /// JS key `minSaved`.
    // a wall whose holes open less of its face than this stays solid
    pub min_saved: f64,
}

impl Default for CutConfig {
    fn default() -> Self {
        CutConfig {
            pattern: "none".to_string(),
            pitch: 8.0,
            web: 1.6,
            rail: 1.2,
            post: 2.0,
            min_half: 1.2,
            min_hole: 3.0,
            slope: 1.4,
            max_slope: 3.0,
            eps: 0.05,
            lat_pitch: 6.0,
            lat_strut: 1.2,
            lat_slope: 1.5,
            roof_min: 1.0,
            min_area: 3.0,
            hex_pitch: 6.0,
            hex_strut: 1.2,
            hex_slope: 1.4,
            min_saved: 0.1,
        }
    }
}

/// Port of `holesFor` (web/cutout.js:80-105): the holes that fit a cell `w` wide and `H` tall,
/// as `[{a, z0, z1}]` (half-width, bottom, top), or `[]` when none does. Tall cells stack holes
/// with a `web` between them rather than stretching one into a long thin slit.
pub fn holes_for(kind: &str, w: f64, h: f64, cfg: &Config) -> Vec<Hole> {
    let cut = &cfg.cut;
    let shape = hole_shape(kind);
    let mut a = (w - cut.web) / 2.0;
    let Some(shape) = shape else {
        return Vec::new();
    };
    if a < cut.min_half || h < cut.min_hole {
        return Vec::new();
    }
    // The part of the hole height that is roof, per unit of it: the roof must rise
    // slope*a over that share, so the hole needs at least slope*a/roof of height.
    // The arch's straight sides take up whatever height is left over.
    let roof = shape.roof.unwrap_or(0.0);
    let min_h = |h: f64| -> f64 {
        if roof != 0.0 && !roof.is_nan() {
            cut.slope * h / roof
        } else {
            cut.slope * h + cut.min_hole / 2.0
        }
    };
    let max_h = |h: f64| jsnum::max(min_h(h), 2.0 * cut.max_slope * h);
    let mut n = jsnum::max(1.0, jsnum::ceil((h + cut.web) / (max_h(a) + cut.web)));
    let mut hh = (h - (n - 1.0) * cut.web) / n;
    if hh < min_h(a) && n > 1.0 {
        // stacking made them squat: one fewer, full width
        n -= 1.0;
        hh = (h - (n - 1.0) * cut.web) / n; // (a bit over maxH -- only a steeper roof)
    } else if hh < min_h(a) {
        // a lone hole too squat for its width: narrow it
        a = if roof != 0.0 && !roof.is_nan() {
            hh * roof / cut.slope
        } else {
            (hh - cut.min_hole / 2.0) / cut.slope
        };
        if a < cut.min_half {
            return Vec::new();
        }
    }
    let mut holes = Vec::new();
    let mut j = 0.0;
    while j < n {
        let z0 = j * (hh + cut.web);
        holes.push(Hole { a, z0, z1: z0 + hh });
        j += 1.0;
    }
    holes
}

/// Port of `cellHoles` (web/cutout.js:112-124): the per-cell pattern's hole(s) for one cell, as
/// convex CCW polygons in (s, z): up the right side, back down the left (a pointed tip appears
/// once).
fn cell_holes(kind: &str, s0: f64, s1: f64, lo: f64, holes: &[Hole], cfg: &Config) -> Vec<Vec<P2>> {
    let cut = &cfg.cut;
    let sm = (s0 + s1) / 2.0;
    let shape = hole_shape(kind).expect("HOLES[kind]");
    holes
        .iter()
        .map(|&Hole { a, z0, z1 }| {
            let b0 = lo + z0;
            let b1 = lo + z1;
            let prof: Vec<P2> = shape
                .prof
                .iter()
                .map(|&(f, t)| {
                    [
                        f * a,
                        match t {
                            None => b1 - cut.slope * a,
                            Some(t) => b0 + t * (b1 - b0),
                        },
                    ]
                })
                .collect();
            let mut poly: Vec<P2> = prof.iter().map(|&[w, z]| [sm + w, z]).collect();
            poly.extend(prof.iter().rev().map(|&[w, z]| [sm - w, z]));
            let n = poly.len();
            poly.iter()
                .enumerate()
                .filter(|&(i, v)| {
                    let u = poly[(i + 1) % n];
                    jsnum::abs(u[0] - v[0]) > 1e-9 || jsnum::abs(u[1] - v[1]) > 1e-9
                })
                .map(|(_, v)| *v)
                .collect()
        })
        .collect()
}

/// Port of `clipPoly` (web/cutout.js:127-139): clip a convex CCW polygon to the half-plane
/// f(v) >= 0 (f linear).
fn clip_poly(poly: &[P2], f: impl Fn(&P2) -> f64) -> Vec<P2> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let fa = f(&a);
        let fb = f(&b);
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Port of `polyArea` (web/cutout.js:141-148).
fn poly_area(p: &[P2]) -> f64 {
    let mut area = 0.0;
    for i in 0..p.len() {
        let [x0, y0] = p[i];
        let [x1, y1] = p[(i + 1) % p.len()];
        area += x0 * y1 - x1 * y0;
    }
    area / 2.0
}

/// Port of `roofsOk` (web/cutout.js:151-161): does every ROOF edge (solid above it) rise at
/// least `k` per unit across?
fn roofs_ok(p: &[P2], k: f64) -> bool {
    for i in 0..p.len() {
        let [x0, z0] = p[i];
        let [x1, z1] = p[(i + 1) % p.len()];
        let dx = x1 - x0;
        let dz = z1 - z0;
        // CCW: the hole is on the left, so an edge running LEFTWARD has the hole below
        // it and solid above -- a roof.
        if dx < -1e-9 && jsnum::abs(dz) < k * -dx - 1e-9 {
            return false;
        }
    }
    true
}

/// The closure `zoneFit` returns (web/cutout.js:170-231): a function taking a convex CCW hole
/// and returning it fitted to the zone's real outline, or `None` when no useful hole is left.
struct ZoneFit<'a> {
    s: &'a [f64],
    z_lo: &'a [f64],
    z_hi: &'a [f64],
    s_a: f64,
    s_b: f64,
    roof_min: f64,
    segs: Vec<usize>,
    cfg: &'a Config,
}

/// Port of `zoneFit` (web/cutout.js:162-232): the clip shared by the patterns that tile the
/// whole zone (lattice, honeycomb). Roofs the clip leaves flatter than `roofMin` are cut back
/// to it.
fn zone_fit<'a>(
    s: &'a [f64],
    z_lo: &'a [f64],
    z_hi: &'a [f64],
    s_a: f64,
    s_b: f64,
    roof_min: f64,
    cfg: &'a Config,
) -> ZoneFit<'a> {
    // the zone's edges as half-planes, one per station segment (convex clip is
    // conservative where the outline bends: the hole only ever gets smaller)
    let mut segs = Vec::new();
    let mut i = 0;
    while i + 1 < s.len() {
        if !(s[i + 1] <= s_a || s[i] >= s_b || s[i + 1] - s[i] < 1e-9) {
            segs.push(i);
        }
        i += 1;
    }
    ZoneFit {
        s,
        z_lo,
        z_hi,
        s_a,
        s_b,
        roof_min,
        segs,
        cfg,
    }
}

impl ZoneFit<'_> {
    fn fit(&self, poly: &[P2]) -> Option<Vec<P2>> {
        let s = self.s;
        let roof_min = self.roof_min;
        let cut = &self.cfg.cut;
        let (s_a, s_b) = (self.s_a, self.s_b);
        let line = |zs: &[f64], i: usize| {
            let zs = zs.to_vec();
            move |v: &P2| zs[i] + (zs[i + 1] - zs[i]) * (v[0] - s[i]) / (s[i + 1] - s[i])
        };
        let mut p = clip_poly(poly, |v| v[0] - s_a);
        p = clip_poly(&p, |v| s_b - v[0]);
        for &i in &self.segs {
            let (lo, hi) = (s[i], s[i + 1]);
            // a segment only bounds the hole over its own stretch of the wall
            if !p.iter().any(|v| v[0] > lo - 1e-9) || !p.iter().any(|v| v[0] < hi + 1e-9) {
                continue;
            }
            let top = line(self.z_hi, i);
            let bot = line(self.z_lo, i);
            p = clip_poly(&p, |v| top(v) - v[1]);
            p = clip_poly(&p, |v| v[1] - bot(v));
            if p.len() < 3 {
                return None;
            }
        }
        // A roof clipped to a zone edge flatter than roofMin would be a bridge. Rather
        // than give the hole up, drop that roof to exactly roofMin from its HIGH
        // corner: the hole keeps its reach along the slope with a printable roof.
        let mut pass = 0;
        while pass < 4 && p.len() >= 3 && !roofs_ok(&p, roof_min) {
            for n in 0..p.len() {
                let a = p[n];
                let b = p[(n + 1) % p.len()];
                let dx = b[0] - a[0];
                let dz = b[1] - a[1];
                if !(dx < -1e-9 && jsnum::abs(dz) < roof_min * -dx - 1e-9) {
                    continue;
                }
                if jsnum::abs(dz) < 0.25 * -dx {
                    // a near-level roof: peak it in the middle, a gable rather than a lean-to
                    let m = [(a[0] + b[0]) / 2.0, jsnum::min(a[1], b[1])];
                    p = clip_poly(&p, |v| m[1] - roof_min * (v[0] - m[0]) - v[1]);
                    p = clip_poly(&p, |v| m[1] + roof_min * (v[0] - m[0]) - v[1]);
                } else {
                    let (h, lo) = if a[1] >= b[1] { (a, b) } else { (b, a) };
                    let d = jsnum::sign(lo[0] - h[0]);
                    p = clip_poly(&p, |v| h[1] - roof_min * (v[0] - h[0]) * d - v[1]);
                }
                break;
            }
            pass += 1;
        }
        if p.len() < 3 || poly_area(&p) < cut.min_area || !roofs_ok(&p, roof_min) {
            return None;
        }
        // Too thin to be a hole: a sliver clipped along the zone's edge has room
        // lengthwise but none across. 2*area/perimeter is the inradius of a triangle
        // and a fair stand-in for any convex shape's half-thickness.
        let mut per = 0.0;
        for n in 0..p.len() {
            let u = p[n];
            let v = p[(n + 1) % p.len()];
            per += jsnum::hypot(&[v[0] - u[0], v[1] - u[1]]);
        }
        if 2.0 * poly_area(&p) / per < cut.min_half / 1.5 {
            return None;
        }
        Some(p)
    }
}

/// Port of `zoneSpan` (web/cutout.js:235-238): where the zone's holes start and stop: its
/// lowest floor and highest top.
fn zone_span(s: &[f64], z_lo: &[f64], z_hi: &[f64], s_a: f64, s_b: f64) -> (f64, f64) {
    let pick = |zs: &[f64]| -> Vec<f64> {
        zs.iter()
            .enumerate()
            .filter(|&(i, _)| s[i] >= s_a && s[i] <= s_b)
            .map(|(_, &z)| z)
            .collect()
    };
    (jsnum::min_of(&pick(z_lo)), jsnum::max_of(&pick(z_hi)))
}

/// Port of `latticeHoles` (web/cutout.js:253-277): LATTICE: diamonds in staggered rows over
/// the whole hole zone, the solid left between them a truss of diagonal struts. The zone is the
/// wall's REAL outline (under the top rail, over the foot rail, between the end posts), so on a
/// triangular fin the lattice runs right up the slope instead of stopping at the first cell
/// that is short at one end.
///
/// At the edges each diamond is clipped to the zone, and kept if every roof it ends up with
/// still rises at CUT.roofMin (45deg) -- a slope as steep as that becomes a hole's roof, so a
/// fin under a 45deg face gets triangles hugging it. A flatter edge would leave a bridge, so
/// there the diamond is shrunk instead until it fits whole. Anything smaller than a useful
/// hole is left solid.
fn lattice_holes(
    s: &[f64],
    z_lo: &[f64],
    z_hi: &[f64],
    s_a: f64,
    s_b: f64,
    cfg: &Config,
) -> Vec<Vec<P2>> {
    let cut = &cfg.cut;
    let k = cut.lat_slope;
    let p = cut.lat_pitch;
    let inset = cut.lat_strut / 2.0 / (k / jsnum::hypot(&[1.0, k])); // strut half-width, measured flat
    let a = p / 2.0 - inset;
    let b = k * a;
    if a < cut.min_half {
        return Vec::new();
    }
    let fit = zone_fit(s, z_lo, z_hi, s_a, s_b, cut.roof_min, cfg);
    let (z0, z_top) = zone_span(s, z_lo, z_hi, s_a, s_b);
    let cols = jsnum::ceil((s_b - s_a) / p) + 1.0;
    let mut holes = Vec::new();
    let mut j = 0.0;
    while z0 + j * (k * p / 2.0) - b <= z_top {
        let zc = z0 + b * 0.5 + j * (k * p / 2.0);
        let mut i = -1.0;
        while i <= cols {
            let sm =
                s_a + cut.lat_strut / 2.0 + a + i * p + if j % 2.0 != 0.0 { p / 2.0 } else { 0.0 };
            if sm + a < s_a || sm - a > s_b {
                i += 1.0;
                continue;
            }
            for f in [1.0, 0.8, 0.62, 0.46, 0.34] {
                let q = fit.fit(&[
                    [sm, zc - b * f],
                    [sm + a * f, zc],
                    [sm, zc + b * f],
                    [sm - a * f, zc],
                ]);
                if let Some(q) = q {
                    holes.push(q);
                    break;
                }
            }
            i += 1.0;
        }
        j += 1.0;
    }
    holes
}

/// Port of `honeycombHoles` (web/cutout.js:288-312): HONEYCOMB: hexagons stood on a corner, in
/// staggered rows, fitted to the zone as the lattice's diamonds are. Each has vertical sides
/// `h` tall and a pointed roof and floor rising CUT.hexSlope per unit across; `h` is the roof
/// edge's length, so at a slope of 1/sqrt(3) it is a regular hexagon and steeper it is one
/// stretched upright. Neighbours in a row leave a vertical strut CUT.hexStrut wide; the next
/// row sits in the notches, its floors CUT.hexStrut square to the roofs below.
pub fn honeycomb_holes(
    s: &[f64],
    z_lo: &[f64],
    z_hi: &[f64],
    s_a: f64,
    s_b: f64,
    cfg: &Config,
) -> Vec<Vec<P2>> {
    let cut = &cfg.cut;
    let k = cut.hex_slope;
    let p = cut.hex_pitch;
    let t = cut.hex_strut;
    let a = (p - t) / 2.0;
    let r = k * a;
    let h = a * jsnum::hypot(&[1.0, k]);
    if a < cut.min_half {
        return Vec::new();
    }
    // row to row: the floor above runs parallel to the roof below, t apart square to it
    let dz = h + k * (p / 2.0 - t) + t * jsnum::hypot(&[1.0, k]);
    let fit = zone_fit(s, z_lo, z_hi, s_a, s_b, jsnum::min(cut.roof_min, k), cfg);
    let (z0, z_top) = zone_span(s, z_lo, z_hi, s_a, s_b);
    let half = h / 2.0 + r; // centre to tip
    let cols = jsnum::ceil((s_b - s_a) / p) + 1.0;
    let mut holes = Vec::new();
    let mut j = 0.0;
    while z0 + half * 0.5 + j * dz - half <= z_top {
        let zc = z0 + half * 0.5 + j * dz;
        let mut i = -1.0;
        while i <= cols {
            let sm = s_a + t / 2.0 + a + i * p + if j % 2.0 != 0.0 { p / 2.0 } else { 0.0 };
            if sm + a < s_a || sm - a > s_b {
                i += 1.0;
                continue;
            }
            for f in [1.0, 0.8, 0.62, 0.46, 0.34] {
                let q = fit.fit(&[
                    [sm, zc - half * f],
                    [sm + a * f, zc - h / 2.0 * f],
                    [sm + a * f, zc + h / 2.0 * f],
                    [sm, zc + half * f],
                    [sm - a * f, zc + h / 2.0 * f],
                    [sm - a * f, zc - h / 2.0 * f],
                ]);
                if let Some(q) = q {
                    holes.push(q);
                    break;
                }
            }
            i += 1.0;
        }
        j += 1.0;
    }
    holes
}

/// Port of `spanAt` (web/cutout.js:315-326): a convex polygon's extent [left, right] at height
/// z, or `None` if it misses.
fn span_at(p: &[P2], z: f64) -> Option<[f64; 2]> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for i in 0..p.len() {
        let [x0, z0] = p[i];
        let [x1, z1] = p[(i + 1) % p.len()];
        if (z0 - z) * (z1 - z) > 0.0 {
            continue;
        }
        if jsnum::abs(z1 - z0) < 1e-12 {
            lo = jsnum::min_of(&[lo, x0, x1]);
            hi = jsnum::max_of(&[hi, x0, x1]);
            continue;
        }
        let x = x0 + (x1 - x0) * (z - z0) / (z1 - z0);
        lo = jsnum::min(lo, x);
        hi = jsnum::max(hi, x);
    }
    if lo <= hi { Some([lo, hi]) } else { None }
}

/// One edge of the solid inside a slab, "as functions of z" (`L` / `R` in `stripSolid`):
/// either a constant (the strip's side) or the line through an obstacle's edge.
enum Edge {
    Const(f64),
    Lerp {
        m1: f64,
        m2: f64,
        q1: f64,
        q2: f64,
        s0: f64,
        s1: f64,
    },
}

impl Edge {
    fn at(&self, z: f64) -> f64 {
        match *self {
            Edge::Const(x) => x,
            Edge::Lerp {
                m1,
                m2,
                q1,
                q2,
                s0,
                s1,
            } => jsnum::max(s0, jsnum::min(s1, m1 + (m2 - m1) * (z - q1) / (q2 - q1))),
        }
    }
}

/// `(a, b) => a - b` as a sort comparator: a NaN difference counts as 0 (equal), as in JS.
fn by_difference(a: f64, b: f64) -> std::cmp::Ordering {
    let d = a - b;
    if d < 0.0 {
        std::cmp::Ordering::Less
    } else if d > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// Port of `stripSolid` (web/cutout.js:336-398): the solid of the strip [s0,s1] x [zb,zt] minus
/// `obst` (convex polygons: the holes, plus the regions above the zone's top edge and below its
/// bottom edge), as convex CCW polygons in (s, z). The strip is cut into horizontal slabs at
/// every obstacle corner and wherever an obstacle edge crosses the strip's side; inside a slab
/// every obstacle edge is one straight line, so the solid is a run of trapezoids between
/// neighbouring obstacles -- always convex.
fn strip_solid(s0: f64, s1: f64, zb: f64, zt: f64, obst: &[Vec<P2>]) -> Vec<Vec<P2>> {
    let os: Vec<&Vec<P2>> = obst
        .iter()
        .filter(|p| {
            p.iter().any(|v| v[0] > s0)
                && p.iter().any(|v| v[0] < s1)
                && p.iter().any(|v| v[1] > zb)
                && p.iter().any(|v| v[1] < zt)
        })
        .collect();
    let mut zs = vec![zb, zt];
    for p in &os {
        for i in 0..p.len() {
            let [x0, z0] = p[i];
            let [x1, z1] = p[(i + 1) % p.len()];
            zs.push(z0);
            for side in [s0, s1] {
                let t = (side - x0) / (x1 - x0);
                if x1 != x0 && t > 0.0 && t < 1.0 {
                    zs.push(z0 + t * (z1 - z0));
                }
            }
        }
    }
    // `[...new Set(zs.filter(...).map((z) => +z.toFixed(9)))].sort((u, v) => u - v)`
    let mut cuts: Vec<f64> = Vec::new();
    for &z in zs.iter().filter(|&&z| z >= zb && z <= zt) {
        let r = jsnum::string_to_number(&jsnum::to_fixed(z, 9));
        // Set membership is SameValueZero: -0 equals 0, NaN equals NaN
        if !cuts.iter().any(|&c| c == r || (c.is_nan() && r.is_nan())) {
            cuts.push(r);
        }
    }
    cuts.sort_by(|u, v| by_difference(*u, *v));
    let mut polys = Vec::new();
    for n in 0..cuts.len().saturating_sub(1) {
        let za = cuts[n];
        let zc = cuts[n + 1];
        if zc - za < 1e-7 {
            continue;
        }
        let zm = (za + zc) / 2.0;
        let mut act: Vec<&Vec<P2>> = Vec::new();
        for p in &os {
            if let Some(m) = span_at(p, zm)
                && m[1] > s0
                && m[0] < s1
            {
                act.push(p);
            }
        }
        act.sort_by(|u, v| by_difference(span_at(u, zm).unwrap()[0], span_at(v, zm).unwrap()[0]));
        // The solid's edges in this slab, as functions of z. Every obstacle edge is
        // straight across the slab, so each is read at two interior heights and
        // extended to the slab's ends -- reading AT an end can fall a rounding hair
        // outside an obstacle's corner and miss it.
        let mut l = vec![Edge::Const(s0)];
        let mut r: Vec<Edge> = Vec::new();
        let q1 = za + (zc - za) / 4.0;
        let q2 = zc - (zc - za) / 4.0;
        for p in &act {
            let m1 = span_at(p, q1).or_else(|| span_at(p, zm)).unwrap();
            let m2 = span_at(p, q2).or_else(|| span_at(p, zm)).unwrap();
            let lerp = |k: usize| Edge::Lerp {
                m1: m1[k],
                m2: m2[k],
                q1,
                q2,
                s0,
                s1,
            };
            r.push(lerp(0));
            l.push(lerp(1));
        }
        r.push(Edge::Const(s1));
        for g in 0..l.len() {
            let q = [
                [l[g].at(za), za],
                [r[g].at(za), za],
                [r[g].at(zc), zc],
                [l[g].at(zc), zc],
            ];
            let w0 = q[1][0] - q[0][0];
            let w1 = q[2][0] - q[3][0];
            if w0 < 1e-7 && w1 < 1e-7 {
                continue;
            }
            // a zero-width end (a hole's tip) makes the piece a triangle
            polys.push(if w0 < 1e-7 {
                vec![q[0], q[2], q[3]]
            } else if w1 < 1e-7 {
                vec![q[0], q[1], q[2]]
            } else {
                q.to_vec()
            });
        }
    }
    polys
}

/// One station of a swept wall, as `cutWall` reads it (the objects pushed by
/// prop/sweep.js:119 and :218): `{ p, sx, sy, top, ztip, holeTop, tipHalf, sh, bot, botTip,
/// taperBot, botN, botP }`. `None` is JS's `undefined`.
#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    /// the station's point in XY
    pub p: [f64; 2],
    /// across-the-wall unit vector
    pub sx: f64,
    pub sy: f64,
    pub top: f64,
    pub ztip: f64,
    /// `holeTop`: where the wall's taper began (the holes stop there under a flat crest)
    pub hole_top: Option<f64>,
    /// `tipHalf`: narrower when an interface crest sits on it
    pub tip_half: Option<f64>,
    /// `sh`: the station's body half-width, narrower on a low wide wall (044)
    pub sh: Option<f64>,
    pub bot: f64,
    pub bot_tip: f64,
    /// `taperBot` (a JS boolean)
    pub taper_bot: bool,
    /// `botN` / `botP`: a tilted lifted bottom (sweepBetween)
    pub bot_n: Option<f64>,
    pub bot_p: Option<f64>,
}

/// The `wall` argument of `cutWall`: the wall's own numbers (web/cutout.js:375-378). prop/sweep.js
/// passes `PROP` (`Wall::from(&cfg.prop)`); fins/wedges.js:127 passes
/// `{ th: PERP.th, tip: PERP.th, minStations: 3 }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub th: f64,
    pub tip: f64,
    pub min_stations: f64,
}

impl From<&crate::prop::config::PropConfig> for Wall {
    fn from(p: &crate::prop::config::PropConfig) -> Wall {
        Wall {
            th: p.th,
            tip: p.tip,
            min_stations: p.min_stations,
        }
    }
}

/// Port of `cutWall` (web/cutout.js:379-590): emit `st` (per-station wall data from sweep /
/// sweepBetween) as a wall with CUT.pattern holes, or return false to have the caller emit its
/// solid ribbon. `full` is the caller's solid sections, reused for the end posts.
pub fn cut_wall(
    st: &[Station],
    full: &[Vec<V3>],
    out: &mut Vec<V3>,
    wall: &Wall,
    cfg: &Config,
) -> bool {
    let cut = &cfg.cut;
    let kind = cut.pattern.as_str();
    if !CUTOUT_PATTERNS.contains(&kind) || kind == "none" || (st.len() as f64) < wall.min_stations {
        return false;
    }
    // arclength of each station along the wall, in XY
    let mut s = vec![0.0];
    for i in 1..st.len() {
        s.push(
            s[i - 1] + jsnum::hypot(&[st[i].p[0] - st[i - 1].p[0], st[i].p[1] - st[i - 1].p[1]]),
        );
    }
    let l = s[s.len() - 1];
    let n_cells = jsnum::floor((l - 2.0 * cut.post) / cut.pitch);
    if n_cells < 1.0 {
        return false;
    }
    // Cells: strips between stations about CUT.pitch wide, snapped to stations so
    // each strip's edges sit on real cross-sections. The one-hole-per-cell
    // patterns put their hole in each; for every pattern they are also the strips
    // the solid is built in.
    let snap = |x: f64| -> usize {
        let mut best = 0;
        for i in 1..s.len() {
            if jsnum::abs(s[i] - x) < jsnum::abs(s[best] - x) {
                best = i;
            }
        }
        best
    };
    // The cells share the length between the two end posts evenly, so the posts
    // stay CUT.post wide instead of soaking up the remainder.
    let mut edges: Vec<usize> = Vec::new();
    let mut c = 0.0;
    while c <= n_cells {
        let i = snap(cut.post + c * (l - 2.0 * cut.post) / n_cells);
        if edges.is_empty() || i > edges[edges.len() - 1] {
            edges.push(i);
        }
        c += 1.0;
    }
    if edges.len() < 2 {
        return false;
    }
    // The hole zone at each station: under the top rail, over the foot rail. Where
    // the wall is too short for any zone the two meet in the middle.
    let mut z_hi = Vec::new();
    let mut z_lo = Vec::new();
    for q in st {
        // under a flat interface crest ztip is the crest's cut; the holes still stop
        // where the wall's taper began (holeTop), so the crest takes no plastic from them
        let hi = q.hole_top.unwrap_or(q.ztip) - cut.rail;
        let lo = q.bot_tip + cut.rail;
        let m = (hi + lo) / 2.0;
        z_hi.push(if hi > lo { hi } else { m });
        z_lo.push(if hi > lo { lo } else { m });
    }
    let first = edges[0];
    let last = edges[edges.len() - 1];
    let s_a = s[first];
    let s_b = s[last];
    let holes: Vec<Vec<P2>> = if kind == "lattice" {
        lattice_holes(&s, &z_lo, &z_hi, s_a, s_b, cfg)
    } else if kind == "honeycomb" {
        honeycomb_holes(&s, &z_lo, &z_hi, s_a, s_b, cfg)
    } else {
        let mut holes = Vec::new();
        for c in 0..edges.len() - 1 {
            let i0 = edges[c];
            let i1 = edges[c + 1];
            let mut hi = f64::INFINITY;
            let mut lo = f64::NEG_INFINITY;
            for i in i0..=i1 {
                hi = jsnum::min(hi, z_hi[i]);
                lo = jsnum::max(lo, z_lo[i]);
            }
            holes.extend(cell_holes(
                kind,
                s[i0],
                s[i1],
                lo,
                &holes_for(kind, s[i1] - s[i0], hi - lo, cfg),
                cfg,
            ));
        }
        holes
    };
    if holes.is_empty() {
        return false;
    }
    // Not worth it for a hole or two: a wall whose holes would take out less than
    // CUT.minSaved of its face stays solid -- the extra pieces cost triangles and
    // overlap for next to no plastic saved.
    let mut face = 0.0;
    let mut open = 0.0;
    for i in 0..st.len() - 1 {
        face +=
            (s[i + 1] - s[i]) * ((st[i].top - st[i].bot) + (st[i + 1].top - st[i + 1].bot)) / 2.0;
    }
    for h in &holes {
        open += poly_area(h);
    }
    if open < cut.min_saved * face {
        return false;
    }

    // Bands: the top band reaches down to the zone's top edge, the bottom band up
    // to its bottom edge, station by station.
    let th = wall.th / 2.0;
    let tp = wall.tip / 2.0;
    // Only between the end posts: the posts are solid full height already, and a
    // band running on under them would just be plastic counted twice.
    let mut top: Vec<Vec<V3>> = Vec::new();
    let mut bot: Vec<Vec<V3>> = Vec::new();
    for i in first..=last {
        let q = &st[i];
        let pt = |o: f64, z: f64| -> V3 { [q.p[0] + q.sx * o, q.p[1] + q.sy * o, z] };
        // keep each band inside the wall's own profile. Where the zone has closed up
        // the bands would meet flush at its middle (issue #223), so they overlap there.
        let shut = if z_hi[i] - z_lo[i] < 1e-9 {
            cut.eps / 2.0
        } else {
            0.0
        };
        let ct = jsnum::max(q.bot_tip, jsnum::min(z_hi[i] - shut, q.ztip - 0.01));
        let cb = jsnum::min(q.ztip, jsnum::max(z_lo[i] + shut, q.bot_tip + 0.01));
        let tq = q.tip_half.unwrap_or(tp); // narrower when an interface crest sits on it
        let w = q.sh.unwrap_or(th); // the station's body: narrower on a low wide wall (044)
        top.push(vec![
            pt(w, ct),
            pt(w, q.ztip),
            pt(tq, q.top),
            pt(-tq, q.top),
            pt(-w, q.ztip),
            pt(-w, ct),
        ]);
        bot.push(if q.taper_bot {
            vec![
                pt(tp, q.bot),
                pt(w, q.bot_tip),
                pt(w, cb),
                pt(-w, cb),
                pt(-w, q.bot_tip),
                pt(-tp, q.bot),
            ]
        } else {
            // a tilted lifted bottom (sweepBetween)
            vec![
                pt(w, q.bot_p.unwrap_or(q.bot)),
                pt(w, cb),
                pt(-w, cb),
                pt(-w, q.bot_n.unwrap_or(q.bot)),
            ]
        });
    }
    // Against an end post a band starts 2 * CUT.eps inside it, on the line of its
    // first segment (same surface): ending at the post's own cap, it shared the
    // cap's tip edges -- flush, four faces to an edge (issue #223). Twice eps, past
    // the strips' own reach into the post (0.4-0.6 eps), so a band corner never
    // lands on one of theirs.
    let into = |top: &mut Vec<Vec<V3>>, bot: &mut Vec<Vec<V3>>, k: usize, k2: isize| {
        let f = 2.0 * cut.eps
            / jsnum::max(
                1e-9,
                jsnum::abs(s[edges[k]] - s[(edges[k] as isize + k2) as usize]),
            );
        for band in [top, bot] {
            let j = if k == 0 { 0 } else { band.len() - 1 };
            let n = (j as isize + k2) as usize;
            let moved: Vec<V3> = band[j]
                .iter()
                .enumerate()
                .map(|(m, v)| {
                    let mut o = *v;
                    for d in 0..3 {
                        o[d] = v[d] + (v[d] - band[n][m][d]) * f;
                    }
                    o
                })
                .collect();
            band[j] = moved;
        }
    };
    if top.len() > 1 && edges[0] > 0 {
        into(&mut top, &mut bot, 0, 1);
    }
    if top.len() > 1 && edges[edges.len() - 1] < st.len() - 1 {
        into(&mut top, &mut bot, edges.len() - 1, -1);
    }
    ribbon(&top, out);
    ribbon(&bot, out);
    // End posts: the caller's full sections outside the cut span, so the wall's
    // ends stay solid pillars.
    if edges[0] > 0 {
        ribbon(&full[..edges[0] + 1], out);
    }
    if edges[edges.len() - 1] < st.len() - 1 {
        ribbon(&full[edges[edges.len() - 1]..], out);
    }

    // The strips: zone minus holes, mapped from (s, z) onto the wall. Across-wall
    // direction and XY position are interpolated between stations, so a gently
    // curved auto wall is followed as closely as the ribbon follows it.
    let at = |x: f64| -> (f64, f64, f64, f64) {
        let mut i = 0;
        while i + 2 < s.len() && s[i + 1] < x {
            i += 1;
        }
        let t = jsnum::max(
            -1.0,
            jsnum::min(2.0, (x - s[i]) / jsnum::max(1e-9, s[i + 1] - s[i])),
        );
        let (a, b) = (&st[i], &st[i + 1]);
        let lerp = |u: f64, v: f64| u + (v - u) * t;
        let mut sx = lerp(a.sx, b.sx);
        let mut sy = lerp(a.sy, b.sy);
        let n = jsnum::hypot(&[sx, sy]);
        sx /= n;
        sy /= n;
        (lerp(a.p[0], b.p[0]), lerp(a.p[1], b.p[1]), sx, sy)
    };
    let e = cut.eps;
    let z_min = jsnum::min_of(&z_lo) - 1.0;
    let z_max = jsnum::max_of(&z_hi) + 1.0;
    // Beyond the zone's top and bottom edges, per station segment: obstacles like
    // the holes, pushed eps INTO the bands so the strips overlap them.
    let mut beyond: Vec<Vec<P2>> = Vec::new();
    for i in first..last {
        if s[i + 1] - s[i] < 1e-9 {
            continue;
        }
        beyond.push(vec![
            [s[i], z_hi[i] + e],
            [s[i + 1], z_hi[i + 1] + e],
            [s[i + 1], z_max + 1.0],
            [s[i], z_max + 1.0],
        ]);
        beyond.push(vec![
            [s[i], z_min - 1.0],
            [s[i + 1], z_min - 1.0],
            [s[i + 1], z_lo[i + 1] - e],
            [s[i], z_lo[i] - e],
        ]);
    }
    let mut obst: Vec<Vec<P2>> = holes.clone();
    obst.extend(beyond);
    // A strip's pieces tile it exactly: left as they are they meet FLUSH -- at every
    // slab cut, across every strip side, corner to corner at a hole's tip -- and
    // share the edges a slicer welds, four or six faces to an edge, which it reports
    // as non-manifold (issue #223). So every edge of a piece moves outward by
    // 0.4-0.6 e, and the piece is the region inside the moved edges -- an
    // intersection of half-planes, so still convex -- overlapping each neighbour by
    // about e across every shared boundary. Each piece takes its own distance in
    // that range (a fixed golden-ratio step, so the build stays deterministic): with
    // one distance for all, two pieces e apart (a slab e tall is common -- the
    // zone's edges sit e from the bands) grew into it and met flush in the middle.
    // A hole within reach stays out: the piece and the hole are disjoint and convex,
    // so one of their edges separates them, and the grown piece is clipped to that
    // line (a hole's own edge stays exactly where it was).
    let side = |a: P2, b: P2, v: P2| (b[0] - a[0]) * (v[1] - a[1]) - (b[1] - a[1]) * (v[0] - a[0]);
    // V outside A->B (on its right; holes and pieces are CCW), to within a micron so
    // shapes that touch along an edge or at a corner count; a lattice hole can
    // repeat a corner, and a zero-length edge separates nothing
    let outside = |a: P2, b: P2, v: P2| {
        let len = jsnum::hypot(&[b[0] - a[0], b[1] - a[1]]);
        len > 1e-9 && side(a, b, v) <= 1e-6 * len
    };
    let mut pieces = 0.0f64;
    let mut grow = |poly: &[P2]| -> Vec<P2> {
        pieces += 1.0;
        let g = e * (0.4 + 0.2 * ((pieces * 0.6180339887498949) % 1.0));
        let mut lo0 = f64::INFINITY;
        let mut lo1 = f64::INFINITY;
        let mut hi0 = f64::NEG_INFINITY;
        let mut hi1 = f64::NEG_INFINITY;
        for &[u, v] in poly {
            lo0 = jsnum::min(lo0, u);
            hi0 = jsnum::max(hi0, u);
            lo1 = jsnum::min(lo1, v);
            hi1 = jsnum::max(hi1, v);
        }
        // the box a piece may grow into: g past its own extent, no further (a sharp
        // corner's moved edges would otherwise meet far out)
        let mut gg: Vec<P2> = vec![
            [lo0 - g, lo1 - g],
            [hi0 + g, lo1 - g],
            [hi0 + g, hi1 + g],
            [lo0 - g, hi1 + g],
        ];
        // the piece's side of A->B's line, moved out by d
        let keep = |gg: &mut Vec<P2>, a: P2, b: P2, d: f64| {
            let len = jsnum::hypot(&[b[0] - a[0], b[1] - a[1]]);
            if len >= 1e-9 {
                *gg = clip_poly(gg, |v| side(a, b, *v) / len + d);
            }
        };
        for n in 0..poly.len() {
            keep(&mut gg, poly[n], poly[(n + 1) % poly.len()], g);
        }
        for h in &holes {
            if gg.len() < 3 {
                break;
            }
            let near = h.iter().enumerate().any(|(n, v)| {
                let w = h[(n + 1) % h.len()];
                jsnum::max(v[0], w[0]) >= lo0 - e
                    && jsnum::min(v[0], w[0]) <= hi0 + e
                    && jsnum::max(v[1], w[1]) >= lo1 - e
                    && jsnum::min(v[1], w[1]) <= hi1 + e
            });
            if !near {
                continue;
            }
            // a hole edge with the whole piece outside it, or else a piece edge with the
            // whole hole outside it (that edge then stays put)
            let mut cutl: Option<(P2, P2)> = None;
            let mut n = 0;
            while n < h.len() && cutl.is_none() {
                let a = h[n];
                let b = h[(n + 1) % h.len()];
                if poly.iter().all(|v| outside(a, b, *v)) {
                    cutl = Some((b, a));
                }
                n += 1;
            }
            let mut n = 0;
            while n < poly.len() && cutl.is_none() {
                let a = poly[n];
                let b = poly[(n + 1) % poly.len()];
                if h.iter().all(|v| outside(a, b, *v)) {
                    cutl = Some((a, b));
                }
                n += 1;
            }
            // nothing separates them only if the piece already overlaps the hole: then
            // it doesn't grow at all, rather than grow further in
            let Some((c0, c1)) = cutl else {
                return poly.to_vec();
            };
            keep(&mut gg, c0, c1, 0.0);
        }
        gg
    };
    let place = |u: f64, v: f64, o: f64| -> V3 {
        let (x, y, sx, sy) = at(u);
        [x + sx * o, y + sy * o, v]
    };
    for c in 0..edges.len() - 1 {
        let s0 = s[edges[c]];
        let s1 = s[edges[c + 1]];
        let mut zb = f64::INFINITY;
        let mut zt = f64::NEG_INFINITY;
        let mut sw = th; // a strip no wider than the narrowest body it spans (044)
        for i in edges[c]..=edges[c + 1] {
            zb = jsnum::min(zb, z_lo[i] - e);
            zt = jsnum::max(zt, z_hi[i] + e);
            sw = jsnum::min(sw, st[i].sh.unwrap_or(th));
        }
        for poly in strip_solid(s0, s1, zb, zt, &obst) {
            // corners closer than 0.1 micron are one corner to a slicer, which welds
            // them: left twice, the extrusion has zero-area faces
            let grown = grow(&poly);
            let g: Vec<P2> = grown
                .iter()
                .enumerate()
                .filter(|&(n, v)| {
                    let u = grown[(n + 1) % grown.len()];
                    jsnum::abs(u[0] - v[0]) > 1e-4 || jsnum::abs(u[1] - v[1]) > 1e-4
                })
                .map(|(_, v)| *v)
                .collect();
            if g.len() >= 3 {
                box_extrude(&g, -sw, sw, place, out);
            }
        }
    }
    true
}
