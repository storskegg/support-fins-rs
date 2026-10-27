//! Direct tests of `sf_engine::prop::rival` (web/prop/rival.js): the tube-vs-patches race.

use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Region, Topology, build_topology};
use sf_engine::prop::build::{Acc, Patch, PropRecord};
use sf_engine::prop::rival::{restore, snapshot, tube_rivalry};
use sf_engine::types::Xyz;

/// One overhang face at z = 10 around the origin, as a one-region analysis.
fn world() -> (Topology, Analysis, Vec<f64>) {
    let soup = vec![-1.0, -1.0, 10.0, 1.0, -1.0, 10.0, 0.0, 1.0, 10.0];
    let topo = build_topology(soup.clone());
    let res = Analysis {
        over: vec![1],
        kept: vec![1],
        on_bed: vec![0],
        regions: vec![Region {
            faces: vec![0],
            area: 2.0,
        }],
        raw_region_count: 1,
        over_area: 2.0,
        bed_area: 0.0,
        over_face_count: 1,
        offset: Xyz::default(),
        size: Xyz::default(),
    };
    (topo, res, soup)
}

fn patch(area: f64) -> Patch {
    Patch {
        region: 7,
        area,
        tris: Arc::new(vec![-5.0, -5.0, 9.0, 5.0, 5.0, 9.0, 0.0, 0.0, 9.0]),
        ..Default::default()
    }
}

fn prop(a: usize, b: usize) -> PropRecord {
    PropRecord {
        tri_ranges: vec![[a, b]],
        ..Default::default()
    }
}

#[test]
fn snapshot_and_restore_undo_everything_the_loop_accumulates() {
    let mut a = Acc::default();
    let m = snapshot(&a);
    a.out.push([1.0, 2.0, 3.0, 0.0]);
    a.props.push(prop(0, 1));
    a.tally.tines += 3;
    a.tally.sag_risk = true;
    a.skipped.sliver += 2;
    a.served.insert(4);
    restore(&mut a, &m);
    assert!(a.out.is_empty() && a.props.is_empty() && a.served.is_empty());
    assert_eq!(
        (a.tally.tines, a.tally.sag_risk, a.skipped.sliver),
        (0, false, 0)
    );
}

/// Runs the loop `buildProps` runs: a tube patch (with two big rivals and a sliver), then each
/// rival. `tube_vertex` is what the tube builds, `patch_vertex` what each rival patch builds.
fn run(tube_vertex: Option<[f64; 4]>, patch_vertex: Option<[f64; 4]>, arc: bool) -> Acc {
    let (topo, res, soup) = world();
    let cfg = Config::default();
    let mut a = Acc::default();
    let mut rv = tube_rivalry(&topo, &res, &soup, &cfg);
    let mut tube = patch(30.0);
    tube.arc = arc;
    tube.small_tube = Some(vec![patch(20.0), patch(15.0), patch(1.0)]);
    let mut patches = vec![tube];
    let mut i = 0;
    while i < patches.len() {
        let mark = rv.begin(&mut a, &patches[i]);
        if i == 0 {
            if let Some(v) = tube_vertex {
                a.out.push(v);
                a.props.push(prop(0, 1));
                a.tally.tines += 2;
            }
            rv.tube_done(
                &mut a,
                0,
                mark.expect("a tube patch gets a mark"),
                &mut patches,
            );
        } else if let Some(v) = patch_vertex {
            a.out.push(v);
        }
        i += 1;
    }
    rv.finish(&mut a);
    a
}

const HELD: [f64; 4] = [0.0, 0.0, 9.5, 0.0];

#[test]
fn the_tube_stays_when_it_holds_more_than_the_patches() {
    let a = run(Some(HELD), None, false);
    assert_eq!(a.out, vec![HELD]);
    assert_eq!(a.props, vec![prop(0, 1)]);
    assert_eq!(a.tally.tines, 2);
    assert!(a.served.contains(&7));
    assert_eq!(a.skipped.sliver, 0);
}

