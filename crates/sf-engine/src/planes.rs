//! Port of `web/planes.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Wall sites: the flat, near-upright patches of the part that a fin can stand against.

use crate::jsnum;
use crate::overhangs::Topology;
use crate::types::{Rot3, Xyz};

/// `MAX_LEAN_DEG` (web/planes.js:41): how far from vertical a face may lean and still take a fin.
pub const MAX_LEAN_DEG: f64 = 45.0;
/// `NORMAL_AGREE_DEG` (web/planes.js:54): how far a face's normal may swing from its seed's.
pub const NORMAL_AGREE_DEG: f64 = 35.0;
/// `FLAT_TOL_IN` (web/planes.js:76): how far the surface may recede from the fitted plane, in mm.
pub const FLAT_TOL_IN: f64 = 1.20;
/// `GROW_SLACK` (web/planes.js:77): outward slack while growing, before the supporting plane is known.
pub const GROW_SLACK: f64 = 0.35;
/// `MIN_PATCH_H` (web/planes.js:80): mm, measured up the face.
pub const MIN_PATCH_H: f64 = 4.0;
/// `MIN_PATCH_W` (web/planes.js:81): mm, measured across it.
pub const MIN_PATCH_W: f64 = 4.0;
/// `MIN_PATCH_AREA` (web/planes.js:82): mm^2.
pub const MIN_PATCH_AREA: f64 = 25.0;

/// `topo._adjStart` / `topo._adjNbr` (web/planes.js:195-211), cached per topology: neighbour
/// lists in CSR form.
#[derive(Debug)]
pub struct FaceAdjacency {
    pub start: Vec<i32>,
    pub nbr: Vec<i32>,
}

/// A JS `{ x, y }` object (a patch's horizontal frame vector and its `mid`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xy {
    pub x: f64,
    pub y: f64,
}

/// One wall patch: the object `fitPatch` returns (web/planes.js:325-336).
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub faces: Vec<usize>,
    pub area: f64,
    pub mid: Xy,
    pub n: Xyz,
    pub u: Xy,
    pub t: Xyz,
    pub h: f64,
    pub d: f64,
    pub u0: f64,
    pub u1: f64,
    pub t0: f64,
    pub t1: f64,
    pub z0: f64,
    pub z1: f64,
    pub lean: f64,
    pub flatness: f64,
    /// (u, t, dn) per vertex, 9 values per face.
    pub tris: Vec<f64>,
}

/// The optional `stats` counters of `findWallPatches` / `fitPatch` (JS keys that start
/// `undefined` and count up with `?? 0`; here they start at 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PatchStats {
    pub grown: usize,
    pub too_small: usize,
    pub not_upright: usize,
    pub not_flat: usize,
    pub too_short: usize,
    pub too_narrow: usize,
}

