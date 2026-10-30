//! Port of upstream `tests/small_holes.test.js` and `tests/small_holes_i18n.test.js`' engine side.
//! It lives in sf-cli's tests because it also goes through the plugins' adapter (`computeFins`,
//! `drawWall`). `computeFins`' `overFaces` / `smallFaces` are not on `sf_cli::entry::FinsResult`;
//! they are derived here as fins_entry.js:184-188 does, from a build of the same seated soup.
use sf_cli::entry::{EngineOptions, compute_fins, seat_soup, snap};
use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::fins::coverage::{
    HOLE, automatic_targets, classify, classify_targets, touches_small_hole,
};
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::planes::find_wall_patches;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;
use sf_testkit::{CircularBore, block, circular_bore};
use std::sync::Arc;

const I: [f64; 9] = IDENTITY3;

fn topology(pos: Vec<f64>) -> Arc<Topology> {
    Arc::new(build_topology(pos))
}
fn bore(d: f64) -> Vec<f64> {
    circular_bore(&CircularBore {
        diameter: d,
        ..Default::default()
    })
}
fn opts(mode: &str) -> FinsOpts {
    FinsOpts {
        mode: Some(mode.into()),
        bed_pad: Some(true),
        ..Default::default()
    }
}
fn run(topo: &Arc<Topology>, res: &Analysis, o: &FinsOpts) -> FinsBuilt {
    build_fins(topo, res, &I, o, &Config::default())
}
fn engine(mode: &str) -> EngineOptions {
    EngineOptions {
        mode: Some(mode.into()),
        threshold: Some(45.0),
        ..Default::default()
    }
}
fn flat(parts: &[&[f64]]) -> Vec<f64> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}
/// `computeFins`' `overFaces` / `smallFaces` (fins_entry.js:184-188) for these input faces.
fn face_sets(positions: &[f64], mode: &str) -> (Vec<usize>, Vec<usize>) {
    let (pos, _) = seat_soup(positions).unwrap();
    let topo = topology(pos);
    let res = analyze(&topo, 45.0, &I);
    let built = run(&topo, &res, &opts(mode));
    let (mut over, mut small) = (vec![], vec![]);
    for f in 0..res.over.len() {
        let masked = built.small_hole_mask.as_ref().is_some_and(|m| m[f] != 0);
        if res.kept[f] != 0 && !masked {
            over.push(f);
        } else if res.over[f] != 0 {
            small.push(f);
        }
    }
    (over, small)
}

// small_holes.test.js:14
#[test]
fn small_holes_auto_and_full_leave_a_recognised_circular_bore_clear() {
    for mode in ["auto", "full"] {
        for diameter in [3.0, 5.0] {
            let topo = topology(bore(diameter));
            let res = analyze(&topo, 45.0, &I);
            let (over, kept, regions) = (res.over.clone(), res.kept.clone(), res.regions.clone());
            assert!(classify(&topo, &res, &I).contains(&HOLE));
            let o = FinsOpts {
                curved_walls: Some(true),
                ..opts(mode)
            };
            let built = run(&topo, &res, &o);
            assert_eq!(built.small_hole_regions, 1);
            assert_eq!(built.props.len(), 0);
            assert_eq!(built.triangles.len(), 0);
            assert_eq!(
                built.unserved, 0.0,
                "an intentional exclusion is not a failed placement"
            );
            assert_eq!(
                built.small_hole_mask.as_ref(),
                Some(&automatic_targets(&topo, &res, &I).excluded_support_faces)
            );
            assert_eq!(
                res.over, over,
                "the original overhang highlighting must survive"
            );
            assert_eq!(res.kept, kept);
            assert!(
                res.regions == regions,
                "the caller owns its original regions"
            );
        }
    }
}

