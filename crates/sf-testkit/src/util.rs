//! Port of `tests/_util.js` (the parts that need no topology).

use indexmap::IndexMap;
use sf_engine::jsnum;
use std::path::PathBuf;

/// The upstream checkout: `checkout=` in the repo's `UPSTREAM` file, with a leading `~` expanded.
/// (Stands in for `tests/_util.js:9-10`, which locates `web/` and `MODELS` relative to the tests.)
pub fn upstream_root() -> PathBuf {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../UPSTREAM");
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("checkout="))
        .unwrap_or_else(|| panic!("no `checkout=` line in {}", file.display()));
    let line = line.trim();
    match line.strip_prefix('~') {
        Some(rest) => {
            let home = std::env::var("HOME").expect("HOME is not set");
            PathBuf::from(format!("{home}{rest}"))
        }
        None => PathBuf::from(line),
    }
}

/// `MODELS` (tests/_util.js:10): `<upstream>/prototype/stress/models`.
pub fn models_dir() -> PathBuf {
    upstream_root().join("prototype/stress/models")
}

/// `readSTL` of `tests/fixtures/<name>.stl` in the upstream checkout (raster.test.js `example`).
pub fn load_fixture_soup(name: &str) -> Vec<f64> {
    let path = upstream_root()
        .join("tests/fixtures")
        .join(format!("{name}.stl"));
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    sf_io::stl::read_stl(&bytes).unwrap_or_else(|e| panic!("{}: {}", path.display(), e.0))
}

/// Port of `assertClose` (tests/_util.js:22-24). Like JS, a NaN difference does not fail
/// (`NaN > tol` is false). The message is `${msg || 'not close'}: ${a} vs ${b} (tol ${tol})`.
pub fn assert_close(a: f64, b: f64, tol: f64, msg: &str) {
    if jsnum::abs(a - b) > tol {
        let m = if msg.is_empty() { "not close" } else { msg };
        panic!(
            "{m}: {} vs {} (tol {})",
            jsnum::to_js_string(a),
            jsnum::to_js_string(b),
            jsnum::to_js_string(tol)
        );
    }
}

/// The soup of `loadModel` (tests/_util.js:27-30, without `buildTopology`): `readSTL` of
/// `<models_dir>/<name>.stl`.
pub fn load_model_soup(name: &str) -> Vec<f64> {
    let path = models_dir().join(format!("{name}.stl"));
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    sf_io::stl::read_stl(&bytes).unwrap_or_else(|e| panic!("{}: {}", path.display(), e.0))
}

/// `Float32Array` store: every value rounds to f32.
fn f32s(v: f64) -> f64 {
    (v as f32) as f64
}

/// Port of `block` (tests/_util.js:33-43): a solid axis-aligned block as a closed triangle
/// soup. JS returns a `Float32Array`, so each value is rounded to f32 as stored.
pub fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<f64> {
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let q = |a: usize, b: usize, c: usize, d: usize| [v[a], v[b], v[c], v[a], v[c], v[d]];
    let mut t: Vec<[f64; 3]> = Vec::new();
    for quad in [
        q(0, 3, 2, 1),
        q(4, 5, 6, 7),
        q(0, 1, 5, 4),
        q(2, 3, 7, 6),
        q(1, 2, 6, 5),
        q(0, 4, 7, 3),
    ] {
        t.extend(quad);
    }
    let mut arr = Vec::with_capacity(t.len() * 3);
    for p in t {
        arr.push(f32s(p[0]));
        arr.push(f32s(p[1]));
        arr.push(f32s(p[2]));
    }
    arr
}

/// The soup `tiltedBlockTopo` builds (tests/_util.js:52-64, lines 53-62; the `buildTopology`
/// wrapper is M1): a block with a rotation about X baked in, reseated so min z = 0.
/// Computes in f64 from the f32 `raw` values, stores into a `Float32Array` (round), takes
/// `minZ` from the *stored* values, then subtracts it in place (round again).
pub fn tilted_block_soup(
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    z0: f64,
    z1: f64,
    deg_x: f64,
) -> Vec<f64> {
    let a = (deg_x * std::f64::consts::PI) / 180.0;
    let c = jsnum::cos(a);
    let s = jsnum::sin(a);
    let raw = block(x0, x1, y0, y1, z0, z1);
    let mut min_z = f64::INFINITY;
    let mut rot = vec![0.0; raw.len()];
    let mut i = 0;
    while i < raw.len() {
        let y = raw[i + 1];
        let z = raw[i + 2];
        rot[i] = raw[i];
        rot[i + 1] = f32s(y * c - z * s);
        rot[i + 2] = f32s(y * s + z * c);
        if rot[i + 2] < min_z {
            min_z = rot[i + 2];
        }
        i += 3;
    }
    let mut i = 0;
    while i < rot.len() {
        rot[i + 2] = f32s(rot[i + 2] - min_z);
        i += 3;
    }
    rot
}

