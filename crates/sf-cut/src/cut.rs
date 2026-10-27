//! Port of web/cut.js: a slicer's negative parts, cut out of the part (GitHub #104).
//!
//! A negative part is a mesh the slicer subtracts from the object at slice time (threemf.js reads
//! them as an object's `extras`). Left in, the cut comes back solid; left out, the part is uncut --
//! and either way the fins are built for a shape that will not print. So the cut is made here,
//! before the part reaches the engine, and the fins are built for what actually prints.
//!
//! The boolean is Manifold (see `ffi`). It needs closed meshes. A part that is not one (an open or
//! self-touching scan) cannot be cut, and comes back uncut with the reason -- said, never hidden:
//! its fins are for the uncut shape. (cut.js's `'load'` reason, the WASM that would not load, cannot
//! happen here: the library is linked in.)
//!
//! Colours ride through on the faces the cut leaves alone (Manifold keeps each triangle's source
//! face as faceID); a painted face the cut splits loses its paint, since the paint string
//! subdivides the ORIGINAL triangle.

use std::collections::HashMap;

use indexmap::IndexMap;
use sf_engine::jsnum;
use sf_io::paintcode::is_whole;
use sf_io::threemf::{PartColors, PartKind, ReadExtra};

use crate::ffi::{Solid, reserve_ids};

/// What `cutNegatives` returns (web/cut.js:151-198): `{ positions, colors, cut, uncut, why }`.
#[derive(Clone, Debug)]
pub struct CutResult {
    /// the cut part (the input's own positions when nothing was cut); f32 values when cut
    pub positions: Vec<f64>,
    pub colors: Option<PartColors>,
    /// how many negative parts were cut
    pub cut: usize,
    /// how many were not
    pub uncut: usize,
    /// why NONE was, when that is the whole object's reason: `open` (the part is not a closed
    /// solid), `whole` (the negatives remove all of it) or `load`
    pub why: Option<&'static str>,
}

/// The `${x},${y},${z}` key of a point: exact-equal points weld (`-0` and `0` are one string in JS,
/// and every NaN is one).
fn point_key(p: &[f64]) -> [u64; 3] {
    let k = |v: f64| {
        if v.is_nan() {
            u64::MAX
        } else {
            (v + 0.0).to_bits()
        }
    };
    [k(p[0]), k(p[1]), k(p[2])]
}

/// Port of `signedVolume` (web/cut.js:75-83).
fn signed_volume(p: &[f64]) -> f64 {
    let mut v = 0.0;
    let mut i = 0;
    while i + 8 < p.len() {
        v += p[i] * (p[i + 4] * p[i + 8] - p[i + 5] * p[i + 7])
            - p[i + 1] * (p[i + 3] * p[i + 8] - p[i + 5] * p[i + 6])
            + p[i + 2] * (p[i + 3] * p[i + 7] - p[i + 4] * p[i + 6]);
        i += 9;
    }
    v / 6.0
}

/// Port of `meshOf` (web/cut.js:48-73): a triangle soup -> Manifold's indexed mesh, exact-equal
/// points welded; a negative part (`outward`) is turned outward first (signed volume > 0).
fn mesh_of(positions: &[f64], outward: bool) -> (Vec<f32>, Vec<u32>) {
    let mut index: HashMap<[u64; 3], u32> = HashMap::new();
    let mut verts: Vec<f32> = Vec::new();
    let n = positions.len() / 3;
    let mut tri = vec![0u32; n];
    for (v, t) in tri.iter_mut().enumerate() {
        let p = &positions[v * 3..v * 3 + 3];
        let next = (verts.len() / 3) as u32;
        let i = *index.entry(point_key(p)).or_insert_with(|| {
            verts.extend([p[0] as f32, p[1] as f32, p[2] as f32]);
            next
        });
        *t = i;
    }
    // A shell wound inside out is, to Manifold, a hole: subtracting it ADDS its volume.
    if outward && signed_volume(positions) < 0.0 {
        let mut t = 0;
        while t + 2 < tri.len() {
            tri.swap(t + 1, t + 2);
            t += 3;
        }
    }
    (verts, tri)
}

/// Port of `tryManifold` (web/cut.js:85-92): the solid, when Manifold takes the mesh as one and it
/// is not empty.
fn try_manifold(mesh: Option<(Solid, bool, bool)>) -> Option<Solid> {
    match mesh {
        Some((s, true, false)) => Some(s),
        _ => None,
    }
}