// :37
#[test]
fn small_holes_larger_circular_bores_retain_automatic_support() {
    let topo = topology(bore(10.0));
    let res = analyze(&topo, 45.0, &I);
    for mode in ["auto", "full"] {
        let built = run(&topo, &res, &opts(mode));
        assert_eq!(built.small_hole_regions, 0);
        assert!(
            !built
                .small_hole_mask
                .as_ref()
                .unwrap()
                .iter()
                .any(|&b| b != 0)
        );
        assert!(
            built.props.iter().any(|p| p.in_bore == Some(true)),
            "{mode}: missing larger-bore support"
        );
    }
}

// :48
#[test]
fn small_holes_reuse_the_existing_classifier_boundary_rather_than_adding_a_diameter_rule() {
    for (diameter, threshold, count) in [
        (5.9, 45.0, 1),
        (6.0, 45.0, 1),
        (6.1, 45.0, 0),
        (4.0, 70.0, 1),
        (5.0, 70.0, 0),
    ] {
        let topo = topology(bore(diameter));
        let res = analyze(&topo, threshold, &I);
        assert_eq!(
            automatic_targets(&topo, &res, &I).small_hole_regions,
            count,
            "{diameter} mm at {threshold}°"
        );
    }
}

// :56
#[test]
fn small_holes_one_generation_shares_its_classification_without_caching_a_later_pose() {
    let topo = topology(circular_bore(&CircularBore::default()));
    let res = analyze(&topo, 45.0, &I);
    let targets = automatic_targets(&topo, &res, &I);
    assert_eq!(classify_targets(&targets), targets.coverage_classes);
    assert_eq!(targets.analysis.regions.len(), 0);
    assert!(
        !targets.excluded_support_volumes.is_empty(),
        "Full reserves the cavity without emitting it"
    );
    let lowest = targets
        .excluded_support_volumes
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    assert!(
        lowest >= 7.49 + res.offset.z && lowest < 8.0 + res.offset.z,
        "the keep-clear volume ends at the actual bore floor"
    );
    let in_hole = [res.offset.x, res.offset.y, 12.5 + res.offset.z];
    assert!(
        touches_small_hole(&targets, &[in_hole]),
        "a later pass must reject the exempt ceiling"
    );
    assert!(
        !touches_small_hole(&targets, &[[in_hole[0], in_hole[1], in_hole[2] + 10.0]]),
        "a separate ceiling above the bore must remain available"
    );
    let wedges = find_wall_patches(
        &topo,
        &I,
        &targets.analysis.offset,
        None,
        Some(&targets.excluded_support_faces),
    );
    for p in &wedges {
        assert!(
            p.faces
                .iter()
                .all(|&f| targets.excluded_support_faces[f] == 0)
        );
    }
    // Turn the bore upright: the same topology and analyze scratch are reused.
    let rot = [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0];
    let upright = analyze(&topo, 45.0, &rot);
    let built = build_fins(&topo, &upright, &rot, &opts("auto"), &Config::default());
    assert_eq!(built.small_hole_regions, 0);
    assert!(
        !built
            .small_hole_mask
            .as_ref()
            .unwrap()
            .iter()
            .any(|&b| b != 0)
    );
}

// :86
#[test]
fn small_holes_draw_does_not_apply_the_automatic_exclusion() {
    let topo = topology(circular_bore(&CircularBore::default()));
    let res = analyze(&topo, 45.0, &I);
    let built = run(&topo, &res, &opts("draw"));
    assert_eq!(built.small_hole_regions, 0);
    assert!(built.small_hole_mask.is_none());
    assert!(!res.regions.is_empty());
    // drawWall(circularBore(), [-12, 0, 12.5], [12, 0, 12.5]): draw_entry.js:27-58 on the engine
    let (pos, shift) = seat_soup(&circular_bore(&CircularBore::default())).unwrap();
    let topo = topology(pos.clone());
    let r = analyze(&topo, 45.0, &I);
    let off = r.offset;
    let tris: Vec<f64> = pos
        .chunks(3)
        .flat_map(|v| [v[0] + off.x, v[1] + off.y, v[2] + off.z])
        .collect();
    let seat = |p: [f64; 3]| {
        [
            snap(p[0] - shift.x) + off.x,
            snap(p[1] - shift.y) + off.y,
            snap(p[2] - shift.z) + off.z,
        ]
    };
    let cfg = Config::default();
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let d = drawn_wall(
        seat([-12.0, 0.0, 12.5]),
        seat([12.0, 0.0, 12.5]),
        &Arc::new(Tris::new(tris)),
        0.0,
        &DrawOpts {
            tines: true,
            topo: Some(&topo),
            rot: Some(I),
            offset: Some(off),
            brace_tall: true,
            ..Default::default()
        },
        &mut ctx,
    );
    let DrawnWall::Ok(w) = d else {
        panic!("an explicitly drawn wall can still stand inside the bore")
    };
    assert!(
        w.part_attached == Some(true) && !w.tris.is_empty(),
        "an explicitly drawn wall can still stand inside the bore"
    );
}