/// The soup `holedPlateTopo` builds (tests/_util.js:70-91, lines 71-89; the `buildTopology`
/// wrapper is M1): a wide thin plate x[-X,X] y[-Y,Y] z[0,TH] with a rectangular bore
/// x[-hx,hx] y[-hy,hy] punched through Z, as a watertight frame slab. Stored in a
/// `Float32Array`, so each value rounds to f32.
pub fn holed_plate_soup(x: f64, y: f64, th: f64, hx: f64, hy: f64) -> Vec<f64> {
    type V = [f64; 3];
    fn quad(tris: &mut Vec<V>, a: V, b: V, c: V, d: V) {
        tris.extend([a, b, c, a, c, d]);
    }
    let v = |x: f64, y: f64, z: f64| -> V { [x, y, z] };
    let mut tris: Vec<V> = Vec::new();
    let frame = |tris: &mut Vec<V>, z: f64, up: bool| {
        let strips = [
            [-x, -y, -hx, y],
            [hx, -y, x, y],
            [-hx, -y, hx, -hy],
            [-hx, hy, hx, y],
        ];
        for [x0, y0, x1, y1] in strips {
            let p = [v(x0, y0, z), v(x1, y0, z), v(x1, y1, z), v(x0, y1, z)];
            if up {
                quad(tris, p[0], p[1], p[2], p[3]);
            } else {
                quad(tris, p[0], p[3], p[2], p[1]);
            }
        }
    };
    frame(&mut tris, th, true);
    frame(&mut tris, 0.0, false);
    let wall = |tris: &mut Vec<V>, x0: f64, y0: f64, x1: f64, y1: f64, out: bool| {
        let a = v(x0, y0, 0.0);
        let b = v(x1, y1, 0.0);
        let c = v(x1, y1, th);
        let d = v(x0, y0, th);
        if out {
            quad(tris, a, b, c, d);
        } else {
            quad(tris, a, d, c, b);
        }
    };
    wall(&mut tris, -x, -y, x, -y, true);
    wall(&mut tris, x, -y, x, y, true);
    wall(&mut tris, x, y, -x, y, true);
    wall(&mut tris, -x, y, -x, -y, true);
    wall(&mut tris, -hx, -hy, hx, -hy, false);
    wall(&mut tris, hx, -hy, hx, hy, false);
    wall(&mut tris, hx, hy, -hx, hy, false);
    wall(&mut tris, -hx, hy, -hx, -hy, false);
    let mut pos = Vec::with_capacity(tris.len() * 3);
    for p in tris {
        pos.push(f32s(p[0]));
        pos.push(f32s(p[1]));
        pos.push(f32s(p[2]));
    }
    pos
}

/// `d2r` (tests/_util.js:94): `(d * Math.PI) / 180`.
fn d2r(d: f64) -> f64 {
    (d * std::f64::consts::PI) / 180.0
}

