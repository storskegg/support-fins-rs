//! Own tests of `sf_engine::draw` (web/draw.js): every refusal's message, each return shape of
//! `drawn_wall`, and `drawn_line`. Parity (recorded after the M3 pin moves) pins equality with JS.

mod common;
use common::ctx;
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::*;
use sf_engine::tris::Tris;
use sf_testkit::block;

fn tris_of(blocks: &[Vec<f64>]) -> Arc<Tris> {
    Arc::new(Tris::new(blocks.iter().flatten().copied().collect()))
}

/// A 60 x 20 slab with its underside at z = 20 (a shelf in the air).
fn shelf() -> Arc<Tris> {
    tris_of(&[block(-30.0, 30.0, -10.0, 10.0, 20.0, 24.0)])
}

/// The shelf standing on a 10 mm base block: its underside is over the part.
fn over_base() -> Arc<Tris> {
    tris_of(&[
        block(-30.0, 30.0, -10.0, 10.0, 0.0, 10.0),
        block(-30.0, 30.0, -10.0, 10.0, 12.0, 16.0),
    ])
}

fn reason(w: &DrawnWall) -> &str {
    match w {
        DrawnWall::Refused { reason } => reason,
        DrawnWall::Ok(_) => panic!("built a wall"),
    }
}

#[test]
fn constants() {
    assert_eq!(DRAW_MIN_LEN, 2.0);
}

#[test]
fn a_chord_under_the_minimum_is_refused_with_its_length() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    let r = drawn_wall(
        [0.0, 0.0, 20.0],
        [1.26, 0.0, 20.0],
        &shelf(),
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    assert_eq!(reason(&r), "wall too short — 1.3mm, needs 2mm");
    assert!(!r.ok());
}

#[test]
fn a_curve_tighter_than_the_minimum_radius_is_refused() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    let opts = DrawOpts {
        through: Some([0.0, 2.0, 20.0]),
        ..Default::default()
    };
    let r = drawn_wall(
        [-3.0, 0.0, 20.0],
        [3.0, 0.0, 20.0],
        &shelf(),
        0.0,
        &opts,
        &mut c,
    );
    assert!(reason(&r).starts_with("curve too tight — "), "{r:?}");
    assert!(reason(&r).ends_with("mm radius, needs 5mm"));
}

#[test]
fn a_through_point_in_line_with_the_ends_is_the_straight_wall() {
    let cfg = Config::default();
    let t = shelf();
    let mut c = ctx(&cfg);
    let a = drawn_wall(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &t,
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    let opts = DrawOpts {
        through: Some([5.0, 0.0, 20.0]),
        ..Default::default()
    };
    let b = drawn_wall(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &t,
        0.0,
        &opts,
        &mut c,
    );
    assert!(a.ok());
    assert_eq!(a, b);
}

#[test]
fn a_wall_under_a_shelf_stands_on_the_plate() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    let r = drawn_wall(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &shelf(),
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    let DrawnWall::Ok(w) = r else { panic!("{r:?}") };
    assert!(!w.tris.is_empty() && w.tris.len() % 3 == 0);
    assert_eq!(w.length, 40.0);
    assert!(
        (w.height - (20.0 - cfg.prop.gap)).abs() < 1e-9,
        "height {}",
        w.height
    );
    assert_eq!(w.part_attached, None);
    assert!(!w.squat);
    assert_eq!(w.tines, 0, "no tines without topo");
    assert_eq!(w.braces, Some(0));
    assert_eq!(w.top.len(), w.floors.as_ref().unwrap().len());
    assert!(w.floors.unwrap().iter().all(|&z| z == 0.0));
}

#[test]
fn a_wall_over_the_base_stands_on_the_part() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    let r = drawn_wall(
        [-20.0, 0.0, 12.0],
        [20.0, 0.0, 12.0],
        &over_base(),
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    let DrawnWall::Ok(w) = r else { panic!("{r:?}") };
    assert_eq!(w.part_attached, Some(true));
    assert!(
        w.floors.unwrap().iter().all(|&z| z > 1.0),
        "floor is the base's top"
    );
}

#[test]
fn plate_only_refuses_a_line_with_part_under_it() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    let opts = DrawOpts {
        plate_only: true,
        ..Default::default()
    };
    let r = drawn_wall(
        [-20.0, 0.0, 12.0],
        [20.0, 0.0, 12.0],
        &over_base(),
        0.0,
        &opts,
        &mut c,
    );
    assert_eq!(
        reason(&r),
        "part of the model sits under this overhang, and Plate only is on — a support can’t stand on the part here"
    );
}

#[test]
fn a_line_with_no_surface_above_it_has_nothing_to_hold() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    // off the shelf's side: the hint heights stay, but the line sits at the plate
    let r = drawn_wall(
        [100.0, 0.0, 1.0],
        [140.0, 0.0, 1.0],
        &shelf(),
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    assert_eq!(
        reason(&r),
        "nothing to hold up there — the line sits at the plate"
    );
}

#[test]
fn a_high_overhang_over_another_part_the_plate_cannot_reach() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    // a base 0.2 mm under the shelf's underside: too thin a gap to seat any wall
    let t = tris_of(&[
        block(-30.0, 30.0, -10.0, 10.0, 0.0, 19.8),
        block(-30.0, 30.0, -10.0, 10.0, 20.0, 24.0),
    ]);
    let r = drawn_wall(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &t,
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    assert!(!r.ok(), "{r:?}");
}

