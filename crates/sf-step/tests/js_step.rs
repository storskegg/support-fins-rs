//! Port of upstream `tests/step.test.js`: STEP import against the fixtures made with FreeCAD
//! (`tests/fixtures/*.step`): a 40x20x30 bracket with a flat 25 mm ledge and an 8 mm bore through
//! base and ledge, and the same bracket next to a separate 10 mm peg. The JS runs the vendored WASM
//! kernel; this runs the native one.

use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::overhangs::{analyze, build_topology};
use sf_step::{StepObject, is_step, read_step, step_objects};
use sf_testkit::{assert_close, upstream_root};
use sf_testkit::{is_closed, rot_x};

fn fixture(name: &str) -> Vec<u8> {
    let p = upstream_root().join("tests/fixtures").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn read_fixture(name: &str) -> Vec<StepObject> {
    assert!(
        sf_step::available(),
        "OpenCascade is not in this build: run `just occt`"
    );
    read_step(&fixture(name)).unwrap()
}

/// Signed volume of a closed flat triangle soup (divergence theorem).
fn volume(p: &[f64]) -> f64 {
    let mut v = 0.0;
    for t in p.as_chunks::<9>().0 {
        let [ax, ay, az, bx, by, bz, cx, cy, cz] =
            [t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7], t[8]];
        v += ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx);
    }
    v / 6.0
}

fn triples(p: &[f64]) -> Vec<[f64; 3]> {
    p.as_chunks::<3>().0.to_vec()
}

// tests/step.test.js:36 'isStep sniffs the Part 21 magic, not the extension'
#[test]
fn is_step_sniffs_the_part_21_magic_not_the_extension() {
    assert!(is_step(&fixture("bracket.step")), "fixture is STEP");
    assert!(
        is_step("\u{feff}\n  ISO-10303-21;\nHEADER;".as_bytes()),
        "BOM + leading whitespace"
    );
    assert!(
        !is_step(b"solid cube\n facet normal 0 0 1"),
        "ASCII STL is not STEP"
    );
    assert!(
        !is_step(&[0x50, 0x4b, 0x03, 0x04, 0, 0]),
        "a ZIP (3MF) is not STEP"
    );
    assert!(!is_step(&[]), "empty buffer");
}

// tests/step.test.js:44 'one-body STEP -> one closed object, right size and volume'
#[test]
fn one_body_step_is_one_closed_object_right_size_and_volume() {
    let objects = read_fixture("bracket.step");
    assert_eq!(objects.len(), 1, "expected 1 object, got {}", objects.len());
    let o = &objects[0];
    assert_eq!(o.name, "Bracket", "name from the file, got \"{}\"", o.name);
    assert_eq!(o.tris, o.positions.len() / 9, "tris matches the soup");
    for (k, want) in [40.0, 20.0, 30.0].into_iter().enumerate() {
        assert_close(o.bbox.size[k], want, 1e-3, &format!("bbox axis {k}"));
    }
    // base 40x20x10 + post 10x20x20 above it + ledge 25x20x6, minus the r4 bore
    // through base (10) and ledge (6). Faceting shaves the bore a hair.
    let exact = 8000.0 + 4000.0 + 3000.0 - std::f64::consts::PI * 16.0 * 16.0;
    assert_close(volume(&o.positions), exact, exact * 0.002, "volume");
    assert!(
        is_closed(&triples(&o.positions)),
        "tessellation is a closed solid"
    );
}

// tests/step.test.js:62 'curved faces are finely faceted (the triangles are what prints)'
#[test]
fn curved_faces_are_finely_faceted() {
    let o = &read_fixture("bracket.step")[0];
    // The r4 bore: count distinct vertex angles around its axis (x=30, y=10).
    let mut angles = std::collections::HashSet::new();
    for v in o.positions.as_chunks::<3>().0 {
        let (dx, dy) = (v[0] - 30.0, v[1] - 10.0);
        if (dx.hypot(dy) - 4.0).abs() < 1e-3 {
            angles.insert(sf_engine::jsnum::round(dy.atan2(dx) * 1e3) as i64);
        }
    }
    // 0.1 rad angular deflection -> ~63 sides; an STL exported at "coarse" is ~16.
    assert!(angles.len() >= 60, "bore has {} sides", angles.len());
}

// tests/step.test.js:76 'several bodies -> several pickable objects'
#[test]
fn several_bodies_are_several_pickable_objects() {
    let objects = read_fixture("two-bodies.step");
    assert_eq!(
        objects.len(),
        2,
        "expected 2 objects, got {}",
        objects.len()
    );
    let mut names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
    names.sort();
    assert_eq!(names.join(","), "Bracket,Peg", "names: {names:?}");
    let peg = objects.iter().find(|o| o.name == "Peg").unwrap();
    for (k, want) in [10.0, 10.0, 15.0].into_iter().enumerate() {
        assert_close(peg.bbox.size[k], want, 0.01, &format!("peg bbox axis {k}"));
    }
}

// tests/step.test.js:85 'a kernel failure is an error, not an empty part'
#[test]
fn a_kernel_failure_is_an_error_not_an_empty_part() {
    assert!(step_objects(None).is_err(), "unsuccessful read throws");
    assert!(
        sf_step::available(),
        "OpenCascade is not in this build: run `just occt`"
    );
    assert!(
        read_step(b"ISO-10303-21;\nnot really").is_err(),
        "garbage STEP throws"
    );
}

// tests/step.test.js:95 'an imported STEP part runs through the real fin pipeline'
#[test]
fn an_imported_step_part_runs_through_the_real_fin_pipeline() {
    let o = &read_fixture("bracket.step")[0];
    let topo = Arc::new(build_topology(o.positions.clone()));
    let rot = rot_x(0.0);
    let res = analyze(&topo, 45.0, &rot);
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        ..Default::default()
    };
    let built = build_fins(&topo, &res, &rot, &opts, &Config::default());
    // The ledge is a flat 25 mm overhang 24 mm up: it must be seen and supported.
    assert!(!built.fins.is_empty(), "the ledge gets a fin");
    let tris: Vec<[f64; 3]> = built.triangles.iter().map(|v| [v[0], v[1], v[2]]).collect();
    assert!(is_closed(&tris), "added geometry is watertight");
}
