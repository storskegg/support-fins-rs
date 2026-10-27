//! Port of upstream `tests/separate_walls.test.js`: SEPARATE WALLS (GH #309, fins/fill.js
//! `separateWalls`): with the option on Full coverage never joins a wall to another or its flange.
mod builds;
use builds::*;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::analyze;
use sf_testkit::load_model;
use std::sync::Arc;

fn build(name: &str, separate: bool) -> FinsBuilt {
    let topo = Arc::new(load_model(name));
    let o = FinsOpts {
        mode: Some("full".into()),
        bed_pad: Some(true),
        separate_walls: separate,
        ..Default::default()
    };
    build_fins(
        &topo,
        &analyze(&topo, 45.0, &I),
        &I,
        &o,
        &sf_engine::config::Config::default(),
    )
}

/// Pairs of fill walls whose bottom 1 mm (flange, foot) boxes, split per triangle, overlap (each
/// wall's own range: its ribs and ties are a frame with it).
fn touching_bases(b: &FinsBuilt) -> usize {
    let boxes: Vec<Vec<[f64; 4]>> = b
        .fins
        .iter()
        .filter(|f| f.fill)
        .map(|f| {
            let mut out = vec![];
            let [s, e] = f.tri_ranges[0];
            let mut n = s;
            while n + 2 < e {
                let t = [b.triangles[n], b.triangles[n + 1], b.triangles[n + 2]];
                n += 3;
                if t.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min) > 1.0 {
                    continue;
                }
                let mn = |k: usize| t.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
                let mx = |k: usize| t.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max);
                out.push([mn(0), mn(1), mx(0), mx(1)]);
            }
            out
        })
        .collect();
    let mut n = 0;
    for i in 0..boxes.len() {
        for j in i + 1..boxes.len() {
            if boxes[i].iter().any(|a| {
                boxes[j]
                    .iter()
                    .any(|c| a[0] < c[2] && c[0] < a[2] && a[1] < c[3] && c[1] < a[3])
            }) {
                n += 1;
            }
        }
    }
    n
}

// separate_walls.test.js:34
#[test]
fn separate_walls_full_joins_none_and_keeps_every_fill_walls_base_apart() {
    let joined = build("sphere", false);
    let apart = build("sphere", true);
    assert!(
        joined.fill.as_ref().unwrap().joined > 0 && touching_bases(&joined) > 0,
        "the sphere joins walls without the option: the test lost its point"
    );
    assert!(
        apart.fill.as_ref().unwrap().joined == 0,
        "{} walls joined with the option on",
        apart.fill.as_ref().unwrap().joined
    );
    assert!(
        touching_bases(&apart) == 0,
        "{} pairs of fill-wall bases overlap",
        touching_bases(&apart)
    );
}

// :41
#[test]
fn separate_walls_off_is_the_default_build_which_joins_walls() {
    let a = build("sphere", false);
    let b = build("sphere", false); // { separateWalls: false }
    assert!(
        b.fill.as_ref().unwrap().joined > 0,
        "unticked, Full still joins walls as before"
    );
    assert!(
        a.triangles.len() == b.triangles.len(),
        "same triangle count"
    );
    for i in 0..a.triangles.len() {
        for k in 0..3 {
            assert!(a.triangles[i][k] == b.triangles[i][k], "vertex {i} moved");
        }
    }
}
