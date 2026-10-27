//! Port of upstream `tests/squat.test.js`: squat bed props, near-bed overhangs too low for a flanged
//! breakaway wall. `lowledge` is a thin tongue whose underside sits ~1mm above the plate; it gets a
//! FLANGELESS squat wall that appears, stays squat, is watertight, and never fuses into the part.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::prop::build::{BuildOpts, PropsPass, build_props};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_testkit::{inside_count, is_closed, load_model, rot_x};
use std::sync::Arc;

fn build(tines: bool) -> (Arc<Topology>, Analysis, PropsPass) {
    let flat = rot_x(0.0);
    let topo = Arc::new(load_model("lowledge"));
    let res = analyze(&topo, 45.0, &flat);
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let built = build_props(
        &topo,
        &res,
        &flat,
        &BuildOpts {
            coverage: Some(0.25),
            tines,
            ..Default::default()
        },
        &ctx,
    );
    (topo, res, built)
}

// squat.test.js:24
#[test]
fn lowledge_the_near_bed_overhang_gets_a_squat_prop_it_did_not_before() {
    let (_, _, built) = build(true);
    let n = built.props.iter().filter(|p| p.squat).count();
    assert!(
        n >= 1,
        "no squat props placed (props {})",
        built.props.len()
    );
}

// :30
#[test]
fn lowledge_a_squat_prop_stays_squat_between_the_squat_floor_and_min_height() {
    let (_, _, built) = build(true);
    let c = Config::default();
    for p in built.props.iter().filter(|q| q.squat) {
        assert!(
            p.height >= c.prop.min_height_squat - 1e-6 && p.height < c.prop.min_height,
            "squat prop height {:.2} outside [{}, {})",
            p.height,
            c.prop.min_height_squat,
            c.prop.min_height
        );
    }
}

// :38
#[test]
fn lowledge_squat_support_is_watertight() {
    let (_, _, built) = build(true);
    assert!(
        is_closed(&xyz(&built.triangles)),
        "squat support geometry is not closed"
    );
}

// :43
#[test]
fn lowledge_a_squat_wall_stands_on_a_brim_wider_than_the_wall_for_plate_grip() {
    let (_, _, built) = build(false); // walls + brims, no tines
    // Effective plate-contact width = (plate-contact area) / (total wall length).
    let mut plate_area = 0.0;
    for t in built.triangles.chunks(3) {
        let (a, b, c) = (t[0], t[1], t[2]);
        if a[2] < 0.02 && b[2] < 0.02 && c[2] < 0.02 {
            plate_area +=
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0;
        }
    }
    let wall_len: f64 = built.props.iter().filter(|p| p.squat).map(|p| p.span).sum();
    let width = plate_area / wall_len;
    let c = Config::default();
    assert!(
        width > c.prop.th,
        "brim only {width:.2}mm wide (tip is {}, wall th {}) -- would peel",
        c.prop.tip,
        c.prop.th
    );
}

// :59
#[test]
fn lowledge_the_squat_wall_never_fuses_into_the_part() {
    let (topo, res, built) = build(false); // walls only; only tines may bite
    let inside = inside_count(&topo, &rot_x(0.0), &res.offset, &xyz(&built.triangles));
    assert!(
        inside == 0,
        "{inside} squat-wall verts are inside the STL (it should clear the part by the gap)"
    );
}

// :65
#[test]
fn lowledge_the_end_to_end_fins_path_surfaces_the_squat_wall_too() {
    let flat = rot_x(0.0);
    let topo = Arc::new(load_model("lowledge"));
    let res = analyze(&topo, 45.0, &flat);
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        ..Default::default()
    };
    let built = build_fins(&topo, &res, &flat, &o, &Config::default());
    assert!(
        is_closed(&xyz(&built.triangles)),
        "fins-path support geometry is not closed"
    );
    assert!(
        built.brace_count.unwrap_or(0) >= 1,
        "no supports placed via buildFins (braceCount {:?})",
        built.brace_count
    );
}
