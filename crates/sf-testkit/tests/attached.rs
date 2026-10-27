//! Own tests for `prop::attached::build_part_attached`: the three verdicts and every refusal, on
//! parts built from boxes (a base with a slab over it, with pillars and walls added).
use sf_engine::config::Config;
use sf_engine::fins::seating::seated_part_tris;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::attached::{Attached, PART_BAND, Why, build_part_attached};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::types::{IDENTITY3, V4, Xyz};
use sf_testkit::block;

fn run(boxes: &[[f64; 6]], line: &[[f64; 3]], cfg: &Config) -> (Attached, Vec<V4>) {
    let pos: Vec<f64> = boxes
        .iter()
        .flat_map(|b| block(b[0], b[1], b[2], b[3], b[4], b[5]))
        .collect();
    let topo = build_topology(pos);
    let off = Xyz::default();
    let _ = analyze(&topo, 45.0, &IDENTITY3);
    let soup = seated_part_tris(&topo, &IDENTITY3, &off);
    let ctx = BuildCtx::new(cfg, Crest::new(None, 0.0, cfg));
    let mut out = Vec::new();
    let r = build_part_attached(line, &soup, &topo, &IDENTITY3, &off, &mut out, &ctx);
    (r, out)
}

/// A base (0..20 x 0..20, 0..10) under a slab (-20..40 x 0..20, z 20..24).
const T: [[f64; 6]; 2] = [
    [0.0, 20.0, 0.0, 20.0, 0.0, 10.0],
    [-20.0, 40.0, 0.0, 20.0, 20.0, 24.0],
];

fn line_at(y: f64, z: f64, x0: f64, x1: f64) -> Vec<[f64; 3]> {
    let mut l = Vec::new();
    let mut x = x0;
    while x <= x1 + 1e-9 {
        l.push([x, y, z]);
        x += 1.0;
    }
    l
}

#[test]
fn a_wall_under_a_slab_stands_on_the_base() {
    let cfg = Config::default();
    let (r, out) = run(&T, &line_at(10.0, 20.0, 3.0, 17.0), &cfg);
    let Attached::Built(p) = r else {
        panic!("{r:?}")
    };
    assert_eq!(p.stations, 15);
    assert!(p.part_attached && !p.in_bore);
    assert!((p.height - (20.0 - cfg.prop.gap - 10.0)).abs() < 1e-6);
    assert!((p.span - 14.0).abs() < 1e-6);
    assert!(p.volume > 0.0);
    assert_eq!(p.line.len(), 15);
    assert!((p.line[0][2] - (20.0 - cfg.prop.gap)).abs() < 1e-6); // the wall top, `gap` under the line
    assert!(!out.is_empty() && out.len() % 3 == 0);
}

#[test]
fn an_overhang_over_open_plate_is_left_to_the_plate_path() {
    let cfg = Config::default();
    let (r, out) = run(&T, &line_at(10.0, 20.0, 25.0, 38.0), &cfg);
    assert_eq!(r, Attached::NoFloor);
    assert!(out.is_empty());
    // a floor under fewer than half the stations (3 of 7): also no floor
    let (r, _) = run(&T, &line_at(10.0, 20.0, 18.0, 24.0), &cfg);
    assert_eq!(r, Attached::NoFloor);
}

#[test]
fn a_column_inside_the_part_is_buried() {
    let cfg = Config::default();
    // a pillar joining the base to the slab: the wall's centreline is inside it
    let mut boxes = T.to_vec();
    boxes.push([-1.0, 21.0, 8.0, 12.0, 9.0, 21.0]);
    let (r, out) = run(&boxes, &line_at(10.0, 20.0, 3.0, 17.0), &cfg);
    assert_eq!(r, Attached::Floored(Why::Buried));
    assert!(out.is_empty());
}

#[test]
fn a_side_wall_in_the_way_blocks_the_wall() {
    let cfg = Config::default();
    // a thin plank beside the line, inside the wall's side clearance but not on its centreline
    let mut boxes = T.to_vec();
    boxes.push([0.0, 20.0, 10.0 + 0.5 + 0.05, 10.0 + 0.5 + 0.3, 10.0, 20.0]);
    let (r, _) = run(&boxes, &line_at(10.0, 20.0, 3.0, 17.0), &cfg);
    assert_eq!(r, Attached::Floored(Why::Blocked));
}

#[test]
fn an_overhang_too_close_to_its_floor_is_a_stub() {
    let cfg = Config::default();
    let (r, out) = run(&T, &line_at(10.0, 11.3, 3.0, 17.0), &cfg);
    assert_eq!(r, Attached::Floored(Why::Stub));
    assert!(out.is_empty());
}

#[test]
fn a_run_under_the_least_part_span_is_a_stub() {
    let mut cfg = Config::default();
    cfg.prop.min_span_part = 20.0;
    let (r, _) = run(&T, &line_at(10.0, 20.0, 3.0, 17.0), &cfg);
    assert_eq!(r, Attached::Floored(Why::Stub));
}

#[test]
fn a_wall_walled_in_all_round_is_flagged_in_bore() {
    let cfg = Config::default();
    // the base is a pit: four tall walls round it, 4 mm from the line's column
    let boxes = [
        [-6.0, 26.0, -6.0, 26.0, 0.0, 10.0],  // floor
        [-6.0, 26.0, 16.0, 26.0, 10.0, 40.0], // north wall
        [-6.0, 26.0, -6.0, 4.0, 10.0, 40.0],  // south wall
        [-6.0, 4.0, -6.0, 26.0, 10.0, 40.0],  // west wall
        [16.0, 26.0, -6.0, 26.0, 10.0, 40.0], // east wall
        [-6.0, 26.0, -6.0, 26.0, 20.0, 24.0], // ceiling (slab) over the pit
    ];
    let (r, _) = run(&boxes, &line_at(10.0, 20.0, 5.0, 15.0), &cfg);
    match r {
        Attached::Built(p) => assert!(p.in_bore),
        other => panic!("{other:?}"),
    }
}

#[test]
fn part_band_is_three_mm() {
    assert_eq!(PART_BAND, 3.0);
}
