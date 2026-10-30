//! Port of `web/prop/lattice.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Lattice nets: a big overhang region that is really many struts joined at nodes (issue #121).
//! `lattice_struts` cuts such a region back into its struts so each can take the route a
//! standalone strut already takes; `strut_patches` routes each strut the way `buildProps`
//! routes a standalone small region.

use crate::guards::JsSort;
use std::sync::Arc;

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::tracks::{split_region, tube_line};
use crate::tris::Tris;
use crate::types::Rot3;

/// `CELL` (web/prop/lattice.js:37): mm, plan raster.
const CELL: f64 = 0.5;
/// `NET_HALF` (web/prop/lattice.js:43): a strut's underside is at most this far from its edge
/// (99th percentile of the footprint).
const NET_HALF: f64 = 3.5;
/// `MIN_STRUTS` (web/prop/lattice.js:44): a net has at least this many struts.
const MIN_STRUTS: usize = 5;
/// `NODE_CUT` (web/prop/lattice.js:45): cells of skeleton removed around each junction.
const NODE_CUT: f64 = 1.5;

/// The result of `latticeStruts`: the face groups, one per strut, with the `lost` property the
/// JS array carries (faces no live branch reaches).
#[derive(Clone, Debug, PartialEq)]
pub struct Struts {
    pub groups: Vec<Vec<usize>>,
    pub lost: usize,
}

/// `seated(f)`: face `f`'s three seated vertices.
pub type Seated<'a> = &'a dyn Fn(usize) -> [[f64; 3]; 3];

