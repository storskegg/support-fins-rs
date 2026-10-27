//! Port of upstream `tests/integration.test.js`: END-TO-END on whole STLs: load a stress model, run
//! the real buildFins pipeline, and assert on the SUPPORT IT ACTUALLY PRODUCES -- counts and
//! per-tine dimensions.
//!
//! Why this file exists (Matthew's call): the unit tests pin emitTines on a synthetic block, but
//! they can encode a wrong number as "correct" (they did -- the tine width was asserted at the wall
//! thickness, 2x Slant3D's spec, for weeks). These tests measure the tines that come out of a full
//! build against the spec, so a regression in width/height/placement shows up on a real part.

use std::collections::HashSet;
use std::sync::Arc;

use sf_engine::config::{Config, Tunables};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::types::{Rot3, V4};
use sf_testkit::{inside_count, is_closed, load_model, rot_x, tine_boxes};

struct Built {
    topo: Arc<Topology>,
    rot: Rot3,
    res: Analysis,
    built: FinsBuilt,
}

fn base_opts() -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        ..Default::default()
    }
}

fn build(name: &str, tilt: f64, opts: FinsOpts) -> Built {
    let topo = Arc::new(load_model(name));
    let rot = rot_x(tilt);
    let res = analyze(&topo, 45.0, &rot);
    let built = build_fins(&topo, &res, &rot, &opts, &Config::default());
    Built {
        topo,
        rot,
        res,
        built,
    }
}

fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