/// A scanned negative (a candle for a mould) is typically several closed shells touching along a few
/// edges, plus loose sliver triangles. As one mesh it is not manifold; shell by shell it is. The
/// slivers enclose nothing, so dropping them is safe while they are this small a share of the
/// SURFACE -- area, not count. Only a NEGATIVE part gets this.
const MAX_STRAY: f64 = 0.01;

/// Port of `area` (web/cut.js:107-112).
fn area(p: &[f64], t: usize) -> f64 {
    let o = t * 9;
    let (ux, uy, uz) = (p[o + 3] - p[o], p[o + 4] - p[o + 1], p[o + 5] - p[o + 2]);
    let (vx, vy, vz) = (p[o + 6] - p[o], p[o + 7] - p[o + 1], p[o + 8] - p[o + 2]);
    jsnum::hypot(&[uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx]) / 2.0
}

fn root(up: &mut [usize], mut x: usize) -> usize {
    while up[x] != x {
        up[x] = up[up[x]];
        x = up[x];
    }
    x
}

/// Port of `shellsOf` (web/cut.js:120-166): the mesh's closed shells, split where an edge is shared
/// by other than two triangles, as one solid -- or `None` when the open ones are more than
/// MAX_STRAY of the area (then it is not a solid with debris, it is open).
fn shells_of(positions: &[f64]) -> Option<Solid> {
    let n_t = positions.len() / 9;
    let mut index: HashMap<[u64; 3], usize> = HashMap::new();
    let mut tv = vec![0usize; n_t * 3];
    for (v, slot) in tv.iter_mut().enumerate() {
        let k = point_key(&positions[v * 3..v * 3 + 3]);
        let next = index.len();
        *slot = *index.entry(k).or_insert(next);
    }
    let mut edges: IndexMap<(usize, usize), Vec<usize>> = IndexMap::new();
    for t in 0..n_t {
        for k in 0..3 {
            let (a, b) = (tv[t * 3 + k], tv[t * 3 + (k + 1) % 3]);
            let key = if a < b { (a, b) } else { (b, a) };
            edges.entry(key).or_default().push(t);
        }
    }
    let mut up: Vec<usize> = (0..n_t).collect();
    for ts in edges.values() {
        if ts.len() == 2 {
            let r = root(&mut up, ts[1]);
            let a = root(&mut up, ts[0]);
            up[a] = r;
        }
    }
    let mut shells: IndexMap<usize, Vec<usize>> = IndexMap::new();
    for t in 0..n_t {
        let r = root(&mut up, t);
        shells.entry(r).or_default().push(t);
    }

    let mut solids: Vec<Solid> = Vec::new();
    let mut stray = 0.0;
    let mut total = 0.0;
    for t in 0..n_t {
        total += area(positions, t);
    }
    for ts in shells.values() {
        // `new Float32Array(...)`: the shell's coordinates are stored as f32
        let mut p = vec![0.0f64; ts.len() * 9];
        for (j, &t) in ts.iter().enumerate() {
            for k in 0..9 {
                p[j * 9 + k] = (positions[t * 9 + k] as f32) as f64;
            }
        }
        let (verts, tri) = mesh_of(&p, true);
        match try_manifold(Solid::from_mesh(&verts, &tri, None, None)) {
            Some(s) => solids.push(s),
            None => {
                for &t in ts {
                    stray += area(positions, t);
                }
            }
        }
    }
    if solids.is_empty() || stray > MAX_STRAY * total {
        return None;
    }
    if solids.len() == 1 {
        return solids.pop();
    }
    let refs: Vec<&Solid> = solids.iter().collect();
    Solid::union(&refs)
}

/// Port of `partSolid` (web/cut.js:169): the part as a closed solid, or `None` when it is not one.
fn part_solid(positions: &[f64], face_id: &[u32], id: u32) -> Option<Solid> {
    let (verts, tri) = mesh_of(positions, false);
    try_manifold(Solid::from_mesh(&verts, &tri, Some(face_id), Some(id)))
}

/// Port of `negativeSolid` (web/cut.js:172): a negative part as a closed solid, outward, even shell
/// by shell; or `None`.
fn negative_solid(positions: &[f64]) -> Option<Solid> {
    let (verts, tri) = mesh_of(positions, true);
    try_manifold(Solid::from_mesh(&verts, &tri, None, None)).or_else(|| shells_of(positions))
}

