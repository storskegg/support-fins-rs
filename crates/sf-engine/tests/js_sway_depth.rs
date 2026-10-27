//! Port of upstream `tests/sway_depth.test.js` (web/sway.js `uncappedDepth`, `reach`): braces that
//! reach 5 / 15 / 50% of their height. The last two cases (a real Auto worker, and a metre-high
//! post through STL/3MF export) need `finworker` / `buildFins` / `threemf` and are ported with them.

use sf_engine::config::Config;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::sway::*;
use sf_testkit::{assert_close, block, block_topo, is_closed, is_oriented};

const ID: [f64; 9] = sf_engine::types::IDENTITY3;

fn opts<'a>() -> SwayOpts<'a> {
    SwayOpts {
        tines: Some(true),
        layer_height: Some(0.6),
        reach: Some(0.15),
        ..Default::default()
    }
}

fn post() -> Topology {
    block_topo(-80.0, 80.0, -60.0, 60.0, 0.0, 1000.0)
}

fn side(topo: &Topology, ny: f64, y: f64) -> usize {
    (0..topo.n_faces)
        .find(|&i| topo.nrm[i * 3 + 1] * ny > 0.9 && (topo.pos[i * 9 + 1] - y).abs() < 1e-6)
        .unwrap()
}

fn pts(t: &[[f64; 4]]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

// sway_depth.test.js:12
#[test]
fn sway_depth_default_off_is_identical_opt_in_reaches_5_15_50_with_the_original_gauge_and_grip() {
    let cfg = Config::default();
    let topo = post();
    let result = analyze(&topo, 45.0, &ID);
    let legacy = build_sway_braces(&topo, &result, &ID, &opts(), &cfg);
    let off = build_sway_braces(
        &topo,
        &result,
        &ID,
        &SwayOpts {
            uncapped_depth: false,
            ..opts()
        },
        &cfg,
    );
    assert!(off == legacy, "off changes legacy geometry");
    for reach in [0.05, 0.15, 0.5] {
        let b = build_sway_braces(
            &topo,
            &result,
            &ID,
            &SwayOpts {
                reach: Some(reach),
                uncapped_depth: true,
                ..opts()
            },
            &cfg,
        );
        let tris = pts(&b.triangles);
        assert!(b.count >= 2 && is_closed(&tris) && is_oriented(&tris));
        for r in &b.ribs {
            assert_close(r.depth, r.height * reach, 1e-6, "depth");
            assert!(r.tines >= 3 && r.th == 2.4, "gauge or minimum grip changed");
        }
    }
    for r in &legacy.ribs {
        assert_close(r.depth, 60.0, 1e-6, "legacy depth");
    }
    let short = block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 150.0);
    let res = analyze(&short, 45.0, &ID);
    assert!(
        build_sway_braces(&short, &res, &ID, &opts(), &cfg)
            == build_sway_braces(
                &short,
                &res,
                &ID,
                &SwayOpts {
                    uncapped_depth: true,
                    ..opts()
                },
                &cfg
            ),
        "short brace unnecessarily changes"
    );
}

// sway_depth.test.js:31
#[test]
fn sway_depth_expanded_draw_foot_still_refuses_a_wall_beyond_the_old_60_mm_footprint() {
    let cfg = Config::default();
    let topo = post();
    let result = analyze(&topo, 45.0, &ID);
    let face = side(&topo, -1.0, -60.0);
    let none = Avoid::default();
    let normal = sway_at_face(
        &topo,
        &result,
        &ID,
        face,
        [0.0, -60.0, 1000.0],
        &opts(),
        &none,
        &cfg,
    );
    let full_opts = SwayOpts {
        uncapped_depth: true,
        ..opts()
    };
    let full = sway_at_face(
        &topo,
        &result,
        &ID,
        face,
        [0.0, -60.0, 1000.0],
        &full_opts,
        &none,
        &cfg,
    );
    let (normal, full) = (normal.unwrap(), full.unwrap());
    let p: Vec<f64> = (0..3)
        .map(|i| full.foot[0][i] + 0.85 * (full.foot[1][i] - full.foot[0][i]))
        .collect();
    let wall = vec![[p[0] - 10.0, p[1], 0.0], [p[0] + 10.0, p[1], 0.0]];
    assert!(
        !sway_clashes_wall(&normal, std::slice::from_ref(&wall), &cfg)
            && sway_clashes_wall(&full, std::slice::from_ref(&wall), &cfg)
    );
    let blocked = sway_at_face(
        &topo,
        &result,
        &ID,
        face,
        [0.0, -60.0, 1000.0],
        &full_opts,
        &Avoid {
            braces: &[],
            walls: std::slice::from_ref(&wall),
        },
        &cfg,
    );
    assert!(
        blocked.err().is_some_and(|r| r.contains("would fuse")),
        "expanded foot ignores existing wall"
    );
}

