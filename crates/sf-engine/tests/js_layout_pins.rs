//! Port of upstream `tests/layout_pins.test.js`: HOW MANY WALLS the reference parts get. The rest of
//! the suite pins what makes a wall right (closed, clear of the part, gripping), not where walls go
//! -- so a placement change can add two walls to a plain cube with every test green (#143: Matthew
//! saw it in the test build first). A change that moves walls on purpose updates these numbers in
//! the same PR, where review sees it; one that didn't mean to fails here. Wall COUNTS only: tine
//! counts shift with float noise (plugins/shared/ENGINE-SENSITIVITY.md). Auto, bed pad, default
//! coverage.
mod builds;
use builds::*;
use sf_engine::fins::build::{FinsOpts, SwayRequest};
use sf_testkit::{load_model, rot_x, rot_y};
use std::sync::Arc;

// layout_pins.test.js:26 (one test per pin in JS)
#[test]
fn layout_pins_the_reference_parts_get_their_wall_counts() {
    // model, pose, rotation, walls
    let pins: Vec<(&str, &str, [f64; 9], usize)> = vec![
        ("cube", "X25", rot_x(25.0), 4), // the end rows move out flush to the free edges, re-spaced at the pitch (#143 had added two beside them)
        ("cube", "X35", rot_x(35.0), 4), // flush on every overhang now, not only up to 30deg (#269: X35-X44 kept a 6.7 mm bare strip)
        ("cube", "X40", rot_x(40.0), 4),
        ("cube", "X60", rot_x(60.0), 4),
        ("cube", "Y35", rot_y(35.0), 4),
        ("lbracket", "X35", rot_x(35.0), 5), // the plugins' reference part (plugins/*/tests pin its tines too)
        ("lbracket", "Y35", rot_y(35.0), 5),
        ("ramp", "X45", rot_x(45.0), 0),
        ("wedge", "X45", rot_x(45.0), 0),
        ("tube", "X25", rot_x(25.0), 4),
        ("arch", "X25", rot_x(25.0), 5), // likewise (#143 had 6, doubled at the edges)
    ];
    for (name, pose, rot, want) in pins {
        let topo = Arc::new(load_model(name));
        let (_, b) = build(&topo, &rot, &auto_opts(Some(0.5)));
        let got = b.props.len();
        assert!(
            got == want,
            "{name} {pose}: {got} walls, pinned {want} -- if the change is meant, update the pin"
        );
    }
}

// Sway on keeps the old 30deg cutoff (PROP.edgeFlatDegSway): a flush end wall stands where the
// braces go, and the braces win. The cube at X40 keeps main's 3 walls at the thirds and both its
// braces; with Sway off it gets 4, flush to both side edges.
// layout_pins.test.js:35
#[test]
fn layout_cube_x40_with_sway_on_keeps_3_walls_and_both_braces() {
    let topo = Arc::new(load_model("cube"));
    let opts = FinsOpts {
        sway: Some(SwayRequest {
            on: true,
            ..Default::default()
        }),
        ..auto_opts(Some(0.5))
    };
    let (_, b) = build(&topo, &rot_x(40.0), &opts);
    assert!(
        b.props.len() == 3,
        "{} walls with Sway on, pinned 3",
        b.props.len()
    );
    assert!(
        b.sway.as_ref().map(|s| s.count) == Some(2),
        "{:?} sway braces, pinned 2",
        b.sway.map(|s| s.count)
    );
}
