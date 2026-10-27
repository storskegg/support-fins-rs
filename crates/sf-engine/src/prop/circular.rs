//! Port of `web/prop/circular.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Circular overhang measurements in the seated print frame: recognise only complete,
//! horizontal circular boundary loops with a common centre. An ellipse, open arc, uneven rim or
//! non-circular inner boundary keeps the existing placement path.

use std::sync::{Arc, Mutex};

use indexmap::{IndexMap, IndexSet};

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, Region, Topology};
use crate::prop::surface::surface_z_at;
use crate::prop::sweep::foot_for;
use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `TAU` (web/prop/circular.js:12).
const TAU: f64 = 2.0 * std::f64::consts::PI;

/// `key(p)` (web/prop/circular.js:14): `p.map((x) => Math.round(x * 1e5)).join(',')`.
fn key(p: &[f64; 3]) -> String {
    p.iter()
        .map(|&x| jsnum::to_js_string(jsnum::round(x * 1e5)))
        .collect::<Vec<_>>()
        .join(",")
}

/// `angle(a)` (web/prop/circular.js:15): `((a % TAU) + TAU) % TAU`.
fn angle(a: f64) -> f64 {
    ((a % TAU) + TAU) % TAU
}

/// What `circle(points)` fits (web/prop/circular.js:17-42): `{ centre, radius, z, tolerance }`.
#[derive(Clone, Copy, Debug)]
struct Circle {
    centre: [f64; 2],
    radius: f64,
    z: f64,
    tolerance: f64,
}