/// Isolate the tine triangles: whatever a tined build adds over a tine-less one. emitTines appends
/// each tine as a contiguous 36-vertex block (and its wall step, when it has one, as the next
/// 36), so chunk by 36; tineBoxes tells them apart.
fn tine_chunks(name: &str, tilt: f64, opts: &FinsOpts) -> Vec<V4> {
    let off = build(
        name,
        tilt,
        FinsOpts {
            tines: Some(false),
            ..opts.clone()
        },
    )
    .built
    .triangles;
    let on = build(
        name,
        tilt,
        FinsOpts {
            tines: Some(true),
            ..opts.clone()
        },
    )
    .built
    .triangles;
    // JS keys a vertex's components (tagged vertices have four) rounded to 1e-4
    let key = |t: &[V4]| -> Vec<i64> {
        t.iter()
            .flat_map(|v| {
                let n = if v[3] == 1.0 { 4 } else { 3 };
                (0..n)
                    .map(|k| sf_engine::jsnum::round(v[k] * 1e4) as i64)
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let mut off_keys: HashSet<Vec<i64>> = HashSet::new();
    let mut i = 0;
    while i + 2 < off.len() {
        off_keys.insert(key(&off[i..i + 3]));
        i += 3;
    }
    let mut verts: Vec<V4> = Vec::new();
    let mut i = 0;
    while i + 2 < on.len() {
        if !off_keys.contains(&key(&on[i..i + 3])) {
            verts.extend_from_slice(&on[i..i + 3]);
        }
        i += 3;
    }
    // chunks of 36
    let whole = verts.len() / 36 * 36;
    verts.truncate(whole);
    verts
}

/// Tine boxes and their wall steps (local issue 005), told apart by tineBoxes.
fn tines_and_steps(name: &str, tilt: f64, opts: &FinsOpts) -> sf_testkit::TineBoxes {
    tine_boxes(&xyz(&tine_chunks(name, tilt, opts)))
}

// (model, tilt) pairs that genuinely need support and place it.
const NEEDS_SUPPORT: [(&str, f64); 4] = [
    ("ramp", 40.0),
    ("wedge", 40.0),
    ("staircase", 40.0),
    ("lbracket", 40.0),
];

// integration.test.js:47 (one test per pair in JS)
#[test]
fn a_part_that_needs_support_actually_gets_some_no_silent_zero() {
    for (name, tilt) in NEEDS_SUPPORT {
        let b = build(name, tilt, base_opts());
        let n = b.built.props.len();
        assert!(n >= 1, "{name}@{tilt} produced NO support at all");
        assert!(
            is_closed(&xyz(&b.built.triangles)),
            "{name}@{tilt} support is not watertight"
        );
    }
}

// integration.test.js:56
#[test]
fn a_grippable_tilted_part_gets_gripping_tines_and_the_walls_never_fuse() {
    let b = build("ramp", 40.0, base_opts());
    assert!(
        b.built.tines >= 1,
        "ramp@40 got no tines ({}) -- combined support has no grip",
        b.built.tines
    );
    // walls-only: nothing but tines may be inside the part
    let w = build(
        "ramp",
        40.0,
        FinsOpts {
            tines: Some(false),
            ..base_opts()
        },
    );
    let inside = inside_count(&w.topo, &w.rot, &w.res.offset, &xyz(&w.built.triangles));
    assert!(
        inside == 0,
        "{inside} wall verts are inside the STL (should clear by the gap)"
    );
}

// integration.test.js:65
#[test]
fn every_tine_on_a_real_part_matches_slants_spec_one_layer_tall_one_bead_wide() {
    let cfg = Config::default();
    let prop = &cfg.prop;
    let chunks = tines_and_steps("ramp", 40.0, &base_opts()).tines;
    assert!(
        chunks.len() >= 3,
        "expected several tines to measure, got {}",
        chunks.len()
    );
    // the tine footprint is a rectangle: (bite + overlap) long, tineW wide. Pin the diagonal to
    // THAT -- a th-wide (1.0mm) tine pushes the diagonal from ~0.94 to ~1.28 and trips this. Uses
    // tineW so it tracks the spec, not a loose 0.8.
    let max_diag = (prop.tine_reach + prop.tine_overlap).hypot(prop.tine_w) + 0.05;
    for b in &chunks {
        // one layer tall
        let z_ext = b.hi[2] - b.lo[2];
        assert!(
            (z_ext - prop.tine_h).abs() < 1e-4,
            "a tine is {z_ext:.3}mm tall, not one layer ({})",
            prop.tine_h
        );
        // one bead wide: the XY footprint diagonal can't exceed bite x tineW's box.
        let diag = (b.hi[0] - b.lo[0]).hypot(b.hi[1] - b.lo[1]);
        assert!(
            diag <= max_diag,
            "a tine's footprint diagonal is {diag:.2}mm (> {max_diag:.2}) -- wider than one nozzle bead"
        );
    }
}

// integration.test.js:83
#[test]
fn petg_a_tine_snapped_above_the_wall_top_gets_a_wall_step_under_it_inside_its_footprint() {
    // PETG's 0.3 gap is 1.5 layers, so a one-layer tine snapped to the part's layer can start a
    // full layer above the wall: without the step the slice under it is empty and the tine hangs
    // off the part, not touching the wall (local issue 005).
    let cfg = Config::default();
    let opts = FinsOpts {
        tunables: Some(Tunables {
            prop_gap: Some(0.3),
            ..Default::default()
        }),
        ..base_opts()
    };
    let r = tines_and_steps("cone", 30.0, &opts);
    let (tines, steps) = (r.tines, r.steps);
    assert!(
        tines.len() >= 10,
        "expected a comb of tines, got {}",
        tines.len()
    );
    assert!(
        steps.len() as f64 >= tines.len() as f64 / 4.0,
        "only {} wall steps for {} PETG tines",
        steps.len(),
        tines.len()
    );
    for st in &steps {
        let h = st.b.hi[2] - st.b.lo[2];
        // from half a layer inside the wall to half a layer inside the tine: <= 2 layers
        assert!(
            h > 0.0 && h <= 2.0 * cfg.prop.tine_h + 1e-6,
            "a wall step is {h:.3} mm tall (max 2 layers)"
        );
    }
}
