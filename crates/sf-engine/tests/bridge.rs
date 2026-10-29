//! Tests of `sf_engine::prop::bridge::flat_run_axis`. The expected axes for the synthetic parts are
//! what the JS (`web/prop/bridge.js` at the pin) returns for the same parts (also in the parity
//! corpus `bridge/flatRunAxis.soup`); the early returns are checked by hand.
mod common;
use common::*;
use sf_engine::config::Config;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::bridge::{Support, flat_run_axis};
use sf_engine::types::IDENTITY3;

fn axis_of(parts: &[Vec<[f64; 3]>], step: f64) -> sf_engine::prop::bridge::FlatRun {
    let topo = build_topology(soup(parts));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let (_, tris, pts) = region_inputs(&topo, &res, &IDENTITY3, 0);
    let sup = Support {
        topo: &topo,
        rot: &IDENTITY3,
        offset: &res.offset,
        sway: false,
        rows_only: false,
    };
    flat_run_axis(&pts, &tris, step, Some(&sup), &Config::default())
}

#[test]
fn without_a_part_the_longer_side_wins() {
    let tris = sf_engine::tris::Tris::new(vec![]);
    let cfg = Config::default();
    let wide = [[0.0, 0.0, 1.0], [9.0, 3.0, 1.0]];
    let r = flat_run_axis(&wide, &tris, 1.0, None, &cfg);
    assert_eq!((r.axis, r.turned, r.old), ([1.0, 0.0], false, None));
    let tall = [[0.0, 0.0, 1.0], [3.0, 9.0, 1.0]];
    assert_eq!(
        flat_run_axis(&tall, &tris, 1.0, None, &cfg).axis,
        [0.0, 1.0]
    );
    // a tie goes along x
    let square = [[0.0, 0.0, 1.0], [4.0, 4.0, 1.0]];
    assert_eq!(
        flat_run_axis(&square, &tris, 1.0, None, &cfg).axis,
        [1.0, 0.0]
    );
}

#[test]
fn a_footprint_with_no_extent_is_the_old_rule_even_with_a_part() {
    let topo = build_topology(soup(&[block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0)]));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let sup = Support {
        topo: &topo,
        rot: &IDENTITY3,
        offset: &res.offset,
        sway: false,
        rows_only: false,
    };
    let tris = sf_engine::tris::Tris::new(vec![]);
    let line_x = [[0.0, 0.0, 1.0], [5.0, 0.0, 1.0]];
    let r = flat_run_axis(&line_x, &tris, 1.0, Some(&sup), &Config::default());
    assert_eq!((r.axis, r.turned, r.old), ([1.0, 0.0], false, None));
    let line_y = [[0.0, 0.0, 1.0], [0.0, 5.0, 1.0]];
    assert_eq!(
        flat_run_axis(&line_y, &tris, 1.0, Some(&sup), &Config::default()).axis,
        [0.0, 1.0]
    );
}

#[test]
fn a_ledge_off_one_post_turns_its_walls_across_the_bridge_lines() {
    let r = axis_of(
        &[
            block(-5.0, 5.0, -5.0, 5.0, 0.0, 20.0),
            block(-5.0, 25.0, -8.0, 8.0, 20.0, 24.0),
        ],
        1.0,
    );
    assert_eq!(
        (r.axis, r.turned, r.old),
        ([0.0, 1.0], true, Some([1.0, 0.0]))
    );
}

#[test]
fn a_deck_between_two_posts_runs_its_walls_across() {
    let r = axis_of(
        &[
            block(-5.0, 5.0, -5.0, 5.0, 0.0, 20.0),
            block(25.0, 35.0, -5.0, 5.0, 0.0, 20.0),
            block(-5.0, 35.0, -8.0, 8.0, 20.0, 24.0),
        ],
        1.0,
    );
    assert_eq!((r.axis, r.turned), ([0.0, 1.0], true));
}

#[test]
fn a_ledge_too_narrow_for_crossing_walls_keeps_the_old_long_walls() {
    // 4 mm wide: crossing walls would be stubs under minSpan, so no turn
    let r = axis_of(
        &[
            block(-5.0, 5.0, -2.0, 2.0, 0.0, 20.0),
            block(-5.0, 30.0, -2.0, 2.0, 20.0, 22.0),
        ],
        1.0,
    );
    assert_eq!(
        (r.axis, r.turned, r.old),
        ([1.0, 0.0], false, Some([1.0, 0.0]))
    );
}

#[test]
fn the_grid_step_can_change_the_pick() {
    // a post beside the face: coarse cells (2) see it differently from fine ones
    let parts = [
        block(10.0, 14.0, -10.0, 10.0, 0.0, 22.0),
        block(-12.0, 12.0, -10.0, 10.0, 20.0, 22.0),
    ];
    assert_eq!(axis_of(&parts, 1.0).axis, [0.0, 1.0]);
    assert_eq!(axis_of(&parts, 2.0).axis, [1.0, 0.0]);
}

#[test]
fn a_square_bridge_keeps_the_old_pick_walls_along_y() {
    let r = axis_of(
        &[
            block(-5.0, 5.0, -15.0, 15.0, 0.0, 20.0),
            block(25.0, 35.0, -15.0, 15.0, 0.0, 20.0),
            block(-5.0, 35.0, -15.0, 15.0, 20.0, 23.0),
        ],
        1.0,
    );
    assert_eq!(
        (r.axis, r.turned, r.old),
        ([0.0, 1.0], false, Some([0.0, 1.0]))
    );
}
