//! Port of `web/fins/coverage.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Which red counts, and what holds it: the one rule the engine and the coverage scoreboard
//! share. Policy 2026-10-05: every red face must get something, except a TINY HOLE's ceiling
//! (it stays flagged, unsupported); the NEAR-PLATE strip is reported apart. Every other overhang
//! face is MUST.

use std::cell::OnceCell;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::fins::seating::seated_part_tris;
use crate::fxhash::FxHashMap;
use crate::jsnum;
use crate::overhangs::{Analysis, Region, Topology};
use crate::prop::surface::surface_hits_at;
use crate::tris::Tris;
use crate::types::Rot3;

/// `PLATE_Z` (web/fins/coverage.js:17): a face this low is held by the plate itself.
pub const PLATE_Z: f64 = 0.6;

/// `MUST` (web/fins/coverage.js:46).
pub const MUST: u8 = 0;
/// `HOLE` (web/fins/coverage.js:46).
pub const HOLE: u8 = 1;
/// `LOW` (web/fins/coverage.js:46).
pub const LOW: u8 = 2;

/// `HOLE_D` (web/fins/coverage.js:52): a TINY HOLE is a horizontal bore this wide or less.
const HOLE_D: f64 = 6.0;
/// `LOW_Z` (web/fins/coverage.js:56): the NEAR-PLATE strip, under the squat-wall floor.
const LOW_Z: f64 = 0.82;

/// The set `reachOf` builds: wall-top points gridded by plan cell. `reachOf(tops, R)` returns a
/// function with an `add` property in JS; here the set is a value with `near` and `add`.
#[derive(Clone, Debug)]
pub struct Reach {
    r: f64,
    cell: f64,
    grid: FxHashMap<i64, Vec<[f64; 3]>>,
}

/// `key(i, j)` (web/fins/coverage.js:26).
fn cell_key(i: f64, j: f64) -> i64 {
    (i * 100003.0 + j) as i64
}

/// `reachOf(tops, R)` (web/fins/coverage.js:24-44): the wall-top set. `near(x, y, z)` asks: is
/// there a wall-top point within R of (x, y) in plan and 0..1.5 mm under z?
pub fn reach_of(tops: &[[f64; 3]], r: f64) -> Reach {
    let mut reach = Reach {
        r,
        cell: jsnum::max(r, 1.0),
        grid: FxHashMap::default(),
    };
    for p in tops {
        reach.add(*p);
    }
    reach
}

impl Reach {
    /// `near.add(p)`: grows the set (the fill pass adds its walls').
    pub fn add(&mut self, p: [f64; 3]) {
        let k = cell_key(
            jsnum::floor(p[0] / self.cell),
            jsnum::floor(p[1] / self.cell),
        );
        self.grid.entry(k).or_default().push(p);
    }