#[test]
fn the_patches_stay_when_they_hold_as_much_and_the_slivers_count() {
    let a = run(Some(HELD), Some(HELD), false);
    assert_eq!(a.out.len(), 2); // one vertex from each of the two big patches
    assert!(a.props.is_empty() && a.served.is_empty());
    assert_eq!(a.tally.tines, 0);
    assert_eq!(a.skipped.sliver, 1);
}

#[test]
fn an_arc_keeps_a_tie() {
    let a = run(Some(HELD), Some(HELD), true);
    assert_eq!(a.out, vec![HELD]);
    assert!(a.served.contains(&7));
}

#[test]
fn patches_holding_more_win_and_a_tube_that_built_nothing_loses() {
    let a = run(None, Some(HELD), false);
    assert_eq!(a.out.len(), 2);
    assert!(a.served.is_empty());
}

#[test]
fn with_no_big_rival_the_tube_keeps_the_region_at_once() {
    let (topo, res, soup) = world();
    let mut a = Acc::default();
    let mut rv = tube_rivalry(&topo, &res, &soup, &Config::default());
    let mut tube = patch(30.0);
    tube.small_tube = Some(vec![patch(1.0), patch(2.0)]);
    let mut patches = vec![tube];
    let mark = rv.begin(&mut a, &patches[0]).unwrap();
    a.out.push(HELD);
    a.props.push(prop(0, 1));
    rv.tube_done(&mut a, 0, mark, &mut patches);
    rv.finish(&mut a);
    assert_eq!(patches.len(), 1); // nothing queued
    assert_eq!(a.out, vec![HELD]);
    assert!(a.served.contains(&7));
}

#[test]
fn a_tube_with_no_props_is_dropped_and_its_slivers_counted() {
    let (topo, res, soup) = world();
    let mut a = Acc::default();
    let mut rv = tube_rivalry(&topo, &res, &soup, &Config::default());
    let mut tube = patch(30.0);
    tube.small_tube = Some(vec![patch(1.0), patch(2.0)]);
    let mut patches = vec![tube];
    let mark = rv.begin(&mut a, &patches[0]).unwrap();
    rv.tube_done(&mut a, 0, mark, &mut patches);
    assert!(a.served.is_empty());
    assert_eq!(a.skipped.sliver, 2);
}

#[test]
fn a_patch_without_a_rival_begins_with_no_mark_and_settles_nothing() {
    let (topo, res, soup) = world();
    let mut a = Acc::default();
    let mut rv = tube_rivalry(&topo, &res, &soup, &Config::default());
    let patches = [patch(30.0)];
    assert!(rv.begin(&mut a, &patches[0]).is_none());
    rv.finish(&mut a);
    assert!(a.out.is_empty());
}

#[test]
fn a_lattice_struts_reach_bounds_the_held_area_not_its_net() {
    // the tube's own triangles are far from the face; `reach` is on it: only `reach` counts
    let (topo, res, soup) = world();
    let mut a = Acc::default();
    let mut rv = tube_rivalry(&topo, &res, &soup, &Config::default());
    let mut tube = patch(30.0);
    tube.tris = Arc::new(vec![
        500.0, 500.0, 9.0, 501.0, 500.0, 9.0, 500.0, 501.0, 9.0,
    ]);
    tube.reach = Some(Arc::new(vec![
        -1.0, -1.0, 9.0, 1.0, -1.0, 9.0, 0.0, 1.0, 9.0,
    ]));
    tube.small_tube = Some(vec![patch(20.0)]);
    let mut patches = vec![tube];
    let mark = rv.begin(&mut a, &patches[0]).unwrap();
    a.out.push(HELD);
    a.props.push(prop(0, 1));
    rv.tube_done(&mut a, 0, mark, &mut patches);
    rv.begin(&mut a, &patches[1]);
    rv.finish(&mut a); // the patch built nothing: the tube (which holds the face) stays
    assert_eq!(a.out, vec![HELD]);
}
