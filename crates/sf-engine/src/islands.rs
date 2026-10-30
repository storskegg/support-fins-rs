//! Port of `web/islands.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Island starts (local issue 041): a feature whose first layer prints in midair while every
//! face around its low point is steeper than the overhang angle. Found on the mesh, not by
//! slicing. In print space: the part posed by `rot` and seated by `res.offset`.

use crate::guards::JsSort;
use std::collections::HashMap;

use indexmap::{IndexMap, IndexSet};

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology, find};
use crate::types::Rot3;

/// `ISLAND` (web/islands.js:24). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct IslandConfig {
    // mm; one layer: the band below a start a slicer prints it onto
    pub layer: f64,
    // mm in plan; material this close in the layer below holds the start
    pub grace: f64,
    /// JS key `minArea`.
    // mm^2; cross-section one layer up (Matthew 2026-10-17: hold anything real)
    pub min_area: f64,
    /// JS key `plateZ`.
    // mm; a start this low is on the plate (coverage.js PLATE_Z)
    pub plate_z: f64,
    // mm in plan; starts this close are one island (both ends of a boss rim)
    pub merge: f64,
    /// JS key `mergeZ`.
    // mm; ... and within a wall top's reach band of each other
    pub merge_z: f64,
    /// JS key `maxFaces`.
    // faces walked per start for its cross-section; past it, it's big
    pub max_faces: f64,
}

impl Default for IslandConfig {
    fn default() -> Self {
        IslandConfig {
            layer: 0.2,
            grace: 0.3,
            min_area: 0.03,
            plate_z: 0.6,
            merge: 2.0,
            merge_z: 1.5,
            max_faces: 4000.0,
        }
    }
}

/// `EPS` (web/islands.js:34).
const EPS: f64 = 1e-6;

/// `topo._vgraph` (web/islands.js:57), cached per topology: vertex neighbours and incident
/// faces (CSR), rotation-invariant.
#[derive(Debug)]
pub struct VGraph {
    n_v: usize,
    /// One face corner per vertex, for its position.
    corner: Vec<i32>,
    f_start: Vec<i32>,
    v_faces: Vec<i32>,
    nbr: Vec<i32>,
}

/// `vertexGraph(topo)` (web/islands.js:37-58).
fn vertex_graph(topo: &Topology) -> &VGraph {
    topo.cache.vgraph.get_or_init(|| {
        let vid = &topo.vid;
        let n_faces = topo.n_faces;
        let n_v = topo.vertex_count;
        let mut corner = vec![-1i32; n_v];
        let mut f_deg = vec![0i32; n_v + 1];
        for c in 0..n_faces * 3 {
            f_deg[vid[c] as usize + 1] += 1;
            if corner[vid[c] as usize] < 0 {
                corner[vid[c] as usize] = c as i32;
            }
        }
        for v in 0..n_v {
            f_deg[v + 1] += f_deg[v];
        }
        let mut f_at = f_deg.clone();
        let mut v_faces = vec![0i32; n_faces * 3];
        for c in 0..n_faces * 3 {
            let v = vid[c] as usize;
            v_faces[f_at[v] as usize] = (c / 3) as i32;
            f_at[v] += 1;
        }
        // neighbours: each face gives each corner its two others; duplicates are harmless
        let mut nbr = vec![0i32; n_faces * 6];
        let mut n_at = vec![0i32; n_v];
        for v in 0..n_v {
            n_at[v] = f_deg[v] * 2;
        }
        for f in 0..n_faces {
            let (a, b, c) = (vid[f * 3], vid[f * 3 + 1], vid[f * 3 + 2]);
            for (from, x, y) in [(a, b, c), (b, a, c), (c, a, b)] {
                let at = &mut n_at[from as usize];
                nbr[*at as usize] = x;
                *at += 1;
                nbr[*at as usize] = y;
                *at += 1;
            }
        }
        VGraph {
            n_v,
            corner,
            f_start: f_deg,
            v_faces,
            nbr,
        }
    })
}