/// Port of `rotX` (tests/_util.js:95): column-major 3x3, `THREE.Matrix3.elements` order.
pub fn rot_x(deg: f64) -> [f64; 9] {
    let c = jsnum::cos(d2r(deg));
    let s = jsnum::sin(d2r(deg));
    [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
}

/// Port of `rotY` (tests/_util.js:96).
pub fn rot_y(deg: f64) -> [f64; 9] {
    let c = jsnum::cos(d2r(deg));
    let s = jsnum::sin(d2r(deg));
    [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c]
}

/// The vertex key of `isClosed`/`isOriented`: `${Math.round(x*1e3)},${Math.round(y*1e3)},${Math.round(z*1e3)}`.
fn key(p: &[f64; 3]) -> String {
    let n = |v: f64| jsnum::to_js_string(jsnum::round(v * 1e3));
    format!("{},{},{}", n(p[0]), n(p[1]), n(p[2]))
}

/// Port of `isClosed` (tests/_util.js:107-120): is the triangle soup a closed surface
/// (every undirected edge shared evenly)? Edges are keyed by the JS *string* order.
pub fn is_closed(tris: &[[f64; 3]]) -> bool {
    if tris.is_empty() {
        return true;
    }
    let mut edges: IndexMap<String, i64> = IndexMap::new();
    let mut i = 0;
    while i < tris.len() {
        for e in 0..3 {
            let a = key(&tris[i + e]);
            let b = key(&tris[i + (e + 1) % 3]);
            let k = if a < b {
                format!("{a}|{b}")
            } else {
                format!("{b}|{a}")
            };
            *edges.entry(k).or_insert(0) += 1;
        }
        i += 3;
    }
    for c in edges.values() {
        if c % 2 != 0 {
            return false;
        }
    }
    true
}

/// Port of `isOriented` (tests/_util.js:125-141): every edge is walked once each way and the
/// enclosed volume is positive (normals point out). `isClosed` only counts edges, so it
/// passes a mesh with a patch wound inside-out -- one a slicer misreads.
pub fn is_oriented(tris: &[[f64; 3]]) -> bool {
    let mut edges: IndexMap<String, i64> = IndexMap::new();
    let mut vol = 0.0;
    let mut i = 0;
    while i < tris.len() {
        let (p, q, r) = (tris[i], tris[i + 1], tris[i + 2]);
        vol += p[0] * (q[1] * r[2] - q[2] * r[1]) - p[1] * (q[0] * r[2] - q[2] * r[0])
            + p[2] * (q[0] * r[1] - q[1] * r[0]);
        for e in 0..3 {
            let a = key(&tris[i + e]);
            let b = key(&tris[i + (e + 1) % 3]);
            if a == b {
                continue;
            }
            let k = if a < b {
                format!("{a}|{b}")
            } else {
                format!("{b}|{a}")
            };
            *edges.entry(k).or_insert(0) += if a < b { 1 } else { -1 };
        }
        i += 3;
    }
    for c in edges.values() {
        if *c != 0 {
            return false;
        }
    }
    vol > 0.0
}

/// Port of `ptTriDist2` (tests/_util.js:144-163): squared distance from point `p` to triangle
/// `(a, b, c)`, branch for branch and in the same expression order.
pub fn pt_tri_dist2(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let sub = |u: [f64; 3], v: [f64; 3]| [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return dot(ap, ap);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return dot(bp, bp);
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return dot(cp, cp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        let q = [a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]];
        let w = sub(p, q);
        return dot(w, w);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        let q = [a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]];
        let u = sub(p, q);
        return dot(u, u);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let q = [
            b[0] + w * (c[0] - b[0]),
            b[1] + w * (c[1] - b[1]),
            b[2] + w * (c[2] - b[2]),
        ];
        let u = sub(p, q);
        return dot(u, u);
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    let q = [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ];
    let u = sub(p, q);
    dot(u, u)
}

/// Axis-aligned bounding box of a vertex list (`bbox`'s `{lo, hi}`).
#[derive(Debug, Clone, PartialEq)]
pub struct Bbox {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

/// Port of `bbox` (tests/_util.js:188-192). Uses `Math.min`/`Math.max` semantics.
pub fn bbox(tris: &[[f64; 3]]) -> Bbox {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in tris {
        for k in 0..3 {
            lo[k] = jsnum::min(lo[k], v[k]);
            hi[k] = jsnum::max(hi[k], v[k]);
        }
    }
    Bbox { lo, hi }
}

/// One box of `tineBoxes`: `{ verts, lo, hi }`.
#[derive(Debug, Clone, PartialEq)]
pub struct TineBox {
    pub verts: Vec<[f64; 3]>,
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

/// A wall step: its box (`{verts, lo, hi}`) plus `tine`, the index into `TineBoxes::tines`
/// of the box it sits under.
#[derive(Debug, Clone, PartialEq)]
pub struct TineStep {
    pub b: TineBox,
    pub tine: usize,
}

/// `{ tines, steps }` as `tineBoxes` returns them.
#[derive(Debug, Clone, PartialEq)]
pub struct TineBoxes {
    pub tines: Vec<TineBox>,
    pub steps: Vec<TineStep>,
}

/// Port of `tineBoxes` (tests/_util.js:199-215): split `emitTines` output into its boxes.
/// Each tine is a 36-vertex block, followed by its WALL STEP (another 36) when the tine sits
/// above the wall top (local issue 005). A step starts below its tine, ends inside it and lies
/// inside its footprint, which a next tine along the run never does.
pub fn tine_boxes(out: &[[f64; 3]]) -> TineBoxes {
    let mut tines: Vec<TineBox> = Vec::new();
    let mut steps: Vec<TineStep> = Vec::new();
    let within = |b: &TineBox, t: &TineBox| {
        (0..2).all(|k| b.lo[k] >= t.lo[k] - 1e-6 && b.hi[k] <= t.hi[k] + 1e-6)
    };
    let mut prev: Option<usize> = None; // index into `tines`
    let mut i = 0;
    while i + 36 <= out.len() {
        let verts = out[i..i + 36].to_vec();
        let bb = bbox(&verts);
        let b = TineBox {
            verts,
            lo: bb.lo,
            hi: bb.hi,
        };
        let is_step = match prev {
            Some(p) => {
                let pb = &tines[p];
                b.lo[2] < pb.lo[2] && b.hi[2] > pb.lo[2] && b.hi[2] < pb.hi[2] && within(&b, pb)
            }
            None => false,
        };
        if is_step {
            steps.push(TineStep {
                b,
                tine: prev.unwrap(),
            });
            prev = None; // at most one step per tine
        } else {
            tines.push(b);
            prev = Some(tines.len() - 1);
        }
        i += 36;
    }
    TineBoxes { tines, steps }
}
