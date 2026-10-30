//! Direct tests of `sf_engine::prop::raster` (web/prop/raster.js): the grip rule, the leftover
//! pieces of a track, the held-area ruler and the race.

mod common;
use common::ctx;
use indexmap::IndexSet;
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Region, build_topology};
use sf_engine::prop::build::{BuildOpts, PropRecord, PropsPass};
use sf_engine::prop::ctx::TineSeed;
use sf_engine::prop::raster::*;
use sf_engine::types::{IDENTITY3, Xyz};

#[test]
fn constants_and_grip_rise() {
    assert_eq!(
        (
            RACE_REL,
            RACE_ABS,
            RASTER_SKIP_HELD,
            GRIP_RISE,
            GRIP_RISE_REL
        ),
        (0.10, 5.0, 0.95, 0.1, 0.05)
    );
    assert_eq!(grip_rise(0.0), 0.1);
    assert_eq!(grip_rise(2.0), 0.1);
    assert_eq!(grip_rise(40.0), 0.05 * 40.0);
    assert!(grip_rise(f64::INFINITY).is_infinite());
    assert!(grip_rise(f64::NAN).is_nan()); // Math.max(0.1, NaN)
}

#[test]
fn raster_rest_cuts_the_pieces_either_side_of_the_run() {
    let cfg = Config::default(); // minStations 3
    let line: Vec<[f64; 3]> = (0..12).map(|i| [i as f64, 0.0, 5.0]).collect();
    let rest = raster_rest(&line, [4, 8], &cfg);
    assert_eq!(rest.len(), 2);
    assert_eq!((rest[0].from, rest[0].pts.len()), (Some(0), 4));
    assert_eq!((rest[1].from, rest[1].pts.len()), (Some(8), 4));
    // a piece shorter than minStations is dropped
    let rest = raster_rest(&line, [2, 10], &cfg);
    assert!(rest.is_empty());
    assert_eq!(raster_rest(&line, [0, 12], &cfg).len(), 0);
}

fn world() -> (sf_engine::overhangs::Topology, Analysis) {
    // two overhang faces at z = 10, 40 mm apart
    let soup = vec![
        -5.0, -5.0, 10.0, 5.0, -5.0, 10.0, 0.0, 5.0, 10.0, 35.0, -5.0, 10.0, 45.0, -5.0, 10.0,
        40.0, 5.0, 10.0,
    ];
    let topo = build_topology(soup);
    let res = Analysis {
        over: vec![1, 1],
        kept: vec![1, 1],
        on_bed: vec![0, 0],
        regions: vec![
            Region {
                faces: vec![0],
                area: 50.0,
            },
            Region {
                faces: vec![1],
                area: 50.0,
            },
        ],
        raw_region_count: 2,
        over_area: 100.0,
        bed_area: 0.0,
        over_face_count: 2,
        offset: Xyz::default(),
        size: Xyz::default(),
    };
    (topo, res)
}

fn wall(region: usize, x: f64, grip: Option<f64>) -> PropRecord {
    PropRecord {
        region: Some(region),
        line: vec![[x - 3.0, 0.0, 10.0], [x, 0.0, 10.0], [x + 3.0, 0.0, 10.0]],
        height: 10.0,
        volume: 5.0,
        grip,
        tri_ranges: vec![[0, 3]],
        ..Default::default()
    }
}

fn pass(props: Vec<PropRecord>) -> PropsPass {
    PropsPass {
        triangles: vec![[0.0, 0.0, 0.0, 0.0]; 3],
        props,
        ..Default::default()
    }
}

#[test]
fn held_by_counts_faces_with_a_top_in_reach() {
    let cfg = Config::default();
    let faces = [
        [0.0, 0.0, 10.0, 50.0, 0.0, 0.0],
        [40.0, 0.0, 10.0, 30.0, 1.0, 1.0],
    ];
    assert_eq!(held_by(&faces, &[[1.0, 0.0, 10.0]], &cfg), 50.0);
    // 0-1.5 mm below the face (and 0.05 above): too low, too high
    assert_eq!(held_by(&faces, &[[1.0, 0.0, 8.0]], &cfg), 0.0);
    assert_eq!(held_by(&faces, &[[1.0, 0.0, 10.1]], &cfg), 0.0);
    assert_eq!(held_by(&faces, &[[1.0, 0.0, 10.04]], &cfg), 50.0);
    // farther than half the span in plan
    assert_eq!(held_by(&faces, &[[7.0, 0.0, 10.0]], &cfg), 0.0);
    assert_eq!(
        held_by(&faces, &[[1.0, 0.0, 10.0], [41.0, 0.0, 10.0]], &cfg),
        80.0
    );
}

#[test]
fn a_second_pass_that_holds_clearly_more_takes_the_region() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let (topo, res) = world();
    // the normal pass holds nothing of region 0 and all of region 1; the raster pass holds region 0
    let normal = pass(vec![wall(1, 40.0, None)]);
    let second = pass(vec![wall(0, 0.0, None)]);
    let asked = std::cell::RefCell::new(Vec::new());
    let p = |r: &IndexSet<usize>| {
        asked
            .borrow_mut()
            .push(r.iter().copied().collect::<Vec<_>>());
        second.clone()
    };
    let (raced, swapped) = race_against_counted(
        &topo,
        &res,
        &IDENTITY3,
        &BuildOpts::default(),
        normal,
        0,
        &p,
        RaceTag::Raster,
        RaceArgs::default(),
        &ctx,
    );
    assert_eq!(*asked.borrow(), vec![vec![0]]); // only the bare region is rebuilt
    assert_eq!(swapped, Some(1));
    let regions: Vec<usize> = raced.props.iter().filter_map(|q| q.region).collect();
    assert_eq!(regions, vec![0, 1]);
    assert_eq!(raced.props[0].raster, Some(true));
    assert_eq!(raced.props[1].raster, Some(false));
    assert_eq!((raced.props[0].id, raced.props[1].id), (0, 1));
    assert_eq!(raced.served, 2);
}