// :100
#[test]
fn small_holes_an_open_sided_channel_remains_an_automatic_target() {
    let mut pos = block(-15.0, 15.0, -3.0, 3.0, 0.0, 2.0);
    pos.extend(block(-15.0, 15.0, -3.0, -2.0, 2.0, 6.0));
    pos.extend(block(-15.0, 15.0, -3.0, 3.0, 6.0, 8.0));
    for mode in ["auto", "full"] {
        let topo = topology(pos.clone());
        let res = analyze(&topo, 45.0, &I);
        let built = run(&topo, &res, &opts(mode));
        assert_eq!(built.small_hole_regions, 0);
        assert!(
            !built.props.is_empty(),
            "the accessible channel must keep its supports"
        );
    }
}

// :115
#[test]
fn small_holes_keep_neighbouring_overhangs_supported_and_report_through_the_shared_adapter() {
    let pos = flat(&[
        &circular_bore(&CircularBore::default()),
        &block(30.0, 60.0, -10.0, 10.0, 10.0, 15.0),
    ]);
    for mode in ["auto", "full"] {
        let topo = topology(pos.clone());
        let res = analyze(&topo, 45.0, &I);
        let built = run(&topo, &res, &opts(mode));
        assert_eq!(built.small_hole_regions, 1);
        assert!(
            !built.props.is_empty(),
            "the neighbouring ceiling must keep its supports"
        );
        for p in &built.triangles {
            let (x, y, z) = (
                p[0] - res.offset.x,
                p[1] - res.offset.y,
                p[2] - res.offset.z,
            );
            assert!(
                !(x.abs() < 14.9 && y.hypot(z - 10.0) < 2.49),
                "a later pass put a fin in the hole"
            );
        }
        let plugin = compute_fins(&pos, &engine(mode)).unwrap();
        assert_eq!(plugin.stats.small_hole_regions, 1);
        assert_eq!(plugin.stats.braces + plugin.stats.props, built.props.len());
        let (over, small) = face_sets(&pos, mode);
        assert!(!over.is_empty(), "the neighbouring overhang remains red");
        assert!(!small.is_empty(), "the excluded ceiling is amber");
        assert!(
            over.iter().all(|f| !small.contains(f)),
            "red and amber never overlap"
        );
        for f in 0..res.over.len() {
            let want = res.over[f] != 0
                && (res.kept[f] == 0 || built.small_hole_mask.as_ref().unwrap()[f] != 0);
            assert_eq!(small.contains(&f), want);
        }
    }
}