/// `circle(points)` (web/prop/circular.js:17-42).
fn circle(points: &[[f64; 3]]) -> Option<Circle> {
    if points.len() < 12 {
        return None;
    }
    let origin = points[0];
    let (mut mx, mut my, mut z) = (0.0f64, 0.0f64, 0.0f64);
    for p in points {
        mx += p[0] - origin[0];
        my += p[1] - origin[1];
        z += p[2];
    }
    let n = points.len() as f64;
    mx /= n;
    my /= n;
    z /= n;
    let (mut xx, mut xy, mut yy, mut xr, mut yr) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for p in points {
        let x = p[0] - origin[0] - mx;
        let y = p[1] - origin[1] - my;
        let r2 = x * x + y * y;
        xx += x * x;
        xy += x * y;
        yy += y * y;
        xr += x * r2;
        yr += y * r2;
    }
    let det = xx * yy - xy * xy;
    if det <= 1e-12 {
        return None;
    }
    let cx = origin[0] + mx + (yy * xr - xy * yr) / (2.0 * det);
    let cy = origin[1] + my + (xx * yr - xy * xr) / (2.0 * det);
    let radii: Vec<f64> = points
        .iter()
        .map(|p| jsnum::hypot(&[p[0] - cx, p[1] - cy]))
        .collect();
    let mut sum = 0.0f64;
    for r in &radii {
        sum += r;
    }
    let radius = sum / radii.len() as f64;
    let tolerance = jsnum::max(0.002, radius * 1e-4);
    if radius < 1.0
        || radii.iter().any(|r| jsnum::abs(r - radius) > tolerance)
        || points.iter().any(|p| jsnum::abs(p[2] - z) > tolerance)
    {
        return None;
    }
    let mut angles: Vec<f64> = points
        .iter()
        .map(|p| angle(jsnum::atan2(p[1] - cy, p[0] - cx)))
        .collect();
    angles.sort_by(|a, b| cmp_diff(a - b));
    let mut max_gap = 0.0f64;
    for i in 0..angles.len() {
        max_gap = jsnum::max(max_gap, angle(angles[(i + 1) % angles.len()] - angles[i]));
    }
    if max_gap > std::f64::consts::PI / 6.0 + 1e-6 {
        return None;
    }
    Some(Circle {
        centre: [cx, cy],
        radius,
        z,
        tolerance,
    })
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

/// `surroundingGeometry`'s result (web/prop/circular.js:80-81):
/// `{ ceiling, reach, area, centroid, phase }` (`phase` is `null` when the centroid is within 1 um
/// of the centre).
#[derive(Clone, Debug, PartialEq)]
pub struct Surrounding {
    pub ceiling: f64,
    pub reach: f64,
    pub area: f64,
    pub centroid: [f64; 2],
    pub phase: Option<f64>,
}

/// A recognised circular footprint: the frame `circularFootprint` returns, plus the
/// `surrounding` key `circularRegions` adds (web/prop/circular.js:173-178).
#[derive(Clone, Debug, PartialEq)]
pub struct CircularFrame {
    pub centre: [f64; 2],
    pub inner_radius: f64,
    pub outer_radius: f64,
    /// `[[innerRadius, innerZ], [outerRadius, outer.z]]`
    pub radial_profile: [[f64; 2]; 2],
    pub angular_extent: [f64; 2],
    pub phase: f64,
    pub axis: [f64; 2],
    /// `None` until `circularRegions` adds it.
    pub surrounding: Option<Surrounding>,
}

/// `surroundingGeometry(topo, rot, off, frame)` (web/prop/circular.js:48-82): nearby geometry
/// below the ceiling can break the circle's rotational symmetry; its surface-area centroid fixes
/// the candidate phase.
fn surrounding_geometry(
    topo: &Topology,
    rot: &Rot3,
    off: &Xyz,
    frame: &CircularFrame,
    cfg: &Config,
) -> Surrounding {
    let ceiling = jsnum::max_of(&[frame.radial_profile[0][1], frame.radial_profile[1][1]]);
    let reach = frame.outer_radius + cfg.prop.foot_max + cfg.prop.side_clear;
    let mut area = 0.0f64;
    let mut mx = 0.0f64;
    let mut my = 0.0f64;
    let mut f = 0;
    while f < topo.pos.len() {
        let mut points: Vec<[f64; 3]> = Vec::new();
        let mut j = 0;
        while j < 9 {
            let (x, y, z) = (topo.pos[f + j], topo.pos[f + j + 1], topo.pos[f + j + 2]);
            points.push([
                rot[0] * x + rot[3] * y + rot[6] * z + off.x - frame.centre[0],
                rot[1] * x + rot[4] * y + rot[7] * z + off.y - frame.centre[1],
                rot[2] * x + rot[5] * y + rot[8] * z + off.z,
            ]);
            j += 3;
        }
        let mut clipped: Vec<[f64; 3]> = Vec::new();
        for i in 0..3 {
            let a = points[i];
            let b = points[(i + 1) % 3];
            if a[2] <= ceiling {
                clipped.push(a);
            }
            if (a[2] < ceiling && b[2] > ceiling) || (b[2] < ceiling && a[2] > ceiling) {
                let t = (ceiling - a[2]) / (b[2] - a[2]);
                clipped.push([
                    a[0] + t * (b[0] - a[0]),
                    a[1] + t * (b[1] - a[1]),
                    a[2] + t * (b[2] - a[2]),
                ]);
            }
        }
        let mut i = 1;
        while i + 1 < clipped.len() {
            let (a, b, c) = (clipped[0], clipped[i], clipped[i + 1]);
            let x = (a[0] + b[0] + c[0]) / 3.0;
            let y = (a[1] + b[1] + c[1]) / 3.0;
            i += 1;
            if jsnum::hypot(&[x, y]) > reach {
                continue;
            }
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let weight = jsnum::hypot(&[
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]) / 2.0;
            area += weight;
            mx += x * weight;
            my += y * weight;
        }
        f += 9;
    }
    let centroid = if area != 0.0 {
        [mx / area, my / area]
    } else {
        [0.0, 0.0]
    };
    let phase = if jsnum::hypot(&centroid) > 0.001 {
        Some(jsnum::atan2(centroid[1], centroid[0]))
    } else {
        None
    };
    Surrounding {
        ceiling,
        reach,
        area,
        centroid,
        phase,
    }
}

/// A fitted rim loop: `{ ...fitted, points }`.
struct Loop {
    fitted: Circle,
    points: Vec<[f64; 3]>,
}

/// A region's complete circular loops, outer first; `None` (JS `null`) means use the old path.
///
/// `circularFootprint(tris)` (web/prop/circular.js:85-153). `tris` is the region's seated
/// triangle soup (a `Float64Array`: 9 values per face).
pub fn circular_footprint(tris: &[f64]) -> Option<CircularFrame> {
    let mut vertices: IndexMap<String, [f64; 3]> = IndexMap::new();
    struct Edge {
        a: String,
        b: String,
        n: usize,
    }
    let mut edges: IndexMap<String, Edge> = IndexMap::new();
    let mut i = 0;
    while i < tris.len() {
        let mut ids: Vec<String> = Vec::new();
        let mut j = 0;
        while j < 9 {
            let p = [tris[i + j], tris[i + j + 1], tris[i + j + 2]];
            let id = key(&p);
            vertices.insert(id.clone(), p);
            ids.push(id);
            j += 3;
        }
        for j in 0..3 {
            let a = &ids[j];
            let b = &ids[(j + 1) % 3];
            if a == b {
                return None;
            }
            let k = if a < b {
                format!("{a}|{b}")
            } else {
                format!("{b}|{a}")
            };
            if let Some(edge) = edges.get_mut(&k) {
                edge.n += 1;
            } else {
                edges.insert(
                    k,
                    Edge {
                        a: a.clone(),
                        b: b.clone(),
                        n: 1,
                    },
                );
            }
        }
        i += 9;
    }
    let mut adjacency: IndexMap<String, Vec<String>> = IndexMap::new();
    for edge in edges.values() {
        if edge.n > 2 {
            return None;
        }
        if edge.n != 1 {
            continue;
        }
        for (p, q) in [(&edge.a, &edge.b), (&edge.b, &edge.a)] {
            adjacency.entry(p.clone()).or_default().push(q.clone());
        }
    }
    if adjacency.is_empty() || adjacency.values().any(|ns| ns.len() != 2) {
        return None;
    }
    let mut remaining: IndexSet<String> = adjacency.keys().cloned().collect();
    let mut loops: Vec<Loop> = Vec::new();
    while !remaining.is_empty() {
        let start = remaining.first().unwrap().clone();
        let mut ids: Vec<String> = Vec::new();
        let mut current = start.clone();
        let mut previous: Option<String> = None;
        loop {
            if !remaining.shift_remove(&current) {
                return None;
            }
            ids.push(current.clone());
            let next = adjacency[&current]
                .iter()
                .find(|n| Some(*n) != previous.as_ref())
                .expect("a boundary vertex has two distinct neighbours")
                .clone();
            previous = Some(current);
            current = next;
            if current == start {
                break;
            }
        }
        let points: Vec<[f64; 3]> = ids.iter().map(|id| vertices[id]).collect();
        let fitted = circle(&points)?;
        loops.push(Loop { fitted, points });
    }
    if loops.len() > 2 {
        return None;
    }
    loops.sort_by(|a, b| cmp_diff(b.fitted.radius - a.fitted.radius));
    let outer = &loops[0];
    let [cx, cy] = outer.fitted.centre;
    if loops.iter().any(|l| {
        jsnum::hypot(&[l.fitted.centre[0] - cx, l.fitted.centre[1] - cy]) > outer.fitted.tolerance
    }) {
        return None;
    }
    let inner_radius = loops.get(1).map_or(0.0, |l| l.fitted.radius);
    // For now a flat disk/annulus or straight radial slope.
    let inner_z = loops.get(1).map_or(outer.fitted.z, |l| l.fitted.z);
    let slope = (outer.fitted.z - inner_z) / (outer.fitted.radius - inner_radius);
    if outer.fitted.radius - inner_radius < 0.1 {
        return None;
    }
    for p in vertices.values() {
        let r = jsnum::hypot(&[p[0] - cx, p[1] - cy]);
        if r < inner_radius - outer.fitted.tolerance
            || r > outer.fitted.radius + outer.fitted.tolerance
            || jsnum::abs(p[2] - inner_z - slope * (r - inner_radius))
                > 2.0 * outer.fitted.tolerance
        {
            return None;
        }
    }
    // The longest rim chord is a geometric landmark on an uneven tessellation.
    let n = outer.points.len();
    let landmarks: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let p = outer.points[i];
            let q = outer.points[(i + 1) % n];
            (
                jsnum::hypot(&[p[0] - q[0], p[1] - q[1]]),
                angle(jsnum::atan2(
                    (p[1] + q[1]) / 2.0 - cy,
                    (p[0] + q[0]) / 2.0 - cx,
                )),
            )
        })
        .collect();
    let mut longest = 0.0f64;
    for l in &landmarks {
        longest = jsnum::max(longest, l.0);
    }
    let mut phase = f64::INFINITY;
    for l in &landmarks {
        if longest - l.0 < outer.fitted.tolerance {
            phase = jsnum::min(phase, l.1);
        }
    }
    Some(CircularFrame {
        centre: [cx, cy],
        inner_radius,
        outer_radius: outer.fitted.radius,
        radial_profile: [
            [inner_radius, inner_z],
            [outer.fitted.radius, outer.fitted.z],
        ],
        angular_extent: [0.0, TAU],
        phase,
        axis: [jsnum::cos(phase), jsnum::sin(phase)],
        surrounding: None,
    })
}