/// Face groups (one per strut), or `None` (JS `null`) when `faces` is not a lattice net.
///
/// `latticeStruts(topo, faces, seated)` (web/prop/lattice.js:51-191).
pub fn lattice_struts(
    topo: &Topology,
    faces: &[usize],
    seated: Seated,
    cfg: &Config,
) -> Option<Struts> {
    let prop = &cfg.prop;
    let mut area = 0.0f64;
    for &f in faces {
        area += topo.area[f];
    }
    if area < prop.tube_min_area {
        return None; // small regions already take the tube route
    }

    // --- footprint raster ----------------------------------------------------
    let tri: Vec<[[f64; 3]; 3]> = faces.iter().map(|&f| seated(f)).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for t in &tri {
        for v in t {
            if v[0] < x0 {
                x0 = v[0];
            }
            if v[0] > x1 {
                x1 = v[0];
            }
            if v[1] < y0 {
                y0 = v[1];
            }
            if v[1] > y1 {
                y1 = v[1];
            }
        }
    }
    x0 -= 2.0 * CELL;
    y0 -= 2.0 * CELL;
    let w_f = jsnum::ceil((x1 - x0) / CELL) + 3.0;
    let h_f = jsnum::ceil((y1 - y0) / CELL) + 3.0;
    if w_f * h_f > 4e6 {
        return None; // a 1 m part: not a strut net
    }
    let (w, h) = (w_f as usize, h_f as usize);
    let idx = |i: i64, j: i64| (j * w as i64 + i) as usize;
    let cell_of = |x: f64, y: f64| -> (i64, i64) {
        (
            jsnum::floor((x - x0) / CELL) as i64,
            jsnum::floor((y - y0) / CELL) as i64,
        )
    };
    let mut fp = vec![0u8; w * h];
    for t in &tri {
        let (a, b, c) = (t[0], t[1], t[2]);
        let (i0, j0) = cell_of(
            jsnum::min_of(&[a[0], b[0], c[0]]),
            jsnum::min_of(&[a[1], b[1], c[1]]),
        );
        let (i1, j1) = cell_of(
            jsnum::max_of(&[a[0], b[0], c[0]]),
            jsnum::max_of(&[a[1], b[1], c[1]]),
        );
        let d = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let px = x0 + (i as f64 + 0.5) * CELL;
                let py = y0 + (j as f64 + 0.5) * CELL;
                if jsnum::abs(d) < 1e-12 {
                    fp[idx(i, j)] = 1; // edge-on: its box is a sliver anyway
                    continue;
                }
                let u = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / d;
                let v = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / d;
                let tol = CELL / jsnum::sqrt(jsnum::abs(d)); // a half-cell grace, so thin strips stay connected
                if u >= -tol && v >= -tol && 1.0 - u - v >= -tol {
                    fp[idx(i, j)] = 1;
                }
            }
        }
    }

    // --- narrow everywhere? (distance to the footprint's edge, chamfer 3-4) ----
    // (a Float32Array in JS: every store rounds to f32)
    let mut dist: Vec<f32> = fp
        .iter()
        .map(|&b| if b != 0 { 1e9f32 } else { 0.0 })
        .collect();
    let relax = |dist: &mut Vec<f32>, k: usize, n: usize, wgt: f64| {
        if dist[n] as f64 + wgt < dist[k] as f64 {
            dist[k] = (dist[n] as f64 + wgt) as f32;
        }
    };
    for j in 1..h - 1 {
        for i in 1..w - 1 {
            let k = j * w + i;
            if fp[k] == 0 {
                continue;
            }
            relax(&mut dist, k, k - 1, 3.0);
            relax(&mut dist, k, k - w, 3.0);
            relax(&mut dist, k, k - w - 1, 4.0);
            relax(&mut dist, k, k - w + 1, 4.0);
        }
    }
    for j in (1..=h - 2).rev() {
        for i in (1..=w - 2).rev() {
            let k = j * w + i;
            if fp[k] == 0 {
                continue;
            }
            relax(&mut dist, k, k + 1, 3.0);
            relax(&mut dist, k, k + w, 3.0);
            relax(&mut dist, k, k + w + 1, 4.0);
            relax(&mut dist, k, k + w - 1, 4.0);
        }
    }
    let mut inside: Vec<f64> = Vec::new();
    for k in 0..w * h {
        if fp[k] != 0 {
            inside.push((dist[k] as f64 / 3.0) * CELL);
        }
    }
    inside.js_sort_by(|a, b| cmp_diff(a - b));
    if inside.is_empty() {
        return None;
    }
    if inside[jsnum::floor(0.99 * (inside.len() - 1) as f64) as usize] > NET_HALF {
        return None;
    }

    // --- skeleton (Zhang-Suen thinning) --------------------------------------
    let mut sk = fp.clone();
    let nb = |sk: &[u8], k: usize| -> [u8; 8] {
        [
            sk[k - w],
            sk[k - w + 1],
            sk[k + 1],
            sk[k + w + 1],
            sk[k + w],
            sk[k + w - 1],
            sk[k - 1],
            sk[k - w - 1],
        ]
    };
    let mut changed = true;
    while changed {
        changed = false;
        for pass in [0, 1] {
            let mut del: Vec<usize> = Vec::new();
            for j in 1..h - 1 {
                for i in 1..w - 1 {
                    let k = j * w + i;
                    if sk[k] == 0 {
                        continue;
                    }
                    let p = nb(&sk, k); // p2..p9, clockwise from north
                    let b: u32 = p.iter().map(|&q| q as u32).sum();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let mut a = 0;
                    for q in 0..8 {
                        if p[q] == 0 && p[(q + 1) % 8] != 0 {
                            a += 1;
                        }
                    }
                    if a != 1 {
                        continue;
                    }
                    // `p[0] * p[2] * p[4] || p[2] * p[4] * p[6]`: a 0/1 product test
                    let skip = if pass == 0 {
                        p[0] * p[2] * p[4] != 0 || p[2] * p[4] * p[6] != 0
                    } else {
                        p[0] * p[2] * p[6] != 0 || p[0] * p[4] * p[6] != 0
                    };
                    if skip {
                        continue;
                    }
                    del.push(k);
                }
            }
            for &k in &del {
                sk[k] = 0;
            }
            if !del.is_empty() {
                changed = true;
            }
        }
    }

    // --- cut at junctions into branches --------------------------------------
    let wi = w as i64;
    let n8: [i64; 8] = [-wi - 1, -wi, -wi + 1, -1, 1, wi - 1, wi, wi + 1];
    let total = (w * h) as i64;
    // a typed-array read: out of range is `undefined`, i.e. false
    let at = |a: &[u8], k: i64| k >= 0 && k < total && a[k as usize] != 0;
    let mut junction: Vec<usize> = Vec::new();
    for k in 0..w * h {
        if sk[k] == 0 {
            continue;
        }
        let mut n = 0;
        for d in n8 {
            if at(&sk, k as i64 + d) {
                n += 1;
            }
        }
        if n >= 3 {
            junction.push(k);
        }
    }
    if junction.is_empty() {
        return None; // one band or a ring: not a net
    }
    let mut cut = sk.clone();
    let r = jsnum::ceil(NODE_CUT) as i64;
    for &k in &junction {
        let i = (k % w) as i64;
        let j = (k as i64 - i) / wi;
        for dj in -r..=r {
            for di in -r..=r {
                let (ii, jj) = (i + di, j + dj);
                if ii >= 0 && jj >= 0 && ii < wi && jj < h as i64 {
                    cut[idx(ii, jj)] = 0;
                }
            }
        }
    }
    let mut label = vec![-1i32; w * h];
    let mut branches = 0i32;
    for k in 0..w * h {
        if cut[k] == 0 || label[k] >= 0 {
            continue;
        }
        let mut q = vec![k];
        label[k] = branches;
        while let Some(c) = q.pop() {
            for d in n8 {
                let n = c as i64 + d;
                if at(&cut, n) && label[n as usize] < 0 {
                    label[n as usize] = branches;
                    q.push(n as usize);
                }
            }
        }
        branches += 1;
    }
    if branches < 2 {
        return None;
    }

    // --- every footprint cell to its nearest live branch, through the footprint, then faces by
    // the branch under their centroid. A branch whose faces come to under tubeSmallMinArea is
    // dropped and the flood rerun.
    let seed = label.clone();
    let centre: Vec<usize> = tri
        .iter()
        .map(|t| {
            let (i, j) = cell_of(
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][1] + t[1][1] + t[2][1]) / 3.0,
            );
            idx(i, j)
        })
        .collect();
    let mut dead = vec![false; branches as usize];
    let mut groups: Vec<Vec<usize>>;
    loop {
        let mut front: Vec<usize> = Vec::new();
        for k in 0..w * h {
            label[k] = if seed[k] >= 0 && !dead[seed[k] as usize] {
                seed[k]
            } else {
                -1
            };
            if label[k] >= 0 {
                front.push(k);
            }
        }
        while !front.is_empty() {
            let mut next: Vec<usize> = Vec::new();
            for &c in &front {
                for d in n8 {
                    let n = c as i64 + d;
                    if at(&fp, n) && label[n as usize] < 0 {
                        label[n as usize] = label[c];
                        next.push(n as usize);
                    }
                }
            }
            front = next;
        }
        groups = vec![Vec::new(); branches as usize];
        for (n, &f) in faces.iter().enumerate() {
            let b = label[centre[n]];
            if b >= 0 {
                groups[b as usize].push(f);
            }
        }
        let mut pruned = false;
        for (b, g) in groups.iter().enumerate() {
            if g.is_empty() {
                continue;
            }
            let mut s = 0.0f64;
            for &f in g {
                s += topo.area[f];
            }
            if s < prop.tube_small_min_area {
                dead[b] = true;
                pruned = true;
            }
        }
        if !pruned {
            break;
        }
    }
    let sized: Vec<Vec<usize>> = groups.into_iter().filter(|g| !g.is_empty()).collect();
    if sized.len() < MIN_STRUTS {
        return None;
    }
    // Faces no live branch reaches get no wall: counted as the caller's slivers.
    let kept: usize = sized.iter().map(Vec::len).sum();
    Some(Struts {
        lost: faces.len() - kept,
        groups: sized,
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

/// A split patch of a small strut, marked `strut` and carrying the strut's `axis`
/// (`Object.assign(p, { strut: true, axis })` on a `splitRegion` patch).
#[derive(Clone, Debug, PartialEq)]
pub struct StrutSplit {
    pub faces: Vec<usize>,
    pub area: f64,
    pub axis: [f64; 2],
}

/// A `buildProps` patch for one strut (web/prop/lattice.js:215, 221). The key set differs by
/// route: a tube strut carries `reach`, `lines` and `small_tube` (`Some(None)` is JS `null`, a
/// plain tube) but no `axis`; any other strut carries `axis` and none of those. `strut` is
/// always true.
#[derive(Clone, Debug)]
pub struct StrutPatch {
    pub faces: Vec<usize>,
    pub area: f64,
    /// The region index.
    pub region: usize,
    /// `regionTris`: the whole region's triangles, which every patch finishes against.
    pub tris: Arc<Tris>,
    /// The strut's own seated faces (tube route only).
    pub reach: Option<Tris>,
    /// The tube's lines (tube route only).
    pub lines: Option<Vec<Vec<[f64; 3]>>>,
    /// The split patches raced against the tube when the strut is small (tube route only;
    /// `Some(None)` for a plain tube).
    pub small_tube: Option<Option<Vec<StrutSplit>>>,
    pub strut: bool,
    /// The strut's length direction (non-tube route only).
    pub axis: Option<[f64; 2]>,
}

/// `buildProps` patches for a lattice region's struts (`groups`, from [`lattice_struts`]), each
/// marked `strut`.
///
/// `strutPatches(topo, rot, groups, seated, step, region, regionTris)`
/// (web/prop/lattice.js:197-224).
pub fn strut_patches(
    topo: &Topology,
    rot: &Rot3,
    groups: &[Vec<usize>],
    seated: Seated,
    step: f64,
    region: usize,
    region_tris: &Arc<Tris>,
    cfg: &Config,
) -> Vec<StrutPatch> {
    let mut patches = Vec::new();
    for faces in groups {
        let mut tris = vec![0.0f64; faces.len() * 9];
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let mut area = 0.0f64;
        for (k, &f) in faces.iter().enumerate() {
            let t = seated(f);
            area += topo.area[f];
            for i in 0..3 {
                tris[k * 9 + i * 3..k * 9 + i * 3 + 3].copy_from_slice(&t[i]);
                pts.push(t[i]);
            }
            pts.push([
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                (t[0][2] + t[1][2] + t[2][2]) / 3.0,
            ]);
        }
        let axis = long_axis(&pts);
        let tris = Tris::new(tris);
        let tube = tube_line(topo, faces, rot, &pts, &tris, step, cfg);
        if let Some(lines) = tube.filter(|l| !l.is_empty()) {
            // raced against its split patches (rival.js) when small, as a standalone small tube
            // is; over tubeMinArea it is a plain tube, as a standalone one is
            let small = area < cfg.prop.tube_min_area;
            let split = small.then(|| {
                split_region(topo, faces, rot, cfg)
                    .into_iter()
                    .map(|p| StrutSplit {
                        faces: p.faces,
                        area: p.area,
                        axis,
                    })
                    .collect()
            });
            patches.push(StrutPatch {
                faces: faces.clone(),
                area,
                region,
                tris: region_tris.clone(),
                reach: Some(tris),
                lines: Some(lines),
                small_tube: Some(split),
                strut: true,
                axis: None,
            });
            continue;
        }
        // Not a tube: the WHOLE strut is one patch.
        patches.push(StrutPatch {
            faces: faces.clone(),
            area,
            region,
            tris: region_tris.clone(),
            reach: None,
            lines: None,
            small_tube: None,
            strut: true,
            axis: Some(axis),
        });
    }
    patches
}

/// Unit principal axis of points in plan (a strut's length direction).
///
/// `longAxis(pts)` (web/prop/lattice.js:227-239).
fn long_axis(pts: &[[f64; 3]]) -> [f64; 2] {
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    for p in pts {
        cx += p[0];
        cy += p[1];
    }
    cx /= pts.len() as f64;
    cy /= pts.len() as f64;
    let (mut sxx, mut sxy, mut syy) = (0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        let dx = p[0] - cx;
        let dy = p[1] - cy;
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
        ux = if sxx >= syy { 1.0 } else { 0.0 };
        uy = if sxx >= syy { 0.0 } else { 1.0 };
    }
    let n = jsnum::hypot(&[ux, uy]);
    [ux / n, uy / n]
}
