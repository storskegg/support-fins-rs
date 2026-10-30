//! Port of upstream `tests/plate_only.test.js`: PLATE ONLY (#218): every support stands on the build
//! plate, none on the part. A line with part under it is dropped and counted (`skipped.onPart`),
//! never stilted to the plate through that part; a drawn line there is refused with the reason.
mod common;
use common::f32s;
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::overhangs::analyze;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_testkit::{block, load_model, rot_y};

fn dw(a: [f64; 3], b: [f64; 3], pos: &[f64], plate_only: bool) -> DrawnWall {
    let cfg = Config::default();
    let tris = Arc::new(Tris::new(pos.to_vec()));
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let opts = DrawOpts {
        plate_only,
        ..Default::default()
    };
    drawn_wall(a, b, &tris, 0.0, &opts, &mut ctx)
}

/// An L: a base block (top z=5) and a shelf 30 mm above it -- a wall under the shelf stands on the base.
fn l_part() -> Vec<f64> {
    f32s(
        &[
            block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
            block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
        ]
        .concat(),
    )
}

/// The shelf alone, over open plate.
fn shelf() -> Vec<f64> {
    f32s(&block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0))
}

// plate_only.test.js:13
#[test]
fn plate_only_a_drawn_line_with_part_under_it_is_refused_with_the_reason() {
    let l = l_part();
    let on = dw([-30.0, 0.0, 35.0], [30.0, 0.0, 35.0], &l, false);
    assert!(
        matches!(&on, DrawnWall::Ok(o) if o.part_attached == Some(true)),
        "without Plate only the wall stands on the base"
    );
    let r = dw([-30.0, 0.0, 35.0], [30.0, 0.0, 35.0], &l, true);
    let DrawnWall::Refused { reason } = r else {
        panic!("Plate only built a wall over the part")
    };
    assert!(
        reason.contains("Plate only"),
        "reason doesn't name the setting: {reason}"
    );
}

// plate_only.test.js:21
#[test]
fn plate_only_a_drawn_line_over_open_plate_builds_the_same_wall() {
    let s = shelf();
    let a = dw([-30.0, 0.0, 35.0], [30.0, 0.0, 35.0], &s, false);
    let b = dw([-30.0, 0.0, 35.0], [30.0, 0.0, 35.0], &s, true);
    let (DrawnWall::Ok(a), DrawnWall::Ok(b)) = (a, b) else {
        panic!("the plate wall should build either way")
    };
    assert!(
        b.part_attached != Some(true),
        "the plate wall should build either way"
    );
    assert!(a.tris == b.tris, "Plate only changed a plate wall");
}

fn fins_opts(mode: &str, plate_only: bool) -> FinsOpts {
    FinsOpts {
        mode: Some(mode.into()),
        bed_pad: Some(true),
        plate_only,
        ..Default::default()
    }
}

/// The portal on its side (Y90): Auto stands its one wall on the lower leg.
fn plate_only_builds_nothing_on_the_part(mode: &str) {
    let topo = Arc::new(load_model("portal"));
    let rot = rot_y(90.0);
    let cfg = Config::default();
    let res = analyze(&topo, 45.0, &rot);
    let base = build_fins(&topo, &res, &rot, &fins_opts(mode, false), &cfg);
    assert!(
        base.props.iter().any(|q| q.part_attached),
        "{mode}: the fixture lost its wall on the part"
    );
    let res2 = analyze(&topo, 45.0, &rot);
    let b = build_fins(&topo, &res2, &rot, &fins_opts(mode, true), &cfg);
    assert!(
        !b.props.iter().any(|q| q.part_attached),
        "{mode}: a wall still stands on the part"
    );
    assert!(
        b.skipped.on_part > 0,
        "{mode}: the dropped wall isn't counted"
    );
    // every support that IS built reaches down to the plate
    for f in &b.fins {
        let mut low = f64::INFINITY;
        for [i, j] in &f.tri_ranges {
            for k in *i..*j {
                low = low.min(b.triangles[k][2]);
            }
        }
        assert!(
            low < 0.5,
            "{mode}: a {:?} bottoms out at z {low:.2}, off the plate",
            f.kind
        );
    }
    // Full refuses lines over the part before building them, not one wall at a time until it hits its
    // try limit (and then blames the limit)
    if mode == "full" {
        let fill = b.fill.as_ref().unwrap();
        assert!(!fill.capped, "full: capped after {} tries", fill.tries);
    }
}

// plate_only.test.js:44
#[test]
fn plate_only_auto_builds_nothing_on_the_part_and_counts_what_it_left() {
    plate_only_builds_nothing_on_the_part("auto");
}

// plate_only.test.js:44
#[test]
fn plate_only_full_builds_nothing_on_the_part_and_counts_what_it_left() {
    plate_only_builds_nothing_on_the_part("full");
}

// plate_only.test.js:62
#[test]
fn plate_only_off_by_default_and_a_part_with_nothing_over_it_is_unchanged() {
    let topo = Arc::new(load_model("lbracket"));
    let rot = rot_y(35.0);
    let cfg = Config::default();
    let a = build_fins(
        &topo,
        &analyze(&topo, 45.0, &rot),
        &rot,
        &fins_opts("auto", false),
        &cfg,
    );
    let b = build_fins(
        &topo,
        &analyze(&topo, 45.0, &rot),
        &rot,
        &fins_opts("auto", true),
        &cfg,
    );
    assert!(a.skipped.on_part == 0, "onPart counted with Plate only off");
    assert!(a.triangles == b.triangles, "lbracket changed");
}
