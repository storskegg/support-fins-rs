//! Port of upstream `tests/supports.test.js`: end-to-end support invariants on the stress models, in
//! poses that produce fins.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_testkit::{bbox, inside_count, is_closed, load_model, rot_x};
use std::sync::Arc;

const CASES: [&str; 3] = ["cube", "ramp", "wedge"];

fn opts(tines: bool) -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(tines),
        ..Default::default()
    }
}

fn run(name: &str, tines: bool) -> (Arc<Topology>, Analysis, FinsBuilt) {
    let rot = rot_x(45.0);
    let topo = Arc::new(load_model(name));
    let res = analyze(&topo, 45.0, &rot);
    let built = build_fins(&topo, &res, &rot, &opts(tines), &Config::default());
    (topo, res, built)
}

// supports.test.js:26 (per case)
#[test]
fn supports_the_fin_wall_never_fuses_into_the_stl() {
    for name in CASES {
        let (topo, res, built) = run(name, false); // walls only, no grip teeth
        let inside = inside_count(&topo, &rot_x(45.0), &res.offset, &xyz(&built.triangles));
        assert!(
            inside == 0,
            "{name}/X45: {inside} wall verts are inside the STL (the wall should stand off by the gap)"
        );
    }
}

// :32
#[test]
fn supports_added_support_is_watertight() {
    for name in CASES {
        let (_, _, built) = run(name, true);
        assert!(
            is_closed(&xyz(&built.triangles)),
            "{name}/X45: fin geometry is not closed"
        );
        assert!(
            is_closed(&xyz(&built.pad_triangles)),
            "{name}/X45: pad geometry is not closed"
        );
    }
}

// :38
#[test]
fn supports_a_tilted_part_gets_a_tined_gripping_fin() {
    for name in CASES {
        let (topo, res, built) = run(name, true);
        assert!(
            built.brace_count.unwrap_or(0) >= 1,
            "{name}: no fins placed (braceCount {:?})",
            built.brace_count
        );
        assert!(
            built.tines >= 1,
            "{name}: fin has no grip tines ({})",
            built.tines
        );
        // tines on adds interior bite the walls-only build did not have
        let walls_only = build_fins(&topo, &res, &rot_x(45.0), &opts(false), &Config::default());
        let rot = rot_x(45.0);
        let with = inside_count(&topo, &rot, &res.offset, &xyz(&built.triangles));
        let without = inside_count(&topo, &rot, &res.offset, &xyz(&walls_only.triangles));
        assert!(
            with > without,
            "{name}: turning tines on added no bite into the part"
        );
    }
}

// :49
#[test]
fn supports_point_seated_tilt_fin_feet_fuse_into_the_bed_pad_not_lifted_off_it() {
    // A cube on its edge touches the bed on ~nothing, so it gets a pad; the fins must run down and
    // weld to that pad. The pad-trim regression pulled them up.
    let (_, _, built) = run("cube", true);
    assert!(
        !built.pad_triangles.is_empty(),
        "expected a bed pad on a point-seated part"
    );
    let pad = bbox(&xyz(&built.pad_triangles));
    let feet: Vec<[f64; 3]> = xyz(&built.triangles)
        .into_iter()
        .filter(|v| v[2] < 0.7)
        .collect();
    let in_pad = feet
        .iter()
        .filter(|v| {
            v[0] >= pad.lo[0] && v[0] <= pad.hi[0] && v[1] >= pad.lo[1] && v[1] <= pad.hi[1]
        })
        .count();
    // Baseline overlaps the pad heavily (>1000 verts); a trimmed-off fin drops to a handful.
    assert!(
        in_pad > 100,
        "fins barely touch the pad ({in_pad} foot verts in the pad footprint) -- they look lifted off it"
    );
    let min_z = xyz(&built.triangles)
        .iter()
        .fold(f64::INFINITY, |m, v| m.min(v[2]));
    assert!(
        min_z <= 0.1,
        "support does not reach the plate (min z {min_z:.2})"
    );
}