/// Find every patch of the part a fin could stand against, in the orientation `rot` with the
/// part seated by `offset`; largest area first.
///
/// `findWallPatches(topo, rot, offset, stats = null, excludedFaces = null)`
/// (web/planes.js:95-193). `excluded_faces` is the JS `Uint8Array` (truthy = skip).
pub fn find_wall_patches(
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    mut stats: Option<&mut PatchStats>,
    excluded_faces: Option<&[u8]>,
) -> Vec<Patch> {
    let pos = &topo.pos;
    let n_faces = topo.n_faces;
    let nrm = &topo.nrm;
    let area = &topo.area;
    // A hair of slack past the exact sine (see the JS comment: 45 degrees lands on sin(45)
    // to the last float bit).
    let lean_cut = jsnum::sin((MAX_LEAN_DEG * std::f64::consts::PI) / 180.0) + 1e-6;
    let agree_cut = jsnum::cos((NORMAL_AGREE_DEG * std::f64::consts::PI) / 180.0);

    // rotated face normals, kept because both the grouping test and the plane fit need them
    let mut rn = vec![0.0f64; n_faces * 3];
    let mut upright = vec![0u8; n_faces];
    for f in 0..n_faces {
        if let Some(ex) = excluded_faces
            && ex.get(f).copied().unwrap_or(0) != 0
        {
            continue;
        }
        let i = f * 3;
        let (x, y, z) = (nrm[i], nrm[i + 1], nrm[i + 2]);
        let nx = rot[0] * x + rot[3] * y + rot[6] * z;
        let ny = rot[1] * x + rot[4] * y + rot[7] * z;
        let nz = rot[2] * x + rot[5] * y + rot[8] * z;
        rn[i] = nx;
        rn[i + 1] = ny;
        rn[i + 2] = nz;
        if jsnum::abs(nz) <= lean_cut && jsnum::hypot(&[nx, ny]) > 1e-9 {
            upright[f] = 1;
        }
    }

    // Region-grow each patch from a seed face, testing every candidate against the SEED's plane
    // rather than against its neighbour's (pairwise agreement creeps).
    let adj = face_adjacency(topo);
    let (start, nbr) = (&adj.start, &adj.nbr);
    let mut seeds: Vec<usize> = Vec::new();
    for f in 0..n_faces {
        if upright[f] != 0 {
            seeds.push(f);
        }
    }
    // `seeds.sort((a, b) => area[b] - area[a])`: stable; the biggest face defines the plane
    seeds.sort_by(|&a, &b| cmp_diff(area[b] - area[a]));

    let mut taken = vec![0u8; n_faces];
    let mut v = [0.0f64; 3];
    let mut patches: Vec<Patch> = Vec::new();
    let mut queue: Vec<usize> = Vec::new();

    for &seed in &seeds {
        if taken[seed] != 0 {
            continue;
        }
        let (sx, sy, sz) = (rn[seed * 3], rn[seed * 3 + 1], rn[seed * 3 + 2]);

        let mut sd = 0.0f64;
        for i in 0..3 {
            world_vertex(pos, seed * 9 + i * 3, rot, offset, &mut v);
            sd += v[0] * sx + v[1] * sy + v[2] * sz;
        }
        sd /= 3.0;

        taken[seed] = 1;
        let mut faces = vec![seed];
        let mut total = area[seed];
        queue.clear();
        queue.push(seed);

        while let Some(f) = queue.pop() {
            for e in start[f]..start[f + 1] {
                let g = nbr[e as usize] as usize;
                if taken[g] != 0 || upright[g] == 0 {
                    continue;
                }
                let dot = rn[g * 3] * sx + rn[g * 3 + 1] * sy + rn[g * 3 + 2] * sz;
                if dot < agree_cut {
                    continue;
                }
                let mut outward = f64::NEG_INFINITY;
                let mut inward = f64::INFINITY;
                for i in 0..3 {
                    world_vertex(pos, g * 9 + i * 3, rot, offset, &mut v);
                    let dn = v[0] * sx + v[1] * sy + v[2] * sz - sd;
                    if dn > outward {
                        outward = dn;
                    }
                    if dn < inward {
                        inward = dn;
                    }
                }
                if outward > GROW_SLACK || inward < -FLAT_TOL_IN {
                    continue;
                }
                taken[g] = 1;
                faces.push(g);
                total += area[g];
                queue.push(g);
            }
        }

        if let Some(s) = stats.as_deref_mut() {
            s.grown += 1;
        }
        if total < MIN_PATCH_AREA {
            if let Some(s) = stats.as_deref_mut() {
                s.too_small += 1;
            }
            continue;
        }
        if let Some(patch) = fit_patch(topo, faces, total, &rn, rot, offset, stats.as_deref_mut()) {
            patches.push(patch);
        }
    }

    patches.sort_by(|a, b| cmp_diff(b.area - a.area));
    patches
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

/// Neighbour lists in CSR form, cached on the topology (it is rotation-invariant).
///
/// `faceAdjacency(topo)` (web/planes.js:195-211).
pub fn face_adjacency(topo: &Topology) -> &FaceAdjacency {
    topo.cache.face_adjacency.get_or_init(|| {
        let n_faces = topo.n_faces;
        let (adj_a, adj_b) = (&topo.adj_a, &topo.adj_b);
        let mut start = vec![0i32; n_faces + 1];
        for e in 0..adj_a.len() {
            start[adj_a[e] as usize + 1] += 1;
            start[adj_b[e] as usize + 1] += 1;
        }
        for f in 0..n_faces {
            start[f + 1] += start[f];
        }
        let mut nbr = vec![0i32; start[n_faces] as usize];
        let mut cur: Vec<i32> = start[..n_faces].to_vec();
        for e in 0..adj_a.len() {
            let (a, b) = (adj_a[e] as usize, adj_b[e] as usize);
            nbr[cur[a] as usize] = adj_b[e];
            cur[a] += 1;
            nbr[cur[b] as usize] = adj_a[e];
            cur[b] += 1;
        }
        FaceAdjacency { start, nbr }
    })
}

/// `worldVertex(pos, o, rot, offset, out)` (web/planes.js:213-219): rotate + seat one raw
/// vertex into print space.
fn world_vertex(pos: &[f64], o: usize, rot: &Rot3, offset: &Xyz, out: &mut [f64; 3]) {
    let (x, y, z) = (pos[o], pos[o + 1], pos[o + 2]);
    out[0] = rot[0] * x + rot[3] * y + rot[6] * z + offset.x;
    out[1] = rot[1] * x + rot[4] * y + rot[7] * z + offset.y;
    out[2] = rot[2] * x + rot[5] * y + rot[8] * z + offset.z;
}

/// `fitPatch(topo, g, rn, rot, offset, stats)` (web/planes.js:235-337): fit a plane to one
/// face group (`g = { faces, area }`) and measure it, or `None` if it is not a usable fin site.
fn fit_patch(
    topo: &Topology,
    faces: Vec<usize>,
    g_area: f64,
    rn: &[f64],
    rot: &Rot3,
    offset: &Xyz,
    mut stats: Option<&mut PatchStats>,
) -> Option<Patch> {
    let pos = &topo.pos;
    let area = &topo.area;

    // area-weighted mean normal
    let (mut nx, mut ny, mut nz) = (0.0f64, 0.0f64, 0.0f64);
    for &f in &faces {
        nx += rn[f * 3] * area[f];
        ny += rn[f * 3 + 1] * area[f];
        nz += rn[f * 3 + 2] * area[f];
    }
    let nl = jsnum::hypot(&[nx, ny, nz]);
    if nl < 1e-9 {
        return None;
    }
    nx /= nl;
    ny /= nl;
    nz /= nl;

    let h = jsnum::hypot(&[nx, ny]); // horizontal share of the normal
    if h < 1e-6 {
        // a floor or ceiling, not a wall
        if let Some(s) = stats.as_deref_mut() {
            s.not_upright += 1;
        }
        return None;
    }
    let ux = -ny / h;
    let uy = nx / h; // horizontal, across the face

    // t = n x u, up the face (built from n's OWN horizontal direction; see the JS comment)
    let tx = -nz * (nx / h);
    let ty = -nz * (ny / h);
    let tz = h;

    // The plane is a SUPPORTING plane: it touches the outermost point of the patch.
    let mut v = [0.0f64; 3];
    let mut d = f64::NEG_INFINITY;
    for &f in &faces {
        for i in 0..3 {
            world_vertex(pos, f * 9 + i * 3, rot, offset, &mut v);
            let dn = v[0] * nx + v[1] * ny + v[2] * nz;
            if dn > d {
                d = dn;
            }
        }
    }

    // flatness, extent, and the (u, t) projection in one pass
    let mut tris = vec![0.0f64; faces.len() * 9];
    let mut dev_out = f64::NEG_INFINITY;
    let mut dev_in = f64::INFINITY;
    let (mut u0, mut u1) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut t0, mut t1) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut z0, mut z1) = (f64::INFINITY, f64::NEG_INFINITY);
    for (k, &f) in faces.iter().enumerate() {
        for i in 0..3 {
            world_vertex(pos, f * 9 + i * 3, rot, offset, &mut v);
            let dn = v[0] * nx + v[1] * ny + v[2] * nz - d;
            if dn > dev_out {
                dev_out = dn;
            }
            if dn < dev_in {
                dev_in = dn;
            }
            let u = v[0] * ux + v[1] * uy;
            let t = v[0] * tx + v[1] * ty + v[2] * tz;
            tris[k * 9 + i * 3] = u;
            tris[k * 9 + i * 3 + 1] = t;
            tris[k * 9 + i * 3 + 2] = dn;
            if u < u0 {
                u0 = u;
            }
            if u > u1 {
                u1 = u;
            }
            if t < t0 {
                t0 = t;
            }
            if t > t1 {
                t1 = t;
            }
            if v[2] < z0 {
                z0 = v[2];
            }
            if v[2] > z1 {
                z1 = v[2];
            }
        }
    }

    if dev_in < -FLAT_TOL_IN {
        if let Some(s) = stats.as_deref_mut() {
            s.not_flat += 1;
        }
        return None;
    }
    if t1 - t0 < MIN_PATCH_H {
        if let Some(s) = stats.as_deref_mut() {
            s.too_short += 1;
        }
        return None;
    }
    if u1 - u0 < MIN_PATCH_W {
        if let Some(s) = stats {
            s.too_narrow += 1;
        }
        return None;
    }

    // where this site sits, for keeping chosen fins apart in space
    let umid = (u0 + u1) / 2.0;
    let tmid = (t0 + t1) / 2.0;
    let mid = Xy {
        x: nx * d + ux * umid + tx * tmid,
        y: ny * d + uy * umid + ty * tmid,
    };

    Some(Patch {
        faces,
        area: g_area,
        mid,
        n: Xyz {
            x: nx,
            y: ny,
            z: nz,
        },
        u: Xy { x: ux, y: uy },
        t: Xyz {
            x: tx,
            y: ty,
            z: tz,
        },
        h,
        d,
        u0,
        u1,
        t0,
        t1,
        z0,
        z1,
        lean: jsnum::abs(jsnum::asin(nz) * 180.0 / std::f64::consts::PI),
        flatness: jsnum::max(dev_out, -dev_in),
        tris,
    })
}

