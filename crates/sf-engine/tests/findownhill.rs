//! Own tests of `sf_engine::fins::downhill` (web/fins/downhill.js): `judge_downhill`'s decision rule
//! on fake builds, the tine capture it swaps and restores, and `used_downhill`. (`held_area` is
//! pinned against JS by `sf-testkit/tests/parity_findownhill.rs`; the JS `downhill.test.js` builds
//! whole parts and is ported with `buildFins`.)

use std::cell::Cell;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt};
use sf_engine::fins::downhill::{Slack, judge_downhill, used_downhill};
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::build::PropRecord;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::{BuildCtx, TineSeed};
use sf_engine::types::IDENTITY3;
use sf_testkit::block;

fn empty() -> FinsBuilt {
    FinsBuilt::empty(Seating {
        kind: SeatingKind::Face,
        span: 0.0,
        bed_area: 0.0,
    })
}

fn fin(kind: FinKind) -> FinRecord {
    FinRecord {
        height: 10.0,
        length: 1.0,
        tines: 0,
        rows: 0,
        stilt: 0.0,
        lean: 0.0,
        bearing: 0.0,
        site: None,
        id: 0,
        kind,
        tri_ranges: vec![],
        line: vec![],
        span: 1.0,
        short: false,
        fill: false,
        joined: false,
        braces: None,
        ribs: None,
    }
}

fn seed(z: f64) -> TineSeed {
    TineSeed {
        x: 0.0,
        y: 0.0,
        z,
        bite_x: 0.0,
        bite_y: 0.0,
    }
}

/// A build as `buildFinsCore` leaves it: `downhill` walls (or not), `wedges` wedges, and the tine
/// seeds it captured.
fn fake(downhill: bool, wedges: usize, ctx: &BuildCtx, caps: &[f64]) -> FinsBuilt {
    let mut b = empty();
    if downhill {
        b.props.push(PropRecord {
            downhill: Some(true),
            ..Default::default()
        });
    }
    for _ in 0..wedges {
        b.fins.push(fin(FinKind::Wedge));
    }
    for &z in caps {
        ctx.capture(seed(z));
    }
    b
}

fn setup() -> (
    sf_engine::overhangs::Topology,
    sf_engine::overhangs::Analysis,
) {
    let topo = build_topology(block(-10.0, 10.0, -10.0, 10.0, 0.0, 10.0));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    (topo, res)
}

#[test]
fn used_downhill_looks_for_a_swapped_region() {
    let mut b = empty();
    assert!(!used_downhill(&b));
    b.props.push(PropRecord::default());
    assert!(!used_downhill(&b));
    b.props.push(PropRecord {
        downhill: Some(true),
        ..Default::default()
    });
    assert!(used_downhill(&b));
}

#[test]
fn a_build_that_swapped_no_region_is_kept_without_building_the_other() {
    let (topo, res) = setup();
    let cfg = Config::default();
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let calls = Cell::new(Vec::<bool>::new());
    let b = judge_downhill(
        &topo,
        &res,
        &IDENTITY3,
        |flag, ctx| {
            let mut v = calls.take();
            v.push(flag);
            calls.set(v);
            fake(false, 2, ctx, &[3.0])
        },
        Slack::Fixed(0.0),
        None,
        &mut ctx,
    );
    assert_eq!(calls.take(), vec![true]);
    assert_eq!(b.fins.len(), 2);
    // no outer capture was set, so none is left behind
    assert_eq!(ctx.cap_len(), None);
}

#[test]
fn downhill_walls_that_lose_a_wedge_are_dropped_for_the_other_build() {
    let (topo, res) = setup();
    let cfg = Config::default();
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let calls = Cell::new(Vec::<bool>::new());
    let b = judge_downhill(
        &topo,
        &res,
        &IDENTITY3,
        |flag, ctx| {
            let mut v = calls.take();
            v.push(flag);
            calls.set(v);
            // downhill: one wedge; without: two -- "keeps every wedge" fails
            fake(flag, if flag { 1 } else { 2 }, ctx, &[3.0])
        },
        Slack::Fixed(0.0),
        None,
        &mut ctx,
    );
    assert_eq!(calls.take(), vec![true, false]);
    assert_eq!(b.fins.len(), 2);
    assert!(!used_downhill(&b));
}

#[test]
fn downhill_walls_that_grip_higher_are_dropped_and_ones_that_do_not_are_kept() {
    let (topo, res) = setup();
    let cfg = Config::default();
    // with downhill: the lowest kept tine is at 50; without: at 1 -> the grip rose: worse
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let b = judge_downhill(
        &topo,
        &res,
        &IDENTITY3,
        |flag, ctx| fake(flag, 1, ctx, if flag { &[50.0] } else { &[1.0] }),
        Slack::Fixed(0.0),
        None,
        &mut ctx,
    );
    assert!(!used_downhill(&b));
    // equal grip and wedges: the downhill build stays
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let b = judge_downhill(
        &topo,
        &res,
        &IDENTITY3,
        |flag, ctx| fake(flag, 1, ctx, &[2.0]),
        Slack::Fixed(0.0),
        None,
        &mut ctx,
    );
    assert!(used_downhill(&b));
    // a build with no tines at all has an infinite lowest grip on both sides: Infinity > Infinity
    // is false, so it is not "worse" on grip
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let b = judge_downhill(
        &topo,
        &res,
        &IDENTITY3,
        |flag, ctx| fake(flag, 0, ctx, &[]),
        Slack::By(&|_| 0.5),
        None,
        &mut ctx,
    );
    assert!(used_downhill(&b));
}

#[test]
fn the_callers_tine_capture_ends_up_holding_only_the_kept_builds_seeds() {
    let (topo, res) = setup();
    let cfg = Config::default();
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    *ctx.tine_cap.borrow_mut() = Some(vec![seed(99.0)]);
    let _ = judge_downhill(
        &topo,
        &res,
        &IDENTITY3,
        // the downhill build is worse (fewer wedges), so the other one's seeds are what stays
        |flag, ctx| {
            fake(
                flag,
                if flag { 0 } else { 1 },
                ctx,
                if flag { &[7.0, 8.0] } else { &[4.0] },
            )
        },
        Slack::Fixed(0.0),
        None,
        &mut ctx,
    );
    let cap = ctx.tine_cap.borrow().clone().unwrap();
    let zs: Vec<f64> = cap.iter().map(|s| s.z).collect();
    assert_eq!(zs, vec![99.0, 4.0]);
}

#[test]
fn a_panicking_build_still_restores_the_capture() {
    let (topo, res) = setup();
    let cfg = Config::default();
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    *ctx.tine_cap.borrow_mut() = Some(vec![seed(1.0)]);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        judge_downhill(
            &topo,
            &res,
            &IDENTITY3,
            |_, _| panic!("boom"),
            Slack::Fixed(0.0),
            None,
            &mut ctx,
        )
    }));
    assert!(r.is_err());
    assert_eq!(ctx.tine_cap.borrow().as_ref().map(Vec::len), Some(1));
}