/// What `regionCache` keeps for one analyze result: JS keys it on the result object and checks
/// `topo`, `rot`, `footMax` and `sideClear`. Here the caller owns it (one per build), and a hit
/// requires the same analysis *content* (offset and regions), the same topology, rotation and
/// the two config values, so a hit returns exactly what a recompute would.
struct CacheEntry {
    topo: usize,
    n_faces: usize,
    vertex_count: usize,
    offset: Xyz,
    regions: Vec<Region>,
    rot: Rot3,
    foot_max: f64,
    side_clear: f64,
    frames: Arc<IndexMap<usize, CircularFrame>>,
}

/// The caller-owned stand-in for `regionCache` (a `WeakMap` keyed on the analyze result).
/// `buildProps` owns one per build.
#[derive(Default)]
pub struct CircularCache {
    entry: Mutex<Option<CacheEntry>>,
}

impl std::fmt::Debug for CircularCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CircularCache")
    }
}

impl CircularCache {
    pub fn new() -> CircularCache {
        CircularCache::default()
    }
}

/// Collect once for the normal/raster/downhill races: region index -> circular frame.
///
/// `circularRegions(topo, result, rot)` (web/prop/circular.js:156-183), with the `regionCache`
/// as the caller-owned `cache`.
pub fn circular_regions(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    cache: &CircularCache,
    cfg: &Config,
) -> Arc<IndexMap<usize, CircularFrame>> {
    let topo_id = topo as *const Topology as usize;
    {
        let slot = cache.entry.lock().unwrap();
        if let Some(c) = slot.as_ref()
            && c.topo == topo_id
            && c.n_faces == topo.n_faces
            && c.vertex_count == topo.vertex_count
            && c.offset == result.offset
            && c.regions == result.regions
            && c.rot.iter().zip(rot.iter()).all(|(a, b)| a == b)
            && c.foot_max == cfg.prop.foot_max
            && c.side_clear == cfg.prop.side_clear
        {
            return c.frames.clone();
        }
    }
    let mut frames: IndexMap<usize, CircularFrame> = IndexMap::new();
    let pos = &topo.pos;
    let off = &result.offset;
    for (ri, r) in result.regions.iter().enumerate() {
        if r.faces.len() < 12 {
            continue;
        }
        let mut tris = vec![0.0f64; r.faces.len() * 9];
        let mut at = 0;
        for &f in &r.faces {
            let mut i = 0;
            while i < 9 {
                let o = f * 9 + i;
                let (x, y, z) = (pos[o], pos[o + 1], pos[o + 2]);
                tris[at] = rot[0] * x + rot[3] * y + rot[6] * z + off.x;
                tris[at + 1] = rot[1] * x + rot[4] * y + rot[7] * z + off.y;
                tris[at + 2] = rot[2] * x + rot[5] * y + rot[8] * z + off.z;
                at += 3;
                i += 3;
            }
        }
        if let Some(mut frame) = circular_footprint(&tris) {
            let s = surrounding_geometry(topo, rot, off, &frame, cfg);
            if let Some(ph) = s.phase {
                frame.phase = angle(ph);
                frame.axis = [jsnum::cos(frame.phase), jsnum::sin(frame.phase)];
            }
            frame.surrounding = Some(s);
            frames.insert(ri, frame);
        }
    }
    let frames = Arc::new(frames);
    *cache.entry.lock().unwrap() = Some(CacheEntry {
        topo: topo_id,
        n_faces: topo.n_faces,
        vertex_count: topo.vertex_count,
        offset: result.offset,
        regions: result.regions.clone(),
        rot: *rot,
        foot_max: cfg.prop.foot_max,
        side_clear: cfg.prop.side_clear,
        frames: frames.clone(),
    });
    frames
}