/// Port of `cutNegatives` (web/cut.js:174-208): cut an object's negative parts out of it.
/// `positions`, `colors` and `extras` are a `readThreeMF` object's.
pub fn cut_negatives(
    positions: &[f64],
    colors: Option<&PartColors>,
    extras: &[ReadExtra],
) -> CutResult {
    let negs: Vec<&ReadExtra> = extras
        .iter()
        .filter(|e| e.kind == PartKind::Negative)
        .collect();
    let same = CutResult {
        positions: positions.to_vec(),
        colors: colors.cloned(),
        cut: 0,
        uncut: negs.len(),
        why: None,
    };
    if negs.is_empty() {
        return same;
    }

    let n_tri = positions.len() / 9;
    let part_id = reserve_ids(1);
    let face_ids: Vec<u32> = (0..n_tri as u32).collect();
    let Some(part) = part_solid(positions, &face_ids, part_id) else {
        return CutResult {
            why: Some("open"),
            ..same
        };
    };

    let mut tools: Vec<Solid> = Vec::new();
    for n in &negs {
        if let Some(s) = negative_solid(&n.positions) {
            tools.push(s);
        }
    }
    if tools.is_empty() {
        return same;
    }

    let n_tools = tools.len();
    let result = if n_tools == 1 {
        part.subtract(&tools[0])
    } else {
        let refs: Vec<&Solid> = tools.iter().collect();
        Solid::union(&refs).and_then(|tool| part.subtract(&tool))
    };
    let Some(out) = result.and_then(|r| r.mesh()) else {
        return same;
    };

    if out.tri_verts.is_empty() {
        return CutResult {
            why: Some("whole"),
            ..same
        };
    }
    let np = out.num_prop;
    let mut pos = vec![0.0f64; out.tri_verts.len() * 3];
    for (i, &v) in out.tri_verts.iter().enumerate() {
        let o = v as usize * np;
        for k in 0..3 {
            pos[i * 3 + k] = out.vert_properties[o + k] as f64;
        }
    }
    let colors = carry_colors(
        positions,
        colors,
        &pos,
        &out.face_id,
        &out.run_index,
        &out.run_original_id,
        part_id,
    );
    CutResult {
        positions: pos,
        colors,
        cut: n_tools,
        uncut: negs.len() - n_tools,
        why: None,
    }
}

fn set_at<T>(v: &mut Vec<Option<T>>, i: usize, x: T) {
    if v.len() <= i {
        v.resize_with(i + 1, || None);
    }
    v[i] = Some(x);
}

/// Port of `carryColors` (web/cut.js:210-234): the part's colours onto the cut triangles. A triangle
/// from the part's own surface (its run's original id is the part's) names its source triangle by
/// faceID: it keeps that triangle's rgb, and its paint when it IS that triangle, corner for corner
/// -- paint subdivides the original's corners -- or when the paint is one state over the whole of it
/// (a part's filament, paintcode.js). Triangles the negative part made (the cut's walls) stay
/// uncoloured.
fn carry_colors(
    obj_positions: &[f64],
    src: Option<&PartColors>,
    positions: &[f64],
    face_id: &[u32],
    run_index: &[u32],
    run_original_id: &[u32],
    part_id: u32,
) -> Option<PartColors> {
    let src = src?;
    let mut col = PartColors::default();
    for (r, &oid) in run_original_id.iter().enumerate() {
        if oid != part_id {
            continue;
        }
        for t in (run_index[r] / 3) as usize..(run_index[r + 1] / 3) as usize {
            let f = face_id[t] as usize;
            if let Some(Some(rgb)) = src.rgb.get(f)
                && !rgb.is_empty()
            {
                set_at(&mut col.rgb, t, rgb.clone());
            }
            let Some(Some(paint)) = src.paint.get(f) else {
                continue;
            };
            let mut same = true;
            let mut k = 0;
            while k < 9 && same {
                same = positions[t * 9 + k] == obj_positions[f * 9 + k];
                k += 1;
            }
            if same || is_whole(&paint.v) {
                set_at(&mut col.paint, t, paint.clone());
            }
        }
    }
    (!col.paint.is_empty() || !col.rgb.is_empty()).then_some(col)
}
