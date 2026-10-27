//! Port of upstream `tests/stability.test.js`: FLOAT-NOISE STABILITY (local issue 023). The same
//! part nudged by 1e-9 rad -- far below anything a user, an STL writer or a browser's trig can do
//! on purpose -- must build the same supports. Before #171 and this file, 14 of these 24 scenes
//! changed under the nudge: an end station dropped one ulp outside surfaceZAt's grid, a part edge
//! lying in a wall's end-cap plane read as a pierce (segTriHit's absolute parallel test), and a
//! level wall's comb anchored at whichever end noise made lower.

use sf_engine::inside::solid_clearance;
use sf_engine::three::site_pose;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_testkit::{Scene, assert_close, block_topo, models_dir, run};

// stability.test.js:13-19 -- golden picks not already in the stress set at X40, plus every stress
// model at X40
fn scenes() -> Vec<(String, [f64; 3])> {
    let mut v = vec![
        ("sphere".to_string(), [0.0, 0.0, 0.0]),
        ("lbracket".to_string(), [35.0, 0.0, 0.0]),
    ];
    let mut models: Vec<String> = std::fs::read_dir(models_dir())
        .unwrap()
        .filter_map(|e| {
            e.unwrap()
                .file_name()
                .into_string()
                .unwrap()
                .strip_suffix(".stl")
                .map(str::to_string)
        })
        .collect();
    models.sort();
    v.extend(models.into_iter().map(|m| (m, [40.0, 0.0, 0.0])));
    v
}

// stability.test.js:21
#[test]
fn stability_a_1e_9_rad_nudge_builds_the_same_supports_24_scenes() {
    let mut flips = Vec::new();
    for (model, rot) in scenes() {
        let sig = |eps: f64| {
            let scene = Scene {
                name: model.clone(),
                model: model.clone(),
                rot,
                ..Default::default()
            };
            let o = run(&scene, Some(site_pose(rot, eps)));
            format!("{}w {}t {}tri", o.walls, o.tines, o.tris.len() / 3)
        };
        let base = sig(0.0);
        for eps in [1e-9, -1e-9] {
            let got = sig(eps);
            if got != base {
                flips.push(format!("{model} X{} {eps}: {base} -> {got}", rot[0]));
            }
        }
    }
    assert!(
        flips.is_empty(),
        "output moved under a 1e-9 rad nudge:\n  {}",
        flips.join("\n  ")
    );
}

// A wall's end cap standing in the plane of the part's edge, 0.2 below it -- cube X40's
// geometry: a 30 mm block at 40 deg, the cap in three 10 mm tiles. At these nudges the old
// absolute parallel test read the edge as piercing a tile (d 0); the nearest part is the
// underside, 0.2 x cos 40 away, approached from above.
// stability.test.js:41
#[test]
fn stability_an_edge_lying_in_a_walls_end_cap_plane_is_clearance_not_a_weld() {
    let topo = block_topo(-15.0, 15.0, -15.0, 15.0, -15.0, 15.0);
    let off = Xyz {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    for k in [0.0, -35.0, -29.0, -20.0, -12.0, -7.0, 10.0] {
        let a = (40.0 * std::f64::consts::PI) / 180.0 + k * 1e-9;
        let (c, s) = (a.cos(), a.sin());
        let rot = [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c];
        let y0 = 15.0 * c + 15.0 * s;
        let ze = 15.0 * s - 15.0 * c; // the edge: local (x, 15, -15)
        let mut cap: Vec<[f64; 3]> = Vec::new();
        for x0 in [-15.0, -5.0, 5.0] {
            let x1 = x0 + 10.0;
            let (lo, hi) = (ze - 25.0, ze - 0.2);
            cap.extend([
                [x0, y0, lo],
                [x1, y0, lo],
                [x1, y0, hi],
                [x0, y0, lo],
                [x1, y0, hi],
                [x0, y0, hi],
            ]);
        }
        let hit = solid_clearance(&topo, &rot, &off, &cap, 0.45).expect("no part within reach");
        assert_close(
            hit.d,
            0.2 * ((40.0 * std::f64::consts::PI) / 180.0).cos(),
            1e-3,
            &format!("nudge {k}e-9: d {}", hit.d),
        );
    }
}

// The relative parallel test must not let a real crossing through: a cap tipped 1e-4 rad so its
// top edge runs 0.02 mm INTO the part still measures ~0.
// stability.test.js:61
#[test]
fn stability_a_shallow_real_crossing_still_measures_as_touching() {
    let topo = block_topo(-10.0, 10.0, -10.0, 10.0, -10.0, 10.0);
    // a 20 mm long triangle under the part's bottom face (z = -10), rising 0.02 into it
    let tri = [[-10.0, 0.0, -10.5], [10.0, 0.0, -9.98], [10.0, 0.5, -10.5]];
    let hit = solid_clearance(
        &topo,
        &IDENTITY3,
        &Xyz {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        &tri,
        0.45,
    );
    assert!(
        hit.is_some_and(|h| h.d < 1e-6),
        "crossing read as {:?} clear",
        hit.map(|h| h.d)
    );
}
