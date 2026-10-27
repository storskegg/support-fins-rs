//! Port of upstream `tests/weld.test.js`: `buildTopology` welds the STL's triangle soup through
//! numeric hash tables (it used to key a Map on strings). The topology has to be the SAME --
//! vertex ids, edge count, every adjacency pair in order -- or regions, and so every support, shift.

use indexmap::IndexMap;
use sf_engine::jsnum;
use sf_engine::overhangs::build_topology;
use sf_testkit::{load_model_soup, models_dir};

/// The string-keyed weld it replaced, verbatim in effect (weld.test.js:10-33).
struct Reference {
    adj_a: Vec<i32>,
    adj_b: Vec<i32>,
    vertex_count: usize,
    edge_count: usize,
}

fn reference(pos: &[f64]) -> Reference {
    let n_faces = pos.len() / 9;
    let key3 = |x: f64, y: f64, z: f64| {
        format!(
            "{},{},{}",
            jsnum::to_js_string(jsnum::round(x * 1000.0)),
            jsnum::to_js_string(jsnum::round(y * 1000.0)),
            jsnum::to_js_string(jsnum::round(z * 1000.0))
        )
    };
    let mut weld: IndexMap<String, i32> = IndexMap::new();
    let mut vid = vec![0i32; n_faces * 3];
    for f in 0..n_faces {
        for i in 0..3 {
            let o = f * 9 + i * 3;
            let k = key3(pos[o], pos[o + 1], pos[o + 2]);
            let n = weld.len() as i32;
            let id = *weld.entry(k).or_insert(n);
            vid[f * 3 + i] = id;
        }
    }
    let mut first_face: IndexMap<String, i32> = IndexMap::new();
    let (mut adj_a, mut adj_b) = (Vec::new(), Vec::new());
    for f in 0..n_faces {
        for i in 0..3 {
            let a = vid[f * 3 + i];
            let b = vid[f * 3 + ((i + 1) % 3)];
            let k = if a < b {
                format!("{a}_{b}")
            } else {
                format!("{b}_{a}")
            };
            match first_face.get(&k) {
                None => {
                    first_face.insert(k, f as i32);
                }
                Some(&prev) => {
                    adj_a.push(prev);
                    adj_b.push(f as i32);
                }
            }
        }
    }
    Reference {
        adj_a,
        adj_b,
        vertex_count: weld.len(),
        edge_count: first_face.len(),
    }
}

fn same(pos: &[f64], what: &str) {
    let t = build_topology(pos.to_vec());
    let r = reference(pos);
    assert_eq!(
        t.vertex_count, r.vertex_count,
        "{what}: {} vertices vs {}",
        t.vertex_count, r.vertex_count
    );
    assert_eq!(
        t.edge_count, r.edge_count,
        "{what}: {} edges vs {}",
        t.edge_count, r.edge_count
    );
    assert_eq!(
        t.adj_a.len(),
        r.adj_a.len(),
        "{what}: {} adjacencies vs {}",
        t.adj_a.len(),
        r.adj_a.len()
    );
    for i in 0..r.adj_a.len() {
        assert!(
            t.adj_a[i] == r.adj_a[i] && t.adj_b[i] == r.adj_b[i],
            "{what}: adjacency {i} differs"
        );
    }
}

// weld.test.js:46
#[test]
fn the_weld_matches_the_string_keyed_one_on_every_stress_model() {
    let mut names: Vec<String> = std::fs::read_dir(models_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".stl"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    for name in names {
        same(&load_model_soup(name.trim_end_matches(".stl")), &name);
    }
}

// weld.test.js:53
#[test]
fn the_weld_keeps_the_old_key_edge_cases_minus_0_half_micron_rounding_nan_huge_duplicates() {
    let tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| [a, b, c].concat();
    let pos: Vec<f64> = [
        tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        tri([-0.0, -0.0004, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), // rounds to -0: welds with 0
        tri([0.0005, 0.0, 0.0], [0.0015, 0.0, 0.0], [0.0, 0.0025, 0.0]), // .5 rounding up, as Math.round does
        tri([f64::NAN, 0.0, 0.0], [f64::NAN, 0.0, 0.0], [0.0, 0.0, 5.0]), // every NaN welds together
        tri([1e30, 2e30, -3e30], [1e30, 2e30, -3e30], [1.0, 0.0, 0.0]),
        tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // an exact duplicate face
        tri([5.0, 5.0, 5.0], [5.0, 5.0, 5.0], [5.0, 5.0, 5.0]), // a point
        tri(
            [f64::INFINITY, 0.0, 0.0],
            [f64::NEG_INFINITY, 0.0, 0.0],
            [f64::INFINITY, 0.0, 0.0],
        ),
        tri([3e38, 0.0, 0.0], [3e38, 0.0, 0.0], [-3e38, 1.0, 1.0]), // float32-sized huge
    ]
    .concat();
    same(&pos, "edge cases");
}

// weld.test.js:68
#[test]
fn the_weld_matches_on_a_dense_random_ish_soup_with_many_shared_vertices() {
    // a 120 x 120 grid of quads, each corner jittered below the 1e-3 weld step, faces shuffled
    let n = 120usize;
    let mut s: u32 = 7;
    // r = () => ((s = Math.imul(s ^ (s >>> 13), 0x5bd1e995) + 12345 >>> 0) / 2 ** 32 - 0.5) * 4e-4
    let mut r = || {
        let m = ((s ^ (s >> 13)) as i32).wrapping_mul(0x5bd1e995);
        s = (m as i64 + 12345) as u32;
        (s as f64 / 4294967296.0 - 0.5) * 4e-4
    };
    let mut p = |i: usize, j: usize| -> [f64; 3] {
        let (i, j) = (i as f64, j as f64);
        let x = i * 0.7 + r();
        let y = j * 0.7 + r();
        let z = jsnum::sin(i * 0.3) * jsnum::cos(j * 0.2) * 5.0 + r();
        [x, y, z]
    };
    let mut faces: Vec<[[f64; 3]; 3]> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (a, b, c) = (p(i, j), p(i + 1, j), p(i + 1, j + 1));
            faces.push([a, b, c]);
            let (a, b, c) = (p(i, j), p(i + 1, j + 1), p(i, j + 1));
            faces.push([a, b, c]);
        }
    }
    let mut k = faces.len() - 1;
    while k > 0 {
        // (Math.imul(k, 2654435761) >>> 0) % (k + 1)
        let m = ((jsnum::imul(k as f64, 2654435761.0) as u32) as usize) % (k + 1);
        faces.swap(k, m);
        k -= 1;
    }
    // new Float32Array(pos)
    let pos: Vec<f64> = faces
        .iter()
        .flatten()
        .flatten()
        .map(|&v| (v as f32) as f64)
        .collect();
    same(&pos, "dense grid");
}