// sway_depth.test.js:43
#[test]
fn sway_depth_deeper_braces_facing_across_a_channel_still_refuse_overlapping_supports() {
    let cfg = Config::default();
    let mut pos = block(-40.0, 40.0, -160.0, -100.0, 0.0, 1000.0);
    pos.extend(block(-40.0, 40.0, 100.0, 160.0, 0.0, 1000.0));
    let topo = build_topology(pos);
    let result = analyze(&topo, 45.0, &ID);
    let a = side(&topo, 1.0, -100.0);
    let b = side(&topo, -1.0, 100.0);
    let full = SwayOpts {
        uncapped_depth: true,
        ..opts()
    };
    let first = sway_at_face(
        &topo,
        &result,
        &ID,
        a,
        [0.0, -100.0, 1000.0],
        &full,
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let across = sway_at_face(
        &topo,
        &result,
        &ID,
        b,
        [0.0, 100.0, 1000.0],
        &full,
        &Avoid {
            braces: std::slice::from_ref(&first),
            walls: &[],
        },
        &cfg,
    );
    assert!(
        across.err().is_some_and(|r| r.contains("another brace")),
        "opposing deeper braces overlap"
    );
}

// sway_depth.test.js:53
#[test]
fn sway_depth_a_model_obstacle_in_the_expanded_footprint_is_still_rejected() {
    let cfg = Config::default();
    let mut pos = block(-80.0, 80.0, -60.0, 60.0, 0.0, 1000.0);
    pos.extend(block(-5.0, 5.0, -200.0, -170.0, 0.0, 50.0));
    let topo = build_topology(pos);
    let result = analyze(&topo, 45.0, &ID);
    let face = side(&topo, -1.0, -60.0);
    let none = Avoid::default();
    assert!(
        sway_at_face(
            &topo,
            &result,
            &ID,
            face,
            [0.0, -60.0, 1000.0],
            &opts(),
            &none,
            &cfg
        )
        .is_ok(),
        "control brace is blocked"
    );
    let full = sway_at_face(
        &topo,
        &result,
        &ID,
        face,
        [0.0, -60.0, 1000.0],
        &SwayOpts {
            uncapped_depth: true,
            ..opts()
        },
        &none,
        &cfg,
    );
    assert!(full.is_err(), "expanded brace passes through obstacle");
}

// sway_depth.test.js:61
#[test]
fn sway_depth_an_overhead_shoulder_shortens_the_rib_and_recomputes_depth_from_its_actual_height() {
    let cfg = Config::default();
    let mut pos = block(-80.0, 80.0, -60.0, 60.0, 0.0, 1000.0);
    pos.extend(block(-8.0, 8.0, -150.0, -60.0, 300.0, 310.0));
    let topo = build_topology(pos);
    let result = analyze(&topo, 45.0, &ID);
    let face = side(&topo, -1.0, -60.0);
    let r = sway_at_face(
        &topo,
        &result,
        &ID,
        face,
        [0.0, -60.0, 400.0],
        &SwayOpts {
            reach: Some(0.5),
            uncapped_depth: true,
            ..opts()
        },
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        r.height < 300.0 && r.height > 200.0,
        "shoulder did not shorten the rib"
    );
    assert_close(
        r.depth,
        r.height * 0.5,
        1e-6,
        "depth still uses original part height",
    );
    let _ = pts;
}
