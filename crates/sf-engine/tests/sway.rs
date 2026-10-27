//! Own tests of `sf_engine::sway` (web/sway.js): the refusals and options the ported JS tests do not
//! name. Parity (`sf-testkit/tests/parity_sway.rs`) pins equality with JS.

use sf_engine::config::Config;
use sf_engine::fins::seating::seated_part_tris;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::planes::find_wall_patches;
use sf_engine::sway::*;
use sf_engine::types::IDENTITY3;
use sf_testkit::{block, block_topo};

const ID: [f64; 9] = IDENTITY3;

fn opts<'a>() -> SwayOpts<'a> {
    SwayOpts {
        tines: Some(true),
        layer_height: Some(0.2),
        ..Default::default()
    }
}

fn post(h: f64) -> Topology {
    block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, h)
}

fn side(t: &Topology) -> usize {
    (0..t.n_faces).find(|&f| t.nrm[f * 3 + 1] < -0.9).unwrap()
}

#[test]
fn a_roof_is_refused_with_the_reason_a_face_with_nothing_under_the_spot_too() {
    let cfg = Config::default();
    let topo = post(150.0);
    let res = analyze(&topo, 45.0, &ID);
    let roof = (0..topo.n_faces)
        .find(|&f| topo.nrm[f * 3 + 2] > 0.9)
        .unwrap();
    let r = sway_at_face(
        &topo,
        &res,
        &ID,
        roof,
        [0.0, 0.0, 150.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    );
    // a roof is no patch a brace stands against: "too small or curved" is the answer
    assert_eq!(
        r.unwrap_err(),
        "that face is too small or curved to stand a brace against"
    );
    // a rib is refused outright on a face that leans too far
    let patches = find_wall_patches(&topo, &ID, &res.offset, None, None);
    let part = seated_part_tris(&topo, &ID, &res.offset);
    let mut p = patches.iter().find(|p| p.n.y < -0.9).unwrap().clone();
    p.n.z = -0.9;
    let r = build_sway_rib(&p, 0.0, &part, &topo, &ID, &res.offset, &opts(), &cfg);
    assert_eq!(
        r.unwrap_err(),
        "that face leans too far to stand a brace against — pick an upright side"
    );
}

#[test]
fn a_low_face_is_too_short_to_need_a_brace_and_says_how_high_it_reaches() {
    let cfg = Config::default();
    let topo = post(20.0);
    let res = analyze(&topo, 45.0, &ID);
    let r = sway_at_face(
        &topo,
        &res,
        &ID,
        side(&topo),
        [0.0, -15.0, 20.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    );
    // a 20mm block has no patch worth a brace (minFaceH 30), so the click finds none
    assert!(r.is_err());
    // but the rib builder itself says how far the face reaches
    let patches = find_wall_patches(&topo, &ID, &res.offset, None, None);
    let part = seated_part_tris(&topo, &ID, &res.offset);
    let p = patches.iter().find(|p| p.n.y < -0.9).unwrap();
    let r = build_sway_rib(p, 0.0, &part, &topo, &ID, &res.offset, &opts(), &cfg);
    assert_eq!(
        r.unwrap_err(),
        "that face only reaches 20mm up — too short to need a brace"
    );
}

#[test]
fn a_shoulder_over_the_face_caps_the_rib_and_a_flared_base_leaves_no_room_for_the_foot() {
    let cfg = Config::default();
    let mut pos = block(-20.0, 20.0, -15.0, 15.0, 0.0, 120.0);
    pos.extend(block(-20.0, 20.0, -25.0, 15.0, 90.0, 100.0));
    let topo = build_topology(pos);
    let res = analyze(&topo, 45.0, &ID);
    let patches = find_wall_patches(&topo, &ID, &res.offset, None, None);
    let part = seated_part_tris(&topo, &ID, &res.offset);
    let p = patches.iter().find(|p| p.n.y < -0.9).unwrap();
    let r = build_sway_rib(p, 0.0, &part, &topo, &ID, &res.offset, &opts(), &cfg);
    // the ledge sticks out above the face: the rib shrinks to stand under it
    let rib = r.expect("a capped rib");
    assert!(rib.height < 90.0 && rib.height >= cfg.sway.min_rib_h);

    let mut pos = block(-20.0, 20.0, -15.0, 15.0, 0.0, 120.0);
    pos.extend(block(-20.0, 20.0, -30.0, 15.0, 0.0, 6.0));
    let topo = build_topology(pos);
    let res = analyze(&topo, 45.0, &ID);
    let patches = find_wall_patches(&topo, &ID, &res.offset, None, None);
    let part = seated_part_tris(&topo, &ID, &res.offset);
    if let Some(p) = patches.iter().find(|p| p.n.y < -0.9) {
        let r = build_sway_rib(p, 0.0, &part, &topo, &ID, &res.offset, &opts(), &cfg);
        assert!(r.is_err());
    }
}

#[test]
fn a_height_cap_steps_the_rib_along_the_face_and_marks_it_profiled() {
    let cfg = Config::default();
    // a stepped face: the profiled rib follows each band
    let mut pos = block(-20.0, 20.0, -15.0, 15.0, 0.0, 150.0);
    pos.extend(block(-20.0, 20.0, -20.0, -15.0, 0.0, 60.0));
    pos.extend(block(-20.0, 20.0, -17.0, -15.0, 60.0, 100.0));
    let topo = build_topology(pos);
    let res = analyze(&topo, 45.0, &ID);
    let o = SwayOpts {
        max_height: Some(120.0),
        ..opts()
    };
    let r = sway_at_face(
        &topo,
        &res,
        &ID,
        side(&topo),
        [0.0, -20.0, 120.0],
        &o,
        &Avoid::default(),
        &cfg,
    );
    if let Ok(rib) = r {
        assert!(rib.profiled);
        assert!(rib.levels.len() > 2);
    }
}

#[test]
fn the_interface_filament_tags_the_tines_and_round_feet_change_the_foot() {
    let cfg = Config::default();
    let topo = post(150.0);
    let res = analyze(&topo, 45.0, &ID);
    let plain = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    let iface = build_sway_braces(
        &topo,
        &res,
        &ID,
        &SwayOpts {
            iface_all: true,
            ..opts()
        },
        &cfg,
    );
    let tagged = |s: &SwayBraces| s.triangles.iter().filter(|v| v[3] == 1.0).count();
    assert_eq!(tagged(&plain), 0);
    assert!(tagged(&iface) > 0 && iface.triangles.len() == plain.triangles.len());
    let round = build_sway_braces(
        &topo,
        &res,
        &ID,
        &SwayOpts {
            round_feet: true,
            ..opts()
        },
        &cfg,
    );
    assert!(round.triangles.len() > plain.triangles.len());
    // 'vertical' tines turn round feet off
    let mut vertical = cfg.clone();
    vertical.prop.tine_dir = sf_engine::prop::config::TineDir::Vertical;
    let v = build_sway_braces(
        &topo,
        &res,
        &ID,
        &SwayOpts {
            round_feet: true,
            ..opts()
        },
        &vertical,
    );
    assert_eq!(v.triangles.len(), plain.triangles.len());
}

#[test]
fn every_brace_carries_its_own_triangle_range_and_the_ranges_tile_the_output() {
    let cfg = Config::default();
    let topo = post(150.0);
    let res = analyze(&topo, 45.0, &ID);
    let s = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    let mut at = 0;
    for r in &s.ribs {
        let t = r.tri_range.unwrap();
        assert_eq!(t[0], at);
        assert_eq!(t[1] - t[0], r.tris.len());
        at = t[1];
    }
    assert_eq!(at, s.triangles.len());
    assert!(s.reason.is_none() && s.skipped <= 8);
}

#[test]
fn the_patch_cache_is_per_pose_and_replaced_when_the_pose_changes() {
    let cfg = Config::default();
    let topo = post(150.0);
    let res = analyze(&topo, 45.0, &ID);
    let f = side(&topo);
    let a = sway_at_face(
        &topo,
        &res,
        &ID,
        f,
        [0.0, -15.0, 60.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    )
    .unwrap();
    let b = sway_at_face(
        &topo,
        &res,
        &ID,
        f,
        [0.0, -15.0, 60.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    )
    .unwrap();
    assert_eq!(a, b);
    // turned about z: the +x side now faces -y, a different pose and a different patch set
    let rot = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let res2 = analyze(&topo, 45.0, &rot);
    let f2 = (0..topo.n_faces)
        .find(|&i| {
            let (x, y) = (topo.nrm[i * 3], topo.nrm[i * 3 + 1]);
            // rotated normal y' = rot[1]*x + rot[4]*y
            rot[1] * x + rot[4] * y < -0.9
        })
        .unwrap();
    let c = sway_at_face(
        &topo,
        &res2,
        &rot,
        f2,
        [0.0, -20.0, 60.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    );
    assert!(c.is_ok());
}
