//! Port of upstream `tests/part_wall_ends.test.js`: a wall standing ON THE PART keeps its ends off
//! the part's side walls (#119). In a recessed box the walls cross the slicer's bridge and run to
//! the box's end wall; parity probing saw nothing, since the last station sat exactly ON that wall,
//! outside the part, so the end cap was built flush against it and welded.
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::overhangs::build_topology;
use sf_engine::prop::attached::{Attached, build_part_attached};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, V4, Xyz};

/// The reporter's shape in section (part_wall_ends.test.js:11-12), `(y, z)` counter-clockwise.
const SECTION: [[f64; 2]; 12] = [
    [-25.0, 0.0],
    [25.0, 0.0],
    [25.0, 25.0],
    [11.0, 25.0],
    [11.0, 20.0],
    [21.0, 20.0],
    [21.0, 4.0],
    [-21.0, 4.0],
    [-21.0, 20.0],
    [-11.0, 20.0],
    [-11.0, 25.0],
    [-25.0, 25.0],
];
/// `[y0, y1, z0, z1]` end-cap rectangles (part_wall_ends.test.js:13).
const CAPS: [[f64; 4]; 5] = [
    [-25.0, 25.0, 0.0, 4.0],
    [21.0, 25.0, 4.0, 25.0],
    [-25.0, -21.0, 4.0, 25.0],
    [11.0, 21.0, 20.0, 25.0],
    [-21.0, -11.0, 20.0, 25.0],
];

/// `extrude(x0, x1)` (part_wall_ends.test.js:14-30): the section run along x, as an f32 soup.
fn extrude(x0: f64, x1: f64) -> Vec<f32> {
    let mut t: Vec<[f64; 3]> = Vec::new();
    // a triangle wound so its normal points along `n`
    let mut tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3], n: [f64; 3]| {
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let m = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if m[0] * n[0] + m[1] * n[1] + m[2] * n[2] >= 0.0 {
            t.extend([a, b, c]);
        } else {
            t.extend([a, c, b]);
        }
    };
    for (i, &[ya, za]) in SECTION.iter().enumerate() {
        let [yb, zb] = SECTION[(i + 1) % SECTION.len()];
        let n = [0.0, zb - za, -(yb - ya)]; // outward
        tri([x0, ya, za], [x1, ya, za], [x1, yb, zb], n);
        tri([x0, ya, za], [x1, yb, zb], [x0, yb, zb], n);
    }
    for [y0, y1, z0, z1] in CAPS {
        for (x, n) in [(x0, [-1.0, 0.0, 0.0]), (x1, [1.0, 0.0, 0.0])] {
            tri([x, y0, z0], [x, y1, z0], [x, y1, z1], n);
            tri([x, y0, z0], [x, y1, z1], [x, y0, z1], n);
        }
    }
    t.iter().flatten().map(|&v| v as f32).collect()
}

#[test]
fn a_wall_run_up_to_the_end_wall_stops_short_of_it() {
    // under the +y lip, its last station 0.1 mm short of the end wall's face (y = 21): clearly
    // outside the part, so parity passes it, and 0.1 mm welds. Seated as-is.
    let boxed = extrude(-35.0, 35.0);
    let pos: Vec<f64> = boxed.iter().map(|&v| v as f64).collect();
    let topo = build_topology(pos.clone());
    let offset = Xyz {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let mut line = Vec::new();
    let mut y = 11.9;
    while y < 21.0 {
        line.push([0.0, y, 20.0]);
        y += 1.0;
    }
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let mut out: Vec<V4> = Vec::new();
    let r = build_part_attached(
        &line,
        &Tris::new(pos),
        &topo,
        &IDENTITY3,
        &offset,
        &mut out,
        &ctx,
    );
    assert!(
        matches!(r, Attached::Built(_)),
        "no wall under the lip: {r:?}"
    );
    let end = out.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        end <= 21.0 - 0.205,
        "the wall's end is {:.3} mm off the end wall -- it welds",
        21.0 - end
    );
}

