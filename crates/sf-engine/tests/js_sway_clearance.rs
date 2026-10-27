//! Port of upstream `tests/sway_clearance.test.js` (web/sway.js `swayAtFace`, `swayClashes`,
//! `SWAY.clearance`).

use sf_engine::config::Config;
use sf_engine::fins::build::SwayLevel;
use sf_engine::overhangs::analyze;
use sf_engine::sway::*;
use sf_testkit::{block_topo, is_closed, is_oriented};

fn side(topo: &sf_engine::overhangs::Topology, sign: f64) -> usize {
    (0..topo.n_faces)
        .find(|&f| topo.nrm[3 * f + 1] * sign > 0.9)
        .unwrap()
}

// sway_clearance.test.js:6
#[test]
fn sway_clearance_opposite_sides_of_a_thin_wall_accept_both_braces_rotated_and_with_cross_ribs() {
    let cfg = Config::default();
    for width in [2.0, 6.0] {
        for angle in [0.0, 45.0, 90.0] {
            for cross in [false, true] {
                let a = angle * std::f64::consts::PI / 180.0;
                let (c, s) = (a.cos(), a.sin());
                let rot = [c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0];
                let topo = block_topo(-50.0, 50.0, -width / 2.0, width / 2.0, 0.0, 200.0);
                let result = analyze(&topo, 45.0, &rot);
                let point = |sign: f64| [-s * sign * width / 2.0, c * sign * width / 2.0, 150.0];
                let opts = SwayOpts {
                    full: true,
                    tines: Some(true),
                    layer_height: Some(0.2),
                    cross_above: cross.then_some(100.0),
                    cross_rise: cross.then_some(0.5),
                    ..Default::default()
                };
                let none = Avoid::default();
                let first = sway_at_face(
                    &topo,
                    &result,
                    &rot,
                    side(&topo, -1.0),
                    point(-1.0),
                    &opts,
                    &none,
                    &cfg,
                );
                let alone = sway_at_face(
                    &topo,
                    &result,
                    &rot,
                    side(&topo, 1.0),
                    point(1.0),
                    &opts,
                    &none,
                    &cfg,
                );
                let (first, _alone) = (first.expect("first"), alone.expect("alone"));
                let with_first = Avoid {
                    braces: std::slice::from_ref(&first),
                    walls: &[],
                };
                let next = sway_at_face(
                    &topo,
                    &result,
                    &rot,
                    side(&topo, 1.0),
                    point(1.0),
                    &opts,
                    &with_first,
                    &cfg,
                )
                .unwrap_or_else(|e| panic!("{width}mm/{angle}deg/cross={cross}: {e}"));
                assert!(
                    !sway_clashes(&first, std::slice::from_ref(&next), &cfg)
                        && !sway_clashes(&next, std::slice::from_ref(&first), &cfg)
                );
                let pts: Vec<[f64; 3]> = next.tris.iter().map(|v| [v[0], v[1], v[2]]).collect();
                assert!(is_closed(&pts) && is_oriented(&pts));
                assert!(next.tines >= 3);
                let duplicate = sway_at_face(
                    &topo,
                    &result,
                    &rot,
                    side(&topo, -1.0),
                    point(-1.0),
                    &opts,
                    &with_first,
                    &cfg,
                );
                assert!(duplicate.is_err(), "duplicate brace accepted");
            }
        }
    }
}

// sway_clearance.test.js:25
#[test]
fn sway_clearance_rectangular_section_overlap_and_sub_clearance_remain_clashes() {
    let cfg = Config::default();
    let clearance = cfg.sway.clearance;
    let rib = |x0: f64, x1: f64, y: f64, h: f64| SwayRib {
        tris: vec![],
        tines: 0,
        height: h,
        depth: 0.0,
        th: 1.0,
        stilt: 0.0,
        foot: [[x0, y, 0.0], [x1, y, 0.0]],
        half_w: 2.0,
        levels: vec![
            SwayLevel {
                z: 0.0,
                a: [x0, y, 0.0],
                b: [x1, y, 0.0],
            },
            SwayLevel {
                z: h,
                a: [x0, y, h],
                b: [x1, y, h],
            },
        ],
        cross: 0,
        arms: vec![],
        profiled: false,
        tri_range: None,
    };
    let a = rib(0.0, 20.0, 0.0, 200.0);
    assert!(
        sway_clashes(&a, &[rib(5.0, 25.0, 3.0, 200.0)], &cfg),
        "side-by-side feet overlap"
    );
    assert!(
        sway_clashes(&a, &[rib(20.0 + clearance / 2.0, 40.0, 0.0, 200.0)], &cfg),
        "end gap below clearance"
    );
    assert!(
        !sway_clashes(&a, &[rib(20.0 + clearance + 0.1, 40.0, 0.0, 200.0)], &cfg),
        "clear square ends refused"
    );
    let mut b = rib(0.0, 20.0, 10.0, 200.0);
    b.levels[1] = SwayLevel {
        z: 200.0,
        a: [0.0, 0.0, 200.0],
        b: [20.0, 0.0, 200.0],
    };
    assert!(
        sway_clashes(&a, std::slice::from_ref(&b), &cfg),
        "leaning bodies intersect above clear feet"
    );
    b.arms = vec![sf_engine::fins::build::SwayArm {
        a: [0.0; 3],
        b: [0.0; 3],
        th: 0.0,
        plan: vec![[-1.0, -1.0], [2.0, -1.0], [2.0, 1.0], [-1.0, 1.0]],
    }];
    assert!(
        sway_clashes(&a, std::slice::from_ref(&b), &cfg),
        "cross arm intersection ignored"
    );
}