#[test]
fn drawn_line_follows_the_underside_and_gives_none_for_a_point() {
    let cfg = Config::default();
    let t = shelf();
    assert!(
        drawn_line(
            [0.0, 0.0, 20.0],
            [0.0, 0.0, 20.0],
            &t,
            1.0,
            f64::INFINITY,
            false,
            None,
            &cfg
        )
        .is_none()
    );
    let l = drawn_line(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &t,
        1.0,
        f64::INFINITY,
        false,
        None,
        &cfg,
    )
    .unwrap();
    assert_eq!(l.len(), 41);
    assert_eq!((l[0][0], l[40][0]), (-20.0, 20.0));
    assert!(l.iter().all(|p| (p[2] - 20.0).abs() < 0.5));
    // `under`: only down-facing surfaces, the same underside here
    let u = drawn_line(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &t,
        1.0,
        f64::INFINITY,
        true,
        None,
        &cfg,
    )
    .unwrap();
    assert_eq!(l, u);
    // an arc through a point off the chord
    let arc = drawn_line(
        [-10.0, 0.0, 20.0],
        [10.0, 0.0, 20.0],
        &t,
        1.0,
        f64::INFINITY,
        false,
        Some([0.0, 8.0, 20.0]),
        &cfg,
    )
    .unwrap();
    assert!(arc.len() > 21 && arc.iter().any(|p| p[1] > 4.0));
}

#[test]
fn the_crest_part_is_set_when_the_interface_is_on() {
    let mut cfg = Config::default();
    cfg.prop.iface = sf_engine::prop::config::Iface::Flat;
    cfg.prop.iface_layers = 2.0;
    let mut c = ctx(&cfg);
    assert!(c.crest.part.is_none());
    let t = shelf();
    let opts = DrawOpts {
        layer_height: Some(0.2),
        ..Default::default()
    };
    drawn_wall(
        [-20.0, 0.0, 20.0],
        [20.0, 0.0, 20.0],
        &t,
        0.0,
        &opts,
        &mut c,
    );
    assert!(Arc::ptr_eq(c.crest.part.as_ref().unwrap(), &t));
    assert_eq!(c.crest.layer_h, 0.2);
}

/// A closed hexahedron slab whose underside climbs from z0 at x = -30 to z1 at x = 30
/// (60 x 20, 3 mm thick).
fn ramp(z0: f64, z1: f64) -> Arc<Tris> {
    let v = |x: f64, y: f64, z: f64| [x, y, z];
    let p = [
        v(-30.0, -10.0, z0),
        v(30.0, -10.0, z1),
        v(30.0, 10.0, z1),
        v(-30.0, 10.0, z0),
        v(-30.0, -10.0, z0 + 3.0),
        v(30.0, -10.0, z1 + 3.0),
        v(30.0, 10.0, z1 + 3.0),
        v(-30.0, 10.0, z0 + 3.0),
    ];
    let f = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Arc::new(Tris::new(f.iter().flatten().flat_map(|&i| p[i]).collect()))
}

#[test]
fn a_low_plate_wall_is_a_squat_wall() {
    let cfg = Config::default();
    let t = tris_of(&[block(-30.0, 30.0, -10.0, 10.0, 1.2, 5.2)]);
    let mut c = ctx(&cfg);
    let r = drawn_wall(
        [-20.0, 0.0, 1.2],
        [20.0, 0.0, 1.2],
        &t,
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    let DrawnWall::Ok(w) = r else { panic!("{r:?}") };
    assert!(w.squat);
    assert_eq!(w.part_attached, Some(false));
    assert_eq!(
        (w.braces, w.floors),
        (None, None),
        "the squat return has neither"
    );
    assert!(w.height < cfg.prop.min_height);
}

#[test]
fn a_ramp_from_squat_to_full_height_is_a_mixed_run() {
    let cfg = Config::default();
    let t = ramp(1.0, 4.0);
    let mut c = ctx(&cfg);
    let r = drawn_wall(
        [-20.0, 0.0, 1.5],
        [20.0, 0.0, 3.5],
        &t,
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    let DrawnWall::Ok(w) = r else { panic!("{r:?}") };
    assert!(w.squat, "low stations need the squat stem");
    assert!(
        w.height > cfg.prop.min_height,
        "the tall end keeps the full wall"
    );
    assert_eq!(w.tris.len() % 3, 0);
}

#[test]
fn a_line_drawn_beside_a_body_on_the_plate_is_refused() {
    let cfg = Config::default();
    let t = tris_of(&[
        block(-30.0, 30.0, -10.0, 10.0, 6.0, 10.0),
        block(-30.0, 30.0, -12.0, -8.0, 0.0, 6.0),
    ]);
    let mut c = ctx(&cfg);
    let r = drawn_wall(
        [-20.0, -9.0, 6.0],
        [20.0, -9.0, 6.0],
        &t,
        0.0,
        &DrawOpts::default(),
        &mut c,
    );
    assert!(!r.ok(), "{r:?}");
}

#[test]
fn braces_are_counted_when_asked() {
    let cfg = Config::default();
    let mut c = ctx(&cfg);
    let opts = DrawOpts {
        brace: true,
        brace_tall: true,
        brace_clear: Some(&|_| true),
        ..Default::default()
    };
    let r = drawn_wall(
        [-1.5, 0.0, 20.0],
        [1.5, 0.0, 20.0],
        &shelf(),
        0.0,
        &opts,
        &mut c,
    );
    let DrawnWall::Ok(w) = r else { panic!("{r:?}") };
    assert!(
        w.braces.unwrap() > 0,
        "a tall wall over its aspect is braced"
    );
}
