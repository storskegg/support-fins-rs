//! Direct tests of `sf_engine::prop::bridgerace` (web/prop/bridgerace.js): the turned-walls race.

mod common;
use common::ctx;
use std::sync::Arc;

use sf_engine::tris::Tris;

use sf_engine::config::Config;
use sf_engine::prop::bridgerace::{RACE_ABS, RACE_REL, bridge_race};
use sf_engine::prop::build::{Acc, Lines, Patch, PropRecord};
use sf_engine::prop::ctx::TineSeed;

fn patch() -> Patch {
    // a 20 x 20 mm face at z = 10
    Patch {
        region: 3,
        area: 200.0,
        tris: Arc::new(Tris::new(vec![
            0.0, 0.0, 10.0, 20.0, 0.0, 10.0, 0.0, 20.0, 10.0, 20.0, 0.0, 10.0, 20.0, 20.0, 10.0,
            0.0, 20.0, 10.0,
        ])),
        ..Default::default()
    }
}

fn turned() -> Lines {
    Lines {
        turned: Some([0.0, 1.0]),
        ..Default::default()
    }
}

/// A wall of `n` vertices spread over the face, 2 mm below it.
fn wall(n: usize) -> Vec<[f64; 4]> {
    (0..n)
        .map(|i| [(i as f64 * 7.0) % 20.0, (i as f64 * 3.0) % 20.0, 9.0, 0.0])
        .collect()
}

fn build(acc: &mut Acc, verts: Vec<[f64; 4]>) {
    let base = acc.out.len();
    acc.out.extend(verts);
    acc.props.push(PropRecord {
        region: Some(3),
        tri_ranges: vec![[base, acc.out.len()]],
        ..Default::default()
    });
}

#[test]
fn constants_are_the_js_ones() {
    assert_eq!((RACE_REL, RACE_ABS), (0.02, 2.0));
}

#[test]
fn an_unturned_patch_queues_no_twin() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let acc = Acc::default();
    let mut race = bridge_race();
    let mut patches = vec![patch()];
    race.turned(&acc, 0, &Lines::default(), &mut patches, &ctx);
    assert_eq!(patches.len(), 1);
}

#[test]
fn a_turned_patch_queues_its_old_way_twin_unless_it_is_marked() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let acc = Acc::default();
    for veto in 0..5 {
        let mut p = patch();
        match veto {
            1 => p.bridge_twin = true,
            2 => p.rival = Some(0),
            3 => p.small_tube = Some(vec![]),
            4 => p.axis = Some([1.0, 0.0]),
            _ => {}
        }
        let mut race = bridge_race();
        let mut patches = vec![p, patch()];
        race.turned(&acc, 0, &turned(), &mut patches, &ctx);
        assert_eq!(patches.len(), if veto == 0 { 3 } else { 2 }, "veto {veto}");
        if veto == 0 {
            assert!(patches[1].bridge_twin && patches[1].axis == Some([0.0, 1.0]));
            assert!(!patches[2].bridge_twin);
        }
    }
}

/// Runs the loop with `turned_wall` the turned patch's build and `old_wall` its twin's.
fn race(turned_wall: Vec<[f64; 4]>, old_wall: Vec<[f64; 4]>) -> Acc {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    let mut acc = Acc::default();
    let mut race = bridge_race();
    let mut patches = vec![patch()];
    let mut i = 0;
    while i < patches.len() {
        race.begin(&mut acc, &patches[i], &ctx);
        if i == 0 {
            race.turned(&acc, 0, &turned(), &mut patches, &ctx);
            build(&mut acc, turned_wall.clone());
        } else {
            build(&mut acc, old_wall.clone());
        }
        i += 1;
    }
    race.finish(&mut acc, &ctx);
    acc
}

#[test]
fn the_turned_walls_stay_on_a_tie_and_the_twin_is_cut_off() {
    let acc = race(wall(30), wall(30));
    assert_eq!(acc.out.len(), 30);
    assert_eq!(acc.props.len(), 1);
}

#[test]
fn the_old_walls_replace_the_turned_ones_when_they_hold_clearly_more() {
    let acc = race(wall(3), wall(60));
    assert_eq!(acc.out.len(), 60);
    assert_eq!(acc.props.len(), 1);
    assert_eq!(acc.props[0].tri_ranges, vec![[0, 60]]);
    assert!(acc.served.is_empty() || acc.served.contains(&3));
    // the old walls built a prop, so the region reads served
    assert!(acc.served.contains(&3));
}

#[test]
fn the_tine_capture_follows_the_walls_that_stay() {
    let cfg = Config::default();
    let ctx = ctx(&cfg);
    *ctx.tine_cap.borrow_mut() = Some(vec![]);
    let seed = |x: f64| TineSeed {
        x,
        y: 0.0,
        z: 0.0,
        bite_x: 0.0,
        bite_y: 0.0,
    };
    let mut acc = Acc::default();
    let mut race = bridge_race();
    let mut patches = vec![patch()];
    let mut i = 0;
    while i < patches.len() {
        race.begin(&mut acc, &patches[i], &ctx);
        let c0 = ctx.cap_len().unwrap();
        if i == 0 {
            race.turned(&acc, 0, &turned(), &mut patches, &ctx);
            build(&mut acc, wall(3));
            ctx.capture(seed(1.0));
        } else {
            build(&mut acc, wall(60));
            ctx.capture(seed(2.0));
            ctx.capture(seed(3.0));
        }
        acc.props.last_mut().unwrap().caps = Some([c0, ctx.cap_len().unwrap()]);
        i += 1;
    }
    race.finish(&mut acc, &ctx);
    let xs: Vec<f64> = ctx
        .tine_cap
        .borrow()
        .as_ref()
        .unwrap()
        .iter()
        .map(|t| t.x)
        .collect();
    assert_eq!(xs, vec![2.0, 3.0]);
    assert_eq!(acc.props[0].caps, Some([0, 2]));
}