/// One island: the object `findIslands` returns (web/islands.js:260, 265-276, keys in JS order: `z`,
/// `area`, `points`, then `x`, `y` added by the final pass).
#[derive(Clone, Debug, PartialEq)]
pub struct Island {
    pub z: f64,
    pub area: f64,
    pub points: Vec<[f64; 3]>,
    pub x: f64,
    pub y: f64,
}

/// The `opts` object of `findIslands` (`P = { ...ISLAND, ...opts }`): any key given overrides
/// the `cfg.island` value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IslandOpts {
    pub layer: Option<f64>,
    pub grace: Option<f64>,
    pub min_area: Option<f64>,
    pub plate_z: Option<f64>,
    pub merge: Option<f64>,
    pub merge_z: Option<f64>,
    pub max_faces: Option<f64>,
}

impl IslandOpts {
    /// `{ ...ISLAND, ...opts }`
    fn over(&self, base: &IslandConfig) -> IslandConfig {
        IslandConfig {
            layer: self.layer.unwrap_or(base.layer),
            grace: self.grace.unwrap_or(base.grace),
            min_area: self.min_area.unwrap_or(base.min_area),
            plate_z: self.plate_z.unwrap_or(base.plate_z),
            merge: self.merge.unwrap_or(base.merge),
            merge_z: self.merge_z.unwrap_or(base.merge_z),
            max_faces: self.max_faces.unwrap_or(base.max_faces),
        }
    }
}

/// A candidate: a run of low vertices (`run.verts`), its height `z0` and the faces around it.
struct Cand {
    verts: Vec<usize>,
    z0: f64,
    star: IndexSet<usize>,
}