/// Is (u, t) inside the patch? `patchCovers(patch, u, t)` (web/planes.js:340-342).
pub fn patch_covers(patch: &Patch, u: f64, t: f64) -> bool {
    patch_probe(patch, u, t).is_some()
}

/// Where the real surface sits at (u, t), as a signed distance from the patch's fitted plane
/// (negative = receded away from the fin), or `None` if the patch does not cover the point.
///
/// `patchProbe(patch, u, t)` (web/planes.js:353-374).
pub fn patch_probe(patch: &Patch, u: f64, t: f64) -> Option<f64> {
    let tri = &patch.tris;
    let mut i = 0;
    while i < tri.len() {
        let (ax, ay) = (tri[i], tri[i + 1]);
        let (bx, by) = (tri[i + 3], tri[i + 4]);
        let (cx, cy) = (tri[i + 6], tri[i + 7]);

        // Skip slivers (see the JS comment).
        let den = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
        if jsnum::abs(den) < 1e-9 {
            i += 9;
            continue;
        }

        let l1 = ((by - cy) * (u - cx) + (cx - bx) * (t - cy)) / den;
        let l2 = ((cy - ay) * (u - cx) + (ax - cx) * (t - cy)) / den;
        let l3 = 1.0 - l1 - l2;
        if l1 < -1e-9 || l2 < -1e-9 || l3 < -1e-9 {
            i += 9;
            continue;
        }

        return Some(l1 * tri[i + 2] + l2 * tri[i + 5] + l3 * tri[i + 8]);
    }
    None
}

/// World point at patch-frame coordinates: `w` out of the face, `u` across, `t` up.
///
/// `patchPoint(p, w, u, t)` (web/planes.js:377-384).
pub fn patch_point(p: &Patch, w: f64, u: f64, t: f64) -> [f64; 3] {
    let dw = p.d + w;
    [
        p.n.x * dw + p.u.x * u + p.t.x * t,
        p.n.y * dw + p.u.y * u + p.t.y * t,
        p.n.z * dw + p.t.z * t,
    ]
}

/// The `t` at which the plane offset by `w` crosses world z = `z`.
///
/// `tAtZ(p, w, z)` (web/planes.js:387).
pub fn t_at_z(p: &Patch, w: f64, z: f64) -> f64 {
    (z - p.n.z * (p.d + w)) / p.t.z
}

/// World z of a patch-frame point. `u` never contributes: u is horizontal.
///
/// `zAt(p, w, t)` (web/planes.js:390).
pub fn z_at(p: &Patch, w: f64, t: f64) -> f64 {
    p.n.z * (p.d + w) + p.t.z * t
}
