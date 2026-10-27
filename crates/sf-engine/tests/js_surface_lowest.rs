//! Port of upstream `tests/surface_lowest.test.js`: `surfaceZAt` (web/prop/surface.js) stops
//! scanning a cell once no triangle left in it can reach below the best height so far. It must
//! still return EXACTLY the lowest height a full scan finds -- the bed pad, wall tops and wedge
//! feet all stand on that number.

use sf_engine::jsnum;
use sf_engine::prop::surface::surface_z_at;
use sf_engine::tris::Tris;
use sf_testkit::load_model;

/// zOn + the minimum over every triangle: the scan surfaceZAt used to do (surface_lowest.test.js:11-26).
fn scan(tris: &[f64], x: f64, y: f64) -> Option<f64> {
    let mut best = f64::INFINITY;
    let mut i = 0;
    while i < tris.len() {
        let (ax, ay, az) = (tris[i], tris[i + 1], tris[i + 2]);
        let (bx, by, bz) = (tris[i + 3], tris[i + 4], tris[i + 5]);
        let (cx, cy, cz) = (tris[i + 6], tris[i + 7], tris[i + 8]);
        i += 9;
        let den = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
        if jsnum::abs(den) < 1e-12 {
            continue;
        }
        let l1 = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / den;
        let l2 = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / den;
        let l3 = 1.0 - l1 - l2;
        if l1 < -1e-9 || l2 < -1e-9 || l3 < -1e-9 {
            continue;
        }
        let z = l1 * az + l2 * bz + l3 * cz;
        if z < best {
            best = z;
        }
    }
    if best == f64::INFINITY {
        None
    } else {
        Some(best)
    }
}

/// `Object.is` on `number | null`.
fn is(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
        _ => false,
    }
}

// surfaceZAt sorts a grid's lists only once the grid has been asked more than
// 2000 * (1 + triangles / 1000) times (LOW_FIRST_AFTER in surface.js), so a test
// that only probes a few hundred points never reaches the early exit. Ask at a
// vertex (a point off the grid isn't counted) until the grid has sorted.
fn warm(tris: &Tris) {
    let need = 2000.0 * (1.0 + tris.len() as f64 / 9.0 / 1000.0) + 1.0;
    let mut k = 0.0;
    while k < need {
        surface_z_at(tris, tris.v[0], tris.v[1]);
        k += 1.0;
    }
}

fn seated(pos: &[f64], ax: f64, dz: f64) -> Vec<f64> {
    let (c, s) = (jsnum::cos(ax), jsnum::sin(ax));
    let mut out = vec![0.0; pos.len()];
    let mut i = 0;
    while i < pos.len() {
        out[i] = pos[i];
        out[i + 1] = c * pos[i + 1] - s * pos[i + 2];
        out[i + 2] = s * pos[i + 1] + c * pos[i + 2] + dz;
        i += 3;
    }
    out
}

// surface_lowest.test.js:40
#[test]
fn surface_z_at_returns_exactly_the_full_scans_lowest_height() {
    let mut s: u32 = 99;
    // r = () => ((s = Math.imul(s ^ (s >>> 15), 0x2c1b3c6d) + 0x9e3779b9 >>> 0) / 2 ** 32)
    let mut r = || {
        let m = ((s ^ (s >> 15)) as i32).wrapping_mul(0x2c1b3c6d);
        s = (m as i64 + 0x9e3779b9) as u32;
        s as f64 / 4294967296.0
    };
    let (mut hits, mut n) = (0usize, 0usize);
    for name in [
        "torus",
        "sphere",
        "staircase",
        "portal",
        "tube",
        "arch",
        "plus",
        "cone",
    ] {
        let topo = load_model(name);
        for (ax, dz) in [(0.0, 0.0), (0.7, 12.0), (2.1, -30.0)] {
            // tilted, raised, below the plate
            let tris = Tris::new(seated(&topo.pos, ax, dz));
            let v = &tris.v;
            let (mut x0, mut x1, mut y0, mut y1) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            let mut i = 0;
            while i < v.len() {
                x0 = jsnum::min(x0, v[i]);
                x1 = jsnum::max(x1, v[i]);
                y0 = jsnum::min(y0, v[i + 1]);
                y1 = jsnum::max(y1, v[i + 1]);
                i += 3;
            }
            let mut pts: Vec<[f64; 2]> = Vec::new();
            for _ in 0..150 {
                let px = x0 + (x1 - x0) * (r() * 1.1 - 0.05);
                let py = y0 + (y1 - y0) * (r() * 1.1 - 0.05);
                pts.push([px, py]);
            }
            // on vertices and edge midpoints, where neighbouring triangles tie
            for _ in 0..40 {
                let t = jsnum::floor(r() * (v.len() / 9) as f64) as usize * 9;
                pts.push([v[t], v[t + 1]]);
                pts.push([(v[t] + v[t + 3]) / 2.0, (v[t + 1] + v[t + 4]) / 2.0]);
            }
            for path in ["plain", "lowest-first"] {
                if path == "lowest-first" {
                    warm(&tris);
                }
                for [x, y] in &pts {
                    let want = scan(v, *x, *y);
                    let got = surface_z_at(&tris, *x, *y);
                    assert!(
                        is(got, want),
                        "{name} ax {ax} {path}: ({x}, {y}) gave {got:?}, the scan {want:?}"
                    );
                    n += 1;
                    if want.is_some() {
                        hits += 1;
                    }
                }
            }
        }
    }
    assert!(hits > n / 3, "too few probes over the part: {hits} of {n}");
}

// A corrupt binary STL can carry NaN vertices. The plain scan skipped those triangles; on the
// lowest-first path a NaN floor made the sort's comparisons contradict each other (heights 5,
// NaN, 1: index order says 5 before NaN before 1, height order says 1 before 5), the lists came
// out unsorted, and the early exit stopped before the true lowest surface -- 2 here instead of 1.
// surface_lowest.test.js:82
#[test]
fn surface_z_at_ignores_nan_triangles_on_the_lowest_first_path() {
    let mut tris = Vec::new();
    for z in [5.0, f64::NAN, 1.0, 4.0, f64::NAN, 3.0, 2.0] {
        tris.extend([-10.0, -10.0, z, 10.0, -10.0, z, 0.0, 10.0, z]);
    }
    let t = Tris::new(tris);
    warm(&t);
    for (x, y) in [(0.0, 0.0), (1.0, -3.0), (-2.0, 4.0)] {
        let want = scan(&t.v, x, y);
        let got = surface_z_at(&t, x, y);
        assert!(
            is(got, want) && want == Some(1.0),
            "({x}, {y}) gave {got:?}, the scan {want:?}"
        );
    }
}