/// Radial candidates for an annular slope. Counts come from the requested circumferential
/// pitch and the space available at the inner ends' feet. `None` = JS `null`.
///
/// `circularTracks(frame, tris, step, span)` (web/prop/circular.js:190-220).
pub fn circular_tracks(
    frame: &CircularFrame,
    tris: &Tris,
    step: f64,
    span: f64,
    cfg: &Config,
) -> Option<Vec<Vec<[f64; 3]>>> {
    let prop = &cfg.prop;
    let ri = frame.inner_radius;
    let ro = frame.outer_radius;
    let profile = &frame.radial_profile;
    let centre = frame.centre;
    let phase = frame.phase;
    if ri < 1.0 || jsnum::abs(profile[0][1] - profile[1][1]) < 0.01 {
        return None;
    }
    let height = jsnum::max_of(&[profile[0][1], profile[1][1]]) - prop.gap;
    let clearance = 2.0 * foot_for(height, cfg) + prop.side_clear;
    let max_count = if clearance > 2.0 * ri {
        1.0
    } else {
        jsnum::floor(std::f64::consts::PI / jsnum::asin(clearance / (2.0 * ri)))
    };
    let radius = jsnum::sqrt((ri * ri + ro * ro) / 2.0);
    // Keep the outer edge within the existing unsupported-span cap, then let the coverage
    // setting request more. Physical foot clearance still caps the count.
    let requested = jsnum::max(TAU * ro / prop.max_unsupported_span, TAU * radius / span);
    let count = jsnum::max(1.0, jsnum::min(max_count, jsnum::ceil(requested - 1e-6))) as usize;
    let from = ri + prop.edge_inset;
    let to = ro - prop.edge_inset;
    if to - from < prop.min_span {
        return None;
    }
    // Half steps keep a sloping wall's body from losing a full station at the minHeight boundary.
    let stations = jsnum::max(
        prop.min_stations - 1.0,
        jsnum::ceil((to - from) / (step / 2.0) - 1e-6),
    ) as usize;
    let mut lines: Vec<Vec<[f64; 3]>> = Vec::new();
    for i in 0..count {
        let a = phase + (TAU * i as f64) / count as f64;
        let ux = jsnum::cos(a);
        let uy = jsnum::sin(a);
        let mut line: Vec<[f64; 3]> = Vec::new();
        for j in 0..=stations {
            let r = from + ((to - from) * j as f64) / stations as f64;
            let x = centre[0] + r * ux;
            let y = centre[1] + r * uy;
            match surface_z_at(tris, x, y) {
                None => {
                    if line.len() as f64 >= prop.min_stations {
                        lines.push(line.clone());
                    }
                    line.clear();
                }
                Some(z) => line.push([x, y, z]),
            }
        }
        if line.len() as f64 >= prop.min_stations {
            lines.push(line);
        }
    }
    Some(lines)
}
