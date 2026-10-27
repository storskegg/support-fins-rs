//! Port of upstream `tests/sway_cross_gauge.test.js` (web/sway.js cross ribs, web/prop/brace.js
//! `braceCrossRib`). The third case (a metre-high post through the real `finworker`, STL and 3MF
//! export) needs those modules and is ported with them.

use sf_engine::config::Config;
use sf_engine::nozzle::{apply_nozzle, apply_wall_lines};
use sf_engine::overhangs::analyze;
use sf_engine::prop::brace::brace_cross_rib;
use sf_engine::sway::*;
use sf_engine::tris::Tris;
use sf_testkit::{assert_close, block, block_topo, inside_count, is_closed, is_oriented};

const ID: [f64; 9] = sf_engine::types::IDENTITY3;

fn span(values: impl Iterator<Item = f64>) -> f64 {
    let v: Vec<f64> = values.collect();
    v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - v.iter().cloned().fold(f64::INFINITY, f64::min)
}

fn pts(t: &[[f64; 4]]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

// sway_cross_gauge.test.js:13
#[test]
fn sway_cross_actual_brace_gauge_and_flange_margin_at_small_large_nozzles_and_wall_counts() {
    let topo = block_topo(-80.0, 80.0, -60.0, 60.0, 0.0, 1000.0);
    let result = analyze(&topo, 45.0, &ID);
    let face = (0..topo.n_faces)
        .find(|&f| topo.nrm[3 * f + 1] < -0.9)
        .unwrap();
    for nozzle in [0.2, 0.4, 0.8, 1.6, 2.0] {
        for lines in [2.0, 4.0, 8.0] {
            let mut cfg = Config::default();
            apply_nozzle(&mut cfg, nozzle);
            apply_wall_lines(&mut cfg, lines);
            let opts = SwayOpts {
                full: true,
                tines: Some(true),
                layer_height: Some(0.6),
                ..Default::default()
            };
            let none = Avoid::default();
            let plain = sway_at_face(
                &topo,
                &result,
                &ID,
                face,
                [0.0, -60.0, 1000.0],
                &opts,
                &none,
                &cfg,
            )
            .unwrap_or_else(|e| panic!("{e}"));
            let r = sway_at_face(
                &topo,
                &result,
                &ID,
                face,
                [0.0, -60.0, 1000.0],
                &SwayOpts {
                    cross_above: Some(100.0),
                    cross_rise: Some(0.5),
                    ..opts.clone()
                },
                &none,
                &cfg,
            )
            .unwrap_or_else(|e| panic!("{e}"));
            assert!(r.cross == 2, "{nozzle}/{lines}: cross count {}", r.cross);
            assert!(
                r.tines == plain.tines && r.th == plain.th && r.height == plain.height,
                "main brace changed"
            );
            let extra = &r.tris[plain.tris.len()..];
            let ex = pts(extra);
            assert!(
                is_closed(&ex) && is_oriented(&ex),
                "cross solids open or reversed"
            );
            let lifted: Vec<[f64; 3]> = ex.iter().map(|v| [v[0], v[1], v[2].max(0.001)]).collect();
            assert!(inside_count(&topo, &ID, &result.offset, &lifted) == 0);
            for i in 0..2 {
                let arm = &r.arms[i];
                let body = &extra[i * 60..i * 60 + 24];
                let foot = &extra[i * 60 + 24..(i + 1) * 60];
                assert_close(arm.th, r.th, 1e-9, "collision record uses a narrower gauge");
                // A -Y face: its cross arms run along X. Undo the outer edge's lean before
                // measuring the extrusion width in Y.
                let lean = (r.levels.last().unwrap().b[1] - r.levels[0].b[1]) / r.height;
                assert_close(
                    span(body.iter().map(|v| v[1] - lean * v[2])),
                    r.th,
                    1e-6,
                    "body gauge",
                );
                assert_close(
                    span(foot.iter().map(|v| v[1] - lean * v[2])),
                    r.th + 2.0 * (cfg.prop.foot_min - cfg.prop.th / 2.0),
                    1e-6,
                    "flange margin",
                );
            }
        }
    }
}

// sway_cross_gauge.test.js:40 (the JS also checks the global PROP is unchanged; here the config
// is passed by shared reference, so it cannot change)
#[test]
fn sway_cross_thicker_rib_rechecks_part_and_neighbouring_supports_without_changing_global_wall_gauge()
 {
    let cfg = Config::default();
    let obstacle = Tris::new(block(0.2, 0.8, 2.0, 2.5, 10.0, 20.0));
    let mut thin = Vec::new();
    let mut thick = Vec::new();
    assert!(
        brace_cross_rib(
            &mut thin,
            &obstacle,
            [0.0, 0.0],
            [1.0, 0.0],
            0.0,
            200.0,
            30.0,
            [0.0, 0.0],
            |_| true,
            0.5,
            1.0,
            &cfg
        )
        .is_some()
    );
    assert!(
        brace_cross_rib(
            &mut thick,
            &obstacle,
            [0.0, 0.0],
            [1.0, 0.0],
            0.0,
            200.0,
            30.0,
            [0.0, 0.0],
            |_| true,
            0.5,
            6.0,
            &cfg
        )
        .is_none(),
        "wide body ignores the part beyond the old gauge"
    );
    assert!(thick.is_empty());
    let checks = std::cell::Cell::new(0);
    assert!(
        brace_cross_rib(
            &mut Vec::new(),
            &Tris::new(Vec::new()),
            [0.0, 0.0],
            [1.0, 0.0],
            0.0,
            200.0,
            30.0,
            [0.0, 0.0],
            |mesh| {
                checks.set(checks.get() + 1);
                assert!(
                    span(mesh.iter().map(|v| v[1])) > 6.0,
                    "neighbour check sees the old narrow mesh"
                );
                false
            },
            0.5,
            6.0,
            &cfg
        )
        .is_none()
    );
    assert!(checks.get() > 0);
}