#[test]
fn nothing_bare_means_no_second_pass() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let (topo, res) = world();
    let normal = pass(vec![wall(0, 0.0, None), wall(1, 40.0, None)]);
    let p = |_: &IndexSet<usize>| -> PropsPass { panic!("the second pass must not run") };
    let (raced, swapped) = race_against_counted(
        &topo,
        &res,
        &IDENTITY3,
        &BuildOpts::default(),
        normal.clone(),
        0,
        &p,
        RaceTag::Raster,
        RaceArgs::default(),
        &ctx,
    );
    assert_eq!((raced, swapped), (normal, None));
}

#[test]
fn a_swap_may_not_raise_the_lowest_grip() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let (topo, res) = world();
    // region 0's normal wall (grips at 1 mm) stands where the raster wall (grips at 20 mm, holds
    // more by the ruler) would: it cannot be kept beside it, and the grip would rise: no swap
    let mut near = wall(0, 2.0, Some(1.0));
    near.line = vec![[-1.0, 0.0, 10.0], [2.0, 0.0, 10.0], [5.0, 0.0, 10.0]];
    let normal = pass(vec![near, wall(1, 40.0, None)]);
    let second = pass(vec![wall(0, 0.0, Some(20.0))]);
    let p = |_: &IndexSet<usize>| second.clone();
    // the normal wall's tops hold 10, anything else 50
    let ruler = |_: &[OverFace], tops: &[[f64; 3]]| {
        if tops.first().is_some_and(|t| t[0] == -1.0) {
            10.0
        } else {
            50.0
        }
    };
    let args = RaceArgs {
        ruler: Some(&ruler),
        tried: Some([0].into_iter().collect()),
        ..Default::default()
    };
    let (raced, swapped) = race_against_counted(
        &topo,
        &res,
        &IDENTITY3,
        &BuildOpts::default(),
        normal.clone(),
        0,
        &p,
        RaceTag::Raster,
        args,
        &ctx,
    );
    assert_eq!(swapped, None);
    assert_eq!(raced, normal);
    // the same race with a raster wall that grips as low: it swaps
    let second = pass(vec![wall(0, 0.0, Some(1.0))]);
    let p = |_: &IndexSet<usize>| second.clone();
    let args = RaceArgs {
        ruler: Some(&ruler),
        tried: Some([0].into_iter().collect()),
        ..Default::default()
    };
    let (_, swapped) = race_against_counted(
        &topo,
        &res,
        &IDENTITY3,
        &BuildOpts::default(),
        normal,
        0,
        &p,
        RaceTag::Raster,
        args,
        &ctx,
    );
    assert_eq!(swapped, Some(1));
}

#[test]
fn the_veto_drops_second_pass_walls_before_the_race() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let (topo, res) = world();
    let normal = pass(vec![wall(1, 40.0, None)]);
    let second = pass(vec![wall(0, 0.0, None)]);
    let opts = BuildOpts {
        raster_veto: Some(std::rc::Rc::new(|q: &PropRecord, _| q.region == Some(0))),
        ..Default::default()
    };
    let p = |_: &IndexSet<usize>| second.clone();
    let (_, swapped) = race_against_counted(
        &topo,
        &res,
        &IDENTITY3,
        &opts,
        normal,
        0,
        &p,
        RaceTag::Raster,
        RaceArgs::default(),
        &ctx,
    );
    assert_eq!(swapped, None);
}

#[test]
fn the_tine_capture_holds_only_the_kept_walls_tines() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let seed = |x: f64| TineSeed {
        x,
        y: 0.0,
        z: 0.0,
        bite_x: 0.0,
        bite_y: 0.0,
    };
    // before buildProps: 1 seed; the normal pass captured seeds 1..4 for its wall (caps [1, 4])
    *ctx.tine_cap.borrow_mut() = Some(vec![seed(0.0), seed(1.0), seed(2.0), seed(3.0)]);
    let (topo, res) = world();
    let mut keep = wall(1, 40.0, None);
    keep.caps = Some([1, 4]);
    let normal = pass(vec![keep]);
    let mut new = wall(0, 0.0, None);
    new.caps = Some([0, 2]);
    let second = pass(vec![new]);
    let p = |_: &IndexSet<usize>| {
        ctx.capture(seed(100.0));
        ctx.capture(seed(101.0));
        second.clone()
    };
    let (raced, swapped) = race_against_counted(
        &topo,
        &res,
        &IDENTITY3,
        &BuildOpts::default(),
        normal,
        1,
        &p,
        RaceTag::Raster,
        RaceArgs::default(),
        &ctx,
    );
    assert_eq!(swapped, Some(1));
    let xs: Vec<f64> = ctx
        .tine_cap
        .borrow()
        .as_ref()
        .unwrap()
        .iter()
        .map(|t| t.x)
        .collect();
    // seed 0 stays (capFrom), then the kept walls' slices in prop order: raster wall (100, 101), normal wall (1, 2, 3)
    assert_eq!(xs, vec![0.0, 100.0, 101.0, 1.0, 2.0, 3.0]);
    assert_eq!(raced.props[0].caps, Some([1, 3]));
    assert_eq!(raced.props[1].caps, Some([3, 6]));
}