// :143
#[test]
fn small_holes_full_keeps_nearby_ceilings_supported_without_routing_through_the_bore() {
    let pos = flat(&[
        &circular_bore(&CircularBore::default()),
        &block(16.0, 40.0, -10.0, 10.0, 10.0, 15.0),
        &block(-40.0, -16.0, -10.0, 10.0, 10.0, 15.0),
    ]);
    let topo = topology(pos);
    let result = analyze(&topo, 45.0, &I);
    let o = FinsOpts {
        coverage: Some(1.0),
        ..opts("full")
    };
    let built = run(&topo, &result, &o);
    assert_eq!(built.small_hole_regions, 1);
    assert!(
        built
            .props
            .iter()
            .any(|p| p.line.iter().any(|q| q[0] > 16.0 + result.offset.x))
    );
    assert!(
        built
            .props
            .iter()
            .any(|p| p.line.iter().any(|q| q[0] < -16.0 + result.offset.x))
    );
    for p in &built.triangles {
        let (x, y, z) = (
            p[0] - result.offset.x,
            p[1] - result.offset.y,
            p[2] - result.offset.z,
        );
        assert!(
            !(x.abs() < 14.9 && y.hypot(z - 10.0) < 2.49),
            "Full routed a wall through the excluded cavity"
        );
    }
    let bare = built.bare.as_ref().unwrap();
    assert!(
        bare.area < 0.02 * bare.total,
        "neighbouring ceilings must retain coverage"
    );
}

// A CAD export may split a bore lengthwise at its centre. A containment ray there can hit two
// triangles of one surface; that must not turn air into solid.
fn seam_bore(
    diameter: f64,
    segments: usize,
    axial_segments: usize,
    yaw: u32,
    translated: bool,
) -> Vec<f64> {
    let mut pos = circular_bore(&CircularBore {
        diameter,
        segments,
        axial_segments,
        ..Default::default()
    });
    let (c, s) = if yaw == 90 { (0.0, 1.0) } else { (1.0, 0.0) };
    for v in pos.chunks_mut(3) {
        let (x, y) = (v[0], v[1]);
        v[0] = c * x - s * y + if translated { 30.0 } else { 0.0 };
        v[1] = s * x + c * y + if translated { 110.0 } else { 0.0 };
    }
    pos
}

// :178
#[test]
fn small_holes_coincident_mesh_seams_do_not_change_classification_across_seating_paths() {
    for segments in [64, 96] {
        for axial in [1, 2] {
            for yaw in [0, 90] {
                for translated in [false, true] {
                    let pos = seam_bore(4.0, segments, axial, yaw, translated);
                    let label = format!(
                        "{segments} facets, {axial} axial strips, {yaw}°, translated={translated}"
                    );
                    let topo = topology(pos.clone());
                    let res = analyze(&topo, 45.0, &I);
                    assert_eq!(
                        automatic_targets(&topo, &res, &I).small_hole_regions,
                        1,
                        "{label}"
                    );
                    let plugin = compute_fins(&pos, &engine("auto")).unwrap();
                    assert_eq!(
                        plugin.stats.small_hole_regions, 1,
                        "shared adapter: {label}"
                    );
                    assert_eq!(plugin.triangles.len(), 0, "shared adapter: {label}");
                    let (over, small) = face_sets(&pos, "auto");
                    assert_eq!(over.len(), 0, "red exempt ceiling: {label}");
                    assert!(!small.is_empty(), "missing amber: {label}");
                }
            }
        }
    }
}

// :195
#[test]
fn small_holes_seam_tessellated_bores_stay_empty_in_auto_and_full_with_larger_controls_supported() {
    for mode in ["auto", "full"] {
        for diameter in [4.0, 5.0, 5.8, 10.0] {
            let pos = seam_bore(diameter, 64, 2, 90, true);
            let topo = topology(pos.clone());
            let res = analyze(&topo, 45.0, &I);
            let o = FinsOpts {
                curved_walls: Some(true),
                ..opts(mode)
            };
            let built = run(&topo, &res, &o);
            let plugin = compute_fins(&pos, &engine(mode)).unwrap();
            let excluded = usize::from(diameter < 6.0);
            assert_eq!(built.small_hole_regions, excluded, "{mode}: {diameter} mm");
            assert_eq!(
                plugin.stats.small_hole_regions, excluded,
                "shared {mode}: {diameter} mm"
            );
            if excluded == 1 {
                assert_eq!(built.triangles.len(), 0);
                assert_eq!(plugin.triangles.len(), 0);
            } else {
                assert!(
                    !built.props.is_empty(),
                    "large-bore control must retain support"
                );
                assert!(plugin.stats.braces + plugin.stats.props > 0);
            }
        }
    }
}