    /// `near(cx, cy, cz)`.
    pub fn near(&self, cx: f64, cy: f64, cz: f64) -> bool {
        if self.grid.is_empty() {
            return false;
        }
        let gi = jsnum::floor(cx / self.cell);
        let gj = jsnum::floor(cy / self.cell);
        // `hypot(dx, dy) <= r`, decided on dx^2 + dy^2 unless it is within a hair of r^2 (hypot is
        // correct to an ulp or two, the margin is 1e-9): the exact `hypot` settles the rest
        let lim = self.r * self.r;
        let (far, close) = if self.r >= 0.0 && self.r.is_finite() {
            (lim * (1.0 + 1e-9), lim * (1.0 - 1e-9))
        } else {
            (f64::INFINITY, f64::NEG_INFINITY)
        };
        for di in -1..=1 {
            for dj in -1..=1 {
                if let Some(cell) = self.grid.get(&cell_key(gi + di as f64, gj + dj as f64)) {
                    for p in cell {
                        let dz = cz - p[2];
                        if dz >= -0.05 && dz <= 1.5 {
                            let (dx, dy) = (cx - p[0], cy - p[1]);
                            let d2 = dx * dx + dy * dy;
                            if d2 > far {
                                continue;
                            }
                            if d2 < close || jsnum::hypot(&[dx, dy]) <= self.r {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

/// Per overhang face (`res.over`), its class: `MUST`, `HOLE` or `LOW` (indexed by face; 0 for
/// faces that aren't overhang). Faces are grouped into connected patches (topo adjacency), and a
/// patch is a HOLE as a whole; LOW is per face.
///
/// `classify(topo, res, rot)` (web/fins/coverage.js:64-188). JS returns `res.coverageClasses`
/// when the result already carries it: use [`classify_targets`] for that case.
pub fn classify(topo: &Topology, res: &Analysis, rot: &Rot3) -> Vec<u8> {
    let pos = &topo.pos;
    let n_faces = topo.n_faces;
    let (adj_a, adj_b) = (&topo.adj_a, &topo.adj_b);
    let nrm = &topo.nrm;
    let area = &topo.area;
    let off = &res.offset;
    let mut cls = vec![0u8; n_faces];
    let seat = |o: usize| -> [f64; 3] {
        [
            rot[0] * pos[o] + rot[3] * pos[o + 1] + rot[6] * pos[o + 2] + off.x,
            rot[1] * pos[o] + rot[4] * pos[o + 1] + rot[7] * pos[o + 2] + off.y,
            rot[2] * pos[o] + rot[5] * pos[o + 1] + rot[8] * pos[o + 2] + off.z,
        ]
    };
    // connected overhang patches, union-find over shared edges
    let mut parent: Vec<i32> = (0..n_faces as i32).collect();
    fn find(parent: &mut [i32], mut i: i32) -> i32 {
        while parent[i as usize] != i {
            let g = parent[parent[i as usize] as usize];
            parent[i as usize] = g;
            i = g;
        }
        i
    }
    for k in 0..adj_a.len() {
        let (a, b) = (adj_a[k], adj_b[k]);
        if res.over[a as usize] != 0 && res.over[b as usize] != 0 {
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            parent[ra as usize] = rb;
        }
    }
    let mut patches: IndexMap<i32, Vec<usize>> = IndexMap::new();
    for f in 0..n_faces {
        if res.over[f] != 0 {
            let r = find(&mut parent, f as i32);
            patches.entry(r).or_default().push(f);
        }
    }
    // These few hole-recognition probes often land exactly on a CAD tessellation seam. Count a
    // coincident surface once, and cancel opposite-facing tangent hits. Local: automatic exclusion
    // must not change collision predicates.
    let tris_cell: OnceCell<Arc<Tris>> = OnceCell::new();
    let inside = |x: f64, y: f64, z: f64| -> bool {
        let tris = tris_cell.get_or_init(|| seated_part_tris(topo, rot, off));
        let mut hits: Vec<[f64; 2]> = surface_hits_at(tris, x, y)
            .into_iter()
            .filter(|h| h[0] >= z - 1e-9)
            .collect();
        hits.sort_by(|a, b| cmp_diff(a[0] - b[0]));
        let mut crossings = 0u32;
        let mut i = 0;
        while i < hits.len() {
            let height = hits[i][0];
            let mut up = false;
            let mut down = false;
            loop {
                up = up || hits[i][1] > 0.0;
                down = down || hits[i][1] < 0.0;
                i += 1;
                if !(i < hits.len() && jsnum::abs(hits[i][0] - height) < 1e-7) {
                    break;
                }
            }
            if up != down {
                crossings += 1;
            }
        }
        (crossings & 1) == 1
    };
    for faces in patches.values() {
        // plan extent across the patch's minor axis (2D PCA of its vertices)
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let mut top = f64::NEG_INFINITY;
        for &f in faces {
            for i in 0..3 {
                let v = seat(f * 9 + i * 3);
                pts.push(v);
                if v[2] > top {
                    top = v[2];
                }
            }
        }
        let (mut mx, mut my) = (0.0f64, 0.0f64);
        for v in &pts {
            mx += v[0];
            my += v[1];
        }
        mx /= pts.len() as f64;
        my /= pts.len() as f64;
        let (mut sxx, mut sxy, mut syy) = (0.0f64, 0.0f64, 0.0f64);
        for v in &pts {
            let dx = v[0] - mx;
            let dy = v[1] - my;
            sxx += dx * dx;
            sxy += dx * dy;
            syy += dy * dy;
        }
        let tr = sxx + syy;
        let det = sxx * syy - sxy * sxy;
        let lam = tr / 2.0 + jsnum::sqrt(jsnum::max(0.0, (tr * tr) / 4.0 - det));
        let mut ux = sxy;
        let mut uy = lam - sxx;
        if jsnum::hypot(&[ux, uy]) < 1e-9 {
            ux = 1.0;
            uy = 0.0;
        }
        let un = jsnum::hypot(&[ux, uy]);
        ux /= un;
        uy /= un;
        // Across the bore is whichever axis has part on BOTH sides.
        let extent = |ax: f64, ay: f64| -> f64 {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for v in &pts {
                let t = (v[0] - mx) * ax + (v[1] - my) * ay;
                lo = jsnum::min(lo, t);
                hi = jsnum::max(hi, t);
            }
            hi - lo
        };
        // probed from the patch's plan centre at the apex height
        let z = top - 0.5; // just under the apex
        let apex = [mx, my, top];
        // part either side, equally far: a round bore is symmetric about its apex
        let wall_at = |ax: f64, ay: f64, sg: f64| -> Option<f64> {
            let mut d = 0.25;
            while d <= HOLE_D / 2.0 + 1.0 {
                if inside(apex[0] + sg * ax * d, apex[1] + sg * ay * d, z) {
                    return Some(d);
                }
                d += 0.25;
            }
            None
        };
        let walled = |ax: f64, ay: f64| -> bool {
            let a = wall_at(ax, ay, 1.0);
            let b = wall_at(ax, ay, -1.0);
            match (a, b) {
                (Some(a), Some(b)) => jsnum::abs(a - b) <= 0.5,
                _ => false,
            }
        };
        // a CYLINDER: every ceiling face's normal is square to the bore axis
        let square = |ax: f64, ay: f64| -> bool {
            let mut off_sum = 0.0f64;
            let mut tot = 0.0f64;
            for &f in faces {
                let n: Vec<f64> = (0..3)
                    .map(|k| {
                        rot[k] * nrm[f * 3]
                            + rot[3 + k] * nrm[f * 3 + 1]
                            + rot[6 + k] * nrm[f * 3 + 2]
                    })
                    .collect();
                off_sum += area[f] * jsnum::abs(n[0] * -ay + n[1] * ax);
                tot += area[f];
            }
            tot > 0.0 && off_sum / tot < 0.15
        };
        let narrow = |ax: f64, ay: f64| -> bool {
            extent(ax, ay) <= 0.71 * HOLE_D + 0.25 && walled(ax, ay) && square(ax, ay)
        };
        // and a CEILING: some face within ~14deg of straight down
        let mut roof = false;
        for &f in faces {
            if rot[2] * nrm[f * 3] + rot[5] * nrm[f * 3 + 1] + rot[8] * nrm[f * 3 + 2] < -0.97 {
                roof = true;
                break;
            }
        }
        // the probes start at the patch's plan centre, so that must be AIR
        let mut hole = roof && !inside(apex[0], apex[1], z) && (narrow(-uy, ux) || narrow(ux, uy));
        if hole {
            // under the apex: the bore's floor within a diameter
            let mut floor = false;
            let mut d = 0.5;
            while d <= HOLE_D + 1e-9 && !floor {
                floor = inside(apex[0], apex[1], apex[2] - d);
                d += 0.5;
            }
            hole = floor;
        }
        for &f in faces {
            if hole {
                cls[f] = HOLE;
                continue;
            }
            let mut cz = 0.0f64;
            for i in 0..3 {
                cz += seat(f * 9 + i * 3)[2] / 3.0;
            }
            cls[f] = if cz < LOW_Z { LOW } else { MUST };
        }
    }
    cls
}

/// What a JS `sort` comparator returning `diff` means: negative first, positive last, zero or
/// NaN equal.
fn cmp_diff(diff: f64) -> std::cmp::Ordering {
    if diff < 0.0 {
        std::cmp::Ordering::Less
    } else if diff > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// The copy of the analyze result with extra keys that `automaticTargets` returns
/// (web/fins/coverage.js:224-227): `{ ...result, regions, coverageClasses, excludedSupportFaces,
/// excludedSupportTris, excludedSupportVolumes, smallHoleRegions }`. M3's `buildFins` passes
/// `&Targets` where JS passes this augmented result.
#[derive(Clone, Debug)]
pub struct Targets {
    /// `...result`, with `regions` replaced by the kept regions.
    pub analysis: Analysis,
    pub coverage_classes: Vec<u8>,
    pub excluded_support_faces: Vec<u8>,
    /// The seated triangles of the excluded faces (a plain array in JS, used as a soup).
    pub excluded_support_tris: Tris,
    /// Triples (vertices of the reserved prisms).
    pub excluded_support_volumes: Vec<[f64; 3]>,
    pub small_hole_regions: usize,
}

/// `classify(topo, res, rot)` for a result that is already a [`Targets`]: JS returns the
/// `res.coverageClasses` it carries ("reuse it across races and fill").
pub fn classify_targets(res: &Targets) -> Vec<u8> {
    res.coverage_classes.clone()
}

/// Automatic placement targets, without changing the caller's red/highlight mask or removing any
/// geometry from collision/floor probes.
///
/// `automaticTargets(topo, result, rot)` (web/fins/coverage.js:197-228).
pub fn automatic_targets(topo: &Topology, result: &Analysis, rot: &Rot3) -> Targets {
    let coverage_classes = classify(topo, result, rot);
    let excluded_support_faces: Vec<u8> = coverage_classes
        .iter()
        .map(|&c| u8::from(c == HOLE))
        .collect();
    let regions: Vec<Region> = result
        .regions
        .iter()
        .filter(|r| excluded_support_faces[r.faces[0]] == 0)
        .cloned()
        .collect();
    let mut tris_out: Vec<f64> = Vec::new();
    let mut volumes: Vec<[f64; 3]> = Vec::new();
    if excluded_support_faces.iter().any(|&e| e != 0) {
        let tris = seated_part_tris(topo, rot, &result.offset);
        for f in 0..topo.n_faces {
            if excluded_support_faces[f] == 0 {
                continue;
            }
            for k in 0..9 {
                tris_out.push(tris.v[f * 9 + k]);
            }
            let a = [tris.v[f * 9], tris.v[f * 9 + 1], tris.v[f * 9 + 2]];
            let b = [tris.v[f * 9 + 3], tris.v[f * 9 + 4], tris.v[f * 9 + 5]];
            let c = [tris.v[f * 9 + 6], tris.v[f * 9 + 7], tris.v[f * 9 + 8]];
            let x = (a[0] + b[0] + c[0]) / 3.0;
            let y = (a[1] + b[1] + c[1]) / 3.0;
            let z = (a[2] + b[2] + c[2]) / 3.0;
            // The actual upward-facing floor, not another diameter or cavity rule.
            let mut floor = f64::NEG_INFINITY;
            for [h, nz] in surface_hits_at(&tris, x, y) {
                if nz > 0.0 && h < z - 1e-6 {
                    floor = jsnum::max(floor, h);
                }
            }
            if !floor.is_finite() {
                continue;
            }
            let ba = [a[0], a[1], floor];
            let bb = [b[0], b[1], floor];
            let bc = [c[0], c[1], floor];
            volumes.extend([
                a, b, c, ba, bb, bc, a, b, bb, a, bb, ba, b, c, bc, b, bc, bb, c, a, ba, c, ba, bc,
            ]);
        }
    }
    let small_hole_regions = result.regions.len() - regions.len();
    let mut analysis = result.clone();
    analysis.regions = regions;
    Targets {
        analysis,
        coverage_classes,
        excluded_support_faces,
        excluded_support_tris: Tris::new(tris_out),
        excluded_support_volumes: volumes,
        small_hole_regions,
    }
}

/// A fill/row line can cross a ceiling it was not aimed at. Test its settled contact stations
/// against the excluded surfaces.
///
/// `touchesSmallHole(result, line)` (web/fins/coverage.js:234-238).
pub fn touches_small_hole(result: &Targets, line: &[[f64; 3]]) -> bool {
    let tris = &result.excluded_support_tris;
    !tris.is_empty()
        && line.iter().any(|p| {
            surface_hits_at(tris, p[0], p[1])
                .iter()
                .any(|h| jsnum::abs(h[0] - p[2]) <= 0.5)
        })
}