/// The island starts of the part in pose `rot` (seated by `res.offset`), lowest first.
///
/// `findIslands(topo, res, rot, opts = {})` (web/islands.js:67-278). `opts` default `{}` is
/// `IslandOpts::default()`; the base values are `cfg.island`.
pub fn find_islands(
    topo: &Topology,
    res: &Analysis,
    rot: &Rot3,
    opts: &IslandOpts,
    cfg: &Config,
) -> Vec<Island> {
    let p = opts.over(&cfg.island);
    let pos = &topo.pos;
    let n_faces = topo.n_faces;
    let nrm = &topo.nrm;
    let area = &topo.area;
    let vid = &topo.vid;
    let g = vertex_graph(topo);
    let (n_v, corner, f_start, v_faces, nbr) = (g.n_v, &g.corner, &g.f_start, &g.v_faces, &g.nbr);
    let off = &res.offset;

    let mut xs = vec![0.0f64; n_v];
    let mut ys = vec![0.0f64; n_v];
    let mut zs_ = vec![0.0f64; n_v];
    for v in 0..n_v {
        let o = corner[v] as usize * 3;
        let (x, y, z) = (pos[o], pos[o + 1], pos[o + 2]);
        xs[v] = rot[0] * x + rot[3] * y + rot[6] * z + off.x;
        ys[v] = rot[1] * x + rot[4] * y + rot[7] * z + off.y;
        zs_[v] = rot[2] * x + rot[5] * y + rot[8] * z + off.z;
    }
    let (xx, yy, zz) = (&xs, &ys, &zs_);

    // low points: no neighbour lower; a level run of them is one (union-find)
    let mut lower = vec![0u8; n_v];
    let mut parent: Vec<i32> = (0..n_v as i32).collect();
    for v in 0..n_v {
        if zz[v] < p.plate_z {
            lower[v] = 1;
            continue;
        }
        let zv = zz[v];
        let mut k = f_start[v] as usize * 2;
        let e = f_start[v + 1] as usize * 2;
        while k < e {
            let u = nbr[k] as usize;
            let dz = zz[u] - zv;
            if dz < -EPS {
                lower[v] = 1;
                break;
            }
            if dz <= EPS {
                let ru = find(&mut parent, u as i32);
                let rv = find(&mut parent, v as i32);
                if ru != rv {
                    parent[ru as usize] = rv;
                }
            }
            k += 1;
        }
    }
    // a run is lower if any vertex in it is; gather only the runs that aren't
    let mut root_lower = vec![0u8; n_v];
    for v in 0..n_v {
        if lower[v] != 0 {
            let r = find(&mut parent, v as i32);
            root_lower[r as usize] = 1;
        }
    }
    let mut runs: IndexMap<i32, Vec<usize>> = IndexMap::new();
    for v in 0..n_v {
        let r = find(&mut parent, v as i32);
        if root_lower[r as usize] != 0 {
            continue;
        }
        runs.entry(r).or_default().push(v);
    }

    let fz = |f: usize| -> [f64; 3] {
        [
            zz[vid[f * 3] as usize],
            zz[vid[f * 3 + 1] as usize],
            zz[vid[f * 3 + 2] as usize],
        ]
    };
    let nz_of = |f: usize| rot[2] * nrm[f * 3] + rot[5] * nrm[f * 3 + 1] + rot[8] * nrm[f * 3 + 2];

    let mut cands: Vec<Cand> = Vec::new();
    for (_, verts) in runs {
        let z0 = zz[verts[0]];
        let mut star: IndexSet<usize> = IndexSet::new();
        for &v in &verts {
            for k in f_start[v]..f_start[v + 1] {
                star.insert(v_faces[k as usize] as usize);
            }
        }
        let mut red = false;
        let mut down = 0.0f64;
        // red that gets a fin (a region, res.kept) already holds it; an amber sliver doesn't
        for &f in &star {
            if res.kept[f] != 0 {
                red = true;
            }
            down += area[f] * nz_of(f);
        }
        if red || down >= 0.0 {
            continue;
        }
        cands.push(Cand { verts, z0, star });
    }
    if cands.is_empty() {
        return Vec::new();
    }

    // the layer below, gridded in plan -- only the cells a start's grace reaches
    const CELL: f64 = 1.0;
    let key = |i: i64, j: i64| i * 1000003 + j;
    let mut grid: HashMap<i64, Vec<usize>> = HashMap::new();
    let r = jsnum::ceil(p.grace / CELL) as i64;
    let mut z_lo = f64::INFINITY;
    let mut z_hi = f64::NEG_INFINITY;
    for c in &cands {
        z_lo = jsnum::min(z_lo, c.z0 - p.layer);
        z_hi = jsnum::max(z_hi, c.z0);
        for &v in &c.verts {
            let gi = jsnum::floor(xx[v] / CELL) as i64;
            let gj = jsnum::floor(yy[v] / CELL) as i64;
            for i in gi - r..=gi + r {
                for j in gj - r..=gj + r {
                    grid.entry(key(i, j)).or_default();
                }
            }
        }
    }
    let mut big: Vec<usize> = Vec::new();
    for f in 0..n_faces {
        let (a, b, c) = (
            vid[f * 3] as usize,
            vid[f * 3 + 1] as usize,
            vid[f * 3 + 2] as usize,
        );
        if jsnum::min_of(&[zz[a], zz[b], zz[c]]) >= z_hi
            || jsnum::max_of(&[zz[a], zz[b], zz[c]]) <= z_lo
        {
            continue;
        }
        let i0 = jsnum::floor(jsnum::min_of(&[xx[a], xx[b], xx[c]]) / CELL) as i64;
        let i1 = jsnum::floor(jsnum::max_of(&[xx[a], xx[b], xx[c]]) / CELL) as i64;
        let j0 = jsnum::floor(jsnum::min_of(&[yy[a], yy[b], yy[c]]) / CELL) as i64;
        let j1 = jsnum::floor(jsnum::max_of(&[yy[a], yy[b], yy[c]]) / CELL) as i64;
        if (i1 - i0 + 1) * (j1 - j0 + 1) > 256 {
            big.push(f);
            continue;
        }
        for i in i0..=i1 {
            for j in j0..=j1 {
                if let Some(cell) = grid.get_mut(&key(i, j)) {
                    cell.push(f);
                }
            }
        }
    }
    // face f clipped to z in [lo, hi], projected: within `r` of (px, py)?
    let near = |f: usize, px: f64, py: f64, lo: f64, hi: f64, rad: f64| -> bool {
        let mut poly: Vec<[f64; 3]> = (0..3)
            .map(|i| {
                let v = vid[f * 3 + i] as usize;
                [xx[v], yy[v], zz[v]]
            })
            .collect();
        for (lim, sg) in [(lo, 1.0f64), (hi, -1.0f64)] {
            let mut out: Vec<[f64; 3]> = Vec::new();
            for i in 0..poly.len() {
                let pp = poly[i];
                let q = poly[(i + 1) % poly.len()];
                let dp = sg * (pp[2] - lim);
                let dq = sg * (q[2] - lim);
                if dp >= 0.0 {
                    out.push(pp);
                }
                if (dp >= 0.0) != (dq >= 0.0) {
                    let t = dp / (dp - dq);
                    out.push([pp[0] + t * (q[0] - pp[0]), pp[1] + t * (q[1] - pp[1]), lim]);
                }
            }
            poly = out;
            if poly.is_empty() {
                return false;
            }
        }
        // point in the projected polygon, or within r of an edge
        let mut inside = false;
        let mut d2 = f64::INFINITY;
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let (xi, yi) = (poly[i][0], poly[i][1]);
            let (xj, yj) = (poly[j][0], poly[j][1]);
            if (yi > py) != (yj > py) && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi {
                inside = !inside;
            }
            let ex = xj - xi;
            let ey = yj - yi;
            let l = ex * ex + ey * ey;
            let t = if l > 0.0 {
                jsnum::max(0.0, jsnum::min(1.0, ((px - xi) * ex + (py - yi) * ey) / l))
            } else {
                0.0
            };
            d2 = jsnum::min(
                d2,
                jsnum::pow(xi + t * ex - px, 2.0) + jsnum::pow(yi + t * ey - py, 2.0),
            );
            j = i;
        }
        inside || d2 <= rad * rad
    };
    let held_below = |px: f64, py: f64, z0: f64| -> bool {
        let lo = z0 - p.layer;
        let hi = z0 - EPS;
        let test = |f: usize| -> bool {
            let [a, b, c] = fz(f);
            jsnum::min_of(&[a, b, c]) < hi
                && jsnum::max_of(&[a, b, c]) > lo
                && near(f, px, py, lo, hi, p.grace)
        };
        let gi = jsnum::floor(px / CELL) as i64;
        let gj = jsnum::floor(py / CELL) as i64;
        for i in gi - r..=gi + r {
            for j in gj - r..=gj + r {
                if let Some(cell) = grid.get(&key(i, j)) {
                    for &f in cell {
                        if test(f) {
                            return true;
                        }
                    }
                }
            }
        }
        for &f in &big {
            if test(f) {
                return true;
            }
        }
        false
    };

    // the cross-section one layer up: walk the faces under that height out from the start;
    // reaching a vertex lower than the start means it joins lower material within the layer
    // (not an island). Past maxFaces the start is big, but the walk goes on (to 10x).
    let section = |c: &Cand| -> f64 {
        let zh = c.z0 + p.layer;
        let mut seen: IndexSet<usize> = c.star.clone();
        let mut queue: Vec<usize> = c.star.iter().copied().collect();
        let mut a2 = 0.0f64;
        let mut qi = 0;
        while qi < queue.len() {
            if queue.len() as f64 > 10.0 * p.max_faces {
                return f64::INFINITY;
            }
            let f = queue[qi];
            qi += 1;
            let zs = fz(f);
            if jsnum::min_of(&zs) < c.z0 - EPS {
                return -1.0;
            }
            if jsnum::max_of(&zs) > zh {
                let mut pts: Vec<[f64; 2]> = Vec::new();
                for i in 0..3 {
                    let u = vid[f * 3 + i] as usize;
                    let w = vid[f * 3 + (i + 1) % 3] as usize;
                    if (zz[u] - zh) * (zz[w] - zh) < 0.0 || (zz[u] == zh && zz[w] != zh) {
                        let t = (zh - zz[u]) / (zz[w] - zz[u]);
                        pts.push([xx[u] + t * (xx[w] - xx[u]), yy[u] + t * (yy[w] - yy[u])]);
                    }
                }
                if pts.len() == 2 {
                    // the outward normal's plan part on the right of the segment p -> q
                    let nx =
                        rot[0] * nrm[f * 3] + rot[3] * nrm[f * 3 + 1] + rot[6] * nrm[f * 3 + 2];
                    let ny =
                        rot[1] * nrm[f * 3] + rot[4] * nrm[f * 3 + 1] + rot[7] * nrm[f * 3 + 2];
                    let (mut pa, mut qa) = (pts[0], pts[1]);
                    if (qa[0] - pa[0]) * ny - (qa[1] - pa[1]) * nx > 0.0 {
                        std::mem::swap(&mut pa, &mut qa);
                    }
                    a2 += pa[0] * qa[1] - qa[0] * pa[1];
                }
            }
            if jsnum::min_of(&zs) >= zh {
                continue;
            }
            for i in 0..3 {
                let v = vid[f * 3 + i] as usize;
                if zz[v] >= zh {
                    continue;
                }
                for k in f_start[v]..f_start[v + 1] {
                    let h = v_faces[k as usize] as usize;
                    if seen.insert(h) {
                        queue.push(h);
                    }
                }
            }
        }
        if queue.len() as f64 > p.max_faces {
            f64::INFINITY
        } else {
            jsnum::abs(a2) / 2.0
        }
    };

    // a long level edge can be thousands of vertices: keep its two ends in plan and an even
    // sample, so the merge and the UI's farthest pair stay small
    const MAXPTS: usize = 32;
    let spread = |vs: &[usize]| -> Vec<usize> {
        if vs.len() <= MAXPTS {
            return vs.to_vec();
        }
        let mut a = vs[0];
        let mut b = vs[0];
        let mut far = -1.0f64;
        for &v in vs {
            let d = jsnum::hypot(&[xx[v] - xx[vs[0]], yy[v] - yy[vs[0]]]);
            if d > far {
                far = d;
                a = v;
            }
        }
        far = -1.0;
        for &v in vs {
            let d = jsnum::hypot(&[xx[v] - xx[a], yy[v] - yy[a]]);
            if d > far {
                far = d;
                b = v;
            }
        }
        let mut out = vec![a, b];
        for i in 0..MAXPTS - 2 {
            out.push(vs[jsnum::floor(((i * vs.len()) as f64) / ((MAXPTS - 2) as f64)) as usize]);
        }
        out
    };
    let mut found: Vec<Island> = Vec::new();
    for c in &cands {
        let vs = &c.verts;
        let mut held = false;
        for &v in vs {
            if held_below(xx[v], yy[v], c.z0) {
                held = true;
                break;
            }
        }
        if held {
            continue;
        }
        let a = section(c);
        if a < p.min_area {
            continue;
        }
        found.push(Island {
            z: c.z0,
            area: a,
            points: spread(vs).iter().map(|&v| [xx[v], yy[v], zz[v]]).collect(),
            x: 0.0,
            y: 0.0,
        });
    }

    // merge starts a boss rim or a corner shares into one island, lowest first
    found.js_sort_by(|a, b| {
        let d = a.z - b.z;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let mut out: Vec<Island> = Vec::new();
    let close = |o: &Island, s: &Island| -> bool {
        o.points.iter().any(|a| {
            s.points.iter().any(|b| {
                jsnum::hypot(&[a[0] - b[0], a[1] - b[1]]) <= p.merge
                    && jsnum::abs(a[2] - b[2]) <= p.merge_z
            })
        })
    };
    for s in found {
        if let Some(hit) = out.iter_mut().find(|o| close(o, &s)) {
            hit.points.extend(s.points.iter().copied());
            hit.area = jsnum::max(hit.area, s.area);
        } else {
            out.push(Island {
                z: s.z,
                area: s.area,
                points: s.points,
                x: 0.0,
                y: 0.0,
            });
        }
    }
    for o in out.iter_mut() {
        let mut sx = 0.0f64;
        for pt in &o.points {
            sx += pt[0];
        }
        o.x = sx / o.points.len() as f64;
        let mut sy = 0.0f64;
        for pt in &o.points {
            sy += pt[1];
        }
        o.y = sy / o.points.len() as f64;
    }
    out
}
