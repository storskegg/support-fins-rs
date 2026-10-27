//! Own tests for `sf_engine::prop::brace` and `sf_engine::prop::ties` (the JS `brace.test.js` and
//! `ties.test.js` build whole fin sets: M3, docs/TEST-MAP.md). Bit-for-bit replay against JS is
//! `crates/sf-testkit/tests/parity_brace.rs`.

use sf_engine::config::Config;
use sf_engine::prop::brace::{BraceKinds, brace_cross_rib, brace_wall};
use sf_engine::prop::ties::tie_walls;
use sf_engine::tris::Tris;

type V3 = [f64; 3];

fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<f64> {
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let mut t = Vec::new();
    for [a, b, c] in [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ] {
        t.extend(v[a]);
        t.extend(v[b]);
        t.extend(v[c]);
    }
    t
}

fn open() -> Tris {
    Tris::new(block(-200.0, -190.0, -200.0, -190.0, 0.0, 1.0))
}

fn volume(tris: &[V3]) -> f64 {
    tris.chunks(3)
        .map(|t| {
            let (a, b, c) = (t[0], t[1], t[2]);
            (a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

fn wall(len: f64, z: f64) -> Vec<V3> {
    (0..3)
        .map(|i| [-len / 2.0 + len * f64::from(i) / 2.0, 0.0, z])
        .collect()
}

const SLIM: BraceKinds = BraceKinds {
    slim: true,
    tall: false,
};

#[test]
fn a_stocky_wall_gets_no_braces() {
    let cfg = Config::default();
    let top = wall(20.0, 30.0);
    let mut out = Vec::new();
    let n = brace_wall(&top, |_| 0.0, &open(), 0.0, &mut out, |_| true, SLIM, &cfg);
    assert_eq!(n, 0);
    assert!(out.is_empty());
}

#[test]
fn a_slim_wall_gets_two_ends_and_a_face_pair() {
    let cfg = Config::default();
    let top = wall(3.0, 30.0);
    let mut out = Vec::new();
    let n = brace_wall(&top, |_| 0.0, &open(), 0.0, &mut out, |_| true, SLIM, &cfg);
    assert_eq!(n, 4);
    // each rib is a triangular prism (8 triangles) + a flange box (12 triangles)
    assert_eq!(out.len(), 4 * (24 + 36));
    assert!(volume(&out) > 0.0);
}

#[test]
fn kinds_and_degenerate_walls() {
    let cfg = Config::default();
    let tris = open();
    let run = |top: &[V3], kinds: BraceKinds| {
        let mut out = Vec::new();
        brace_wall(top, |_| 0.0, &tris, 0.0, &mut out, |_| true, kinds, &cfg)
    };
    assert_eq!(run(&wall(3.0, 30.0), BraceKinds::default()), 0); // no rule may fire
    assert_eq!(run(&[[0.0, 0.0, 40.0]], SLIM), 0); // one station
    assert_eq!(run(&[[0.0, 0.0, 40.0], [0.0, 0.0, 40.0]], SLIM), 0); // zero length
    // tall: only over BRACE.tallH
    let tall = BraceKinds {
        slim: false,
        tall: true,
    };
    assert_eq!(run(&wall(40.0, 60.0), tall), 0);
    assert_eq!(run(&wall(40.0, 140.0), tall), 2);
}

#[test]
fn the_clear_check_and_a_raised_floor_veto_ribs() {
    let cfg = Config::default();
    let top = wall(3.0, 30.0);
    let mut out = Vec::new();
    assert_eq!(
        brace_wall(&top, |_| 0.0, &open(), 0.0, &mut out, |_| false, SLIM, &cfg),
        0
    );
    assert!(out.is_empty());
    // a floor above the bed by more than onPlate: no rib stands on it
    assert_eq!(
        brace_wall(&top, |_| 5.0, &open(), 0.0, &mut out, |_| true, SLIM, &cfg),
        0
    );
}

#[test]
fn a_part_in_the_way_blocks_a_rib() {
    let cfg = Config::default();
    let top = wall(3.0, 30.0);
    let near = Tris::new(block(-60.0, 60.0, 1.2, 8.0, 0.0, 60.0));
    let mut out = Vec::new();
    let n = brace_wall(&top, |_| 0.0, &near, 0.0, &mut out, |_| true, SLIM, &cfg);
    // the +Y face rib has the part 1.2 mm off the wall: refused at every reach
    assert!(n < 4, "{n}");
}

#[test]
fn cross_rib_needs_height_and_reports_its_reach() {
    let cfg = Config::default();
    let mut out = Vec::new();
    let r = brace_cross_rib(
        &mut out,
        &open(),
        [0.0, 0.0],
        [1.0, 0.0],
        0.0,
        30.0,
        20.0,
        [0.0, 0.0],
        |_| true,
        cfg.brace.rise,
        cfg.prop.th,
        &cfg,
    )
    .unwrap();
    assert_eq!(r.th, cfg.prop.th);
    assert_eq!(r.reach, cfg.prop.foot_ratio * 30.0);
    assert!(!out.is_empty());
    // too short to hold a rib above its flange
    let mut none = Vec::new();
    assert!(
        brace_cross_rib(
            &mut none,
            &open(),
            [0.0, 0.0],
            [1.0, 0.0],
            0.0,
            1.5,
            20.0,
            [0.0, 0.0],
            |_| true,
            0.5,
            1.0,
            &cfg
        )
        .is_none()
    );
    assert!(none.is_empty());
    // a NaN rise falls back to BRACE.rise
    let mut nan = Vec::new();
    let mut base = Vec::new();
    brace_cross_rib(
        &mut nan,
        &open(),
        [0.0, 0.0],
        [1.0, 0.0],
        0.0,
        30.0,
        20.0,
        [0.0, 0.0],
        |_| true,
        f64::NAN,
        1.0,
        &cfg,
    );
    brace_cross_rib(
        &mut base,
        &open(),
        [0.0, 0.0],
        [1.0, 0.0],
        0.0,
        30.0,
        20.0,
        [0.0, 0.0],
        |_| true,
        0.5,
        1.0,
        &cfg,
    );
    assert_eq!(nan, base);
}

#[test]
fn cross_rib_retries_at_half_reach_until_clear() {
    let cfg = Config::default();
    let calls = std::cell::Cell::new(0);
    let mut out = Vec::new();
    let r = brace_cross_rib(
        &mut out,
        &open(),
        [0.0, 0.0],
        [1.0, 0.0],
        0.0,
        80.0,
        40.0,
        [0.0, 0.0],
        |_| {
            calls.set(calls.get() + 1);
            calls.get() > 1
        },
        0.5,
        1.0,
        &cfg,
    )
    .unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(r.reach, cfg.prop.foot_ratio * 80.0 / 2.0);
}

#[test]
fn ties_span_limits_and_struts() {
    let cfg = Config::default();
    let t = open();
    let run = |a: [f64; 2], b: [f64; 2], z_max: f64, tris: &Tris| {
        let mut out = Vec::new();
        let n = tie_walls(a, b, z_max, tris, 0.0, &mut out, &cfg);
        (n, out)
    };
    assert_eq!(run([0.0, 0.0], [1.9, 0.0], 60.0, &t).0, 0); // closer than minSpan
    assert_eq!(run([0.0, 0.0], [12.1, 0.0], 60.0, &t).0, 0); // farther than maxSpan
    assert_eq!(run([1.0, 1.0], [1.0, 1.0], 60.0, &t).0, 0);
    assert_eq!(run([0.0, 0.0], [6.0, 0.0], 5.0, &t).0, 0); // not even one strut fits
    let (n, out) = run([0.0, 0.0], [6.0, 0.0], 60.0, &t);
    // climb = 6 * tan 55 = 8.57; floor((60 - 1.2 - 0.35 tan 55) / 8.57) = 6
    assert_eq!(n, 6);
    assert_eq!(out.len(), n * 36);
    assert!(volume(&out) > 0.0);
    // NaN limit: floor(NaN) < 1 is false, but the loop never runs: no struts
    assert_eq!(run([0.0, 0.0], [6.0, 0.0], f64::NAN, &t).0, 0);
}

#[test]
fn a_part_between_two_walls_blocks_the_tie_untouched_out() {
    let cfg = Config::default();
    let pillar = Tris::new(block(2.0, 4.0, -3.0, 3.0, 0.0, 80.0));
    let mut out = Vec::new();
    assert_eq!(
        tie_walls([0.0, 0.0], [6.0, 0.0], 60.0, &pillar, 0.0, &mut out, &cfg),
        0
    );
    assert!(out.is_empty());
}