// part_wall_ends.test.js:50
#[test]
fn a_drawn_wall_run_to_the_end_wall_stops_short_of_it_too() {
    // Draw's part path (Draw mode, and the Full fill pass's walls) built from the line as given:
    // drawn to y = 21 it ended flush on the end wall, and a line past it (y = 22) ended there as
    // well. With the topology it trims its ends like Auto's.
    let boxed = extrude(-35.0, 35.0);
    let pos: Vec<f64> = boxed.iter().map(|&v| v as f64).collect();
    let topo = build_topology(pos.clone());
    let tris = Arc::new(Tris::new(pos));
    let offset = Xyz {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    let cfg = Config::default();
    let opts = DrawOpts {
        tines: false,
        topo: Some(&topo),
        rot: Some(IDENTITY3),
        offset: Some(offset),
        ..Default::default()
    };
    let draw = |a: [f64; 3], b: [f64; 3]| {
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        drawn_wall(a, b, &tris, 0.0, &opts, &mut ctx)
    };
    for (y0, y1, sign) in [(12.0, 21.0, 1.0), (12.0, 22.0, 1.0), (-12.0, -21.0, -1.0)] {
        let r = draw([0.0, y0, 20.0], [0.0, y1, 20.0]);
        let DrawnWall::Ok(r) = r else {
            panic!("no part wall for {y0}..{y1}: {r:?}");
        };
        assert!(r.part_attached == Some(true), "no part wall for {y0}..{y1}");
        let end = r
            .tris
            .iter()
            .map(|p| sign * p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            end <= 21.0 - 0.205,
            "drawn {y0}..{y1}: the wall's end is {:.3} mm off the end wall -- it welds",
            21.0 - end
        );
    }
    // a line that already stops clear is left as drawn
    let ok = draw([0.0, 12.0, 20.0], [0.0, 20.5, 20.0]);
    let DrawnWall::Ok(ok) = ok else {
        panic!("a clear drawn line was refused: {ok:?}");
    };
    let max = ok
        .tris
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(max > 20.4, "a clear drawn line was trimmed");
}

// Port-side additions (no JS counterpart): the preview omits the topology and stays as drawn; a line
// with too little clear of the end wall is refused with the translated message's English key.
#[test]
fn a_drawn_wall_without_the_topology_stays_as_drawn_and_a_hopeless_one_is_refused() {
    let boxed = extrude(-35.0, 35.0);
    let pos: Vec<f64> = boxed.iter().map(|&v| v as f64).collect();
    let topo = build_topology(pos.clone());
    let tris = Arc::new(Tris::new(pos));
    let cfg = Config::default();
    let run = |opts: &DrawOpts, a: [f64; 3], b: [f64; 3]| {
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        drawn_wall(a, b, &tris, 0.0, opts, &mut ctx)
    };
    // the live preview (no topology): the cap is built to the line as given
    let preview = run(&DrawOpts::default(), [0.0, 12.0, 20.0], [0.0, 21.0, 20.0]);
    let DrawnWall::Ok(p) = preview else {
        panic!("preview refused: {preview:?}");
    };
    let end = p
        .tris
        .iter()
        .map(|v| v[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(end > 20.8, "the preview was trimmed ({end})");
    // with the topology the same line is trimmed
    let with = DrawOpts {
        topo: Some(&topo),
        rot: Some(IDENTITY3),
        offset: Some(Xyz::default()),
        ..Default::default()
    };
    let DrawnWall::Ok(t) = run(&with, [0.0, 12.0, 20.0], [0.0, 21.0, 20.0]) else {
        panic!("trimmed line refused");
    };
    let trimmed = t
        .tris
        .iter()
        .map(|v| v[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(trimmed < end, "trimmed {trimmed} vs preview {end}");
    assert!(t.top.len() < p.top.len());
    // a line run along the end wall, every station against it: nothing is left to stand
    let r = run(&with, [-20.0, 20.8, 20.0], [20.0, 20.8, 20.0]);
    let DrawnWall::Refused { reason } = &r else {
        panic!("a line along the side wall was built: {r:?}");
    };
    assert_eq!(
        reason,
        "too close to the part’s side wall — draw the line a little further from it"
    );
}
