//! Own tests for `sf_engine::drawarc` and `sf_engine::prop::curved` (JS: drawarc.test.js and
//! curved.test.js build whole fin sets: M3, docs/TEST-MAP.md). Bit-for-bit replay against JS is
//! `tests/parity_drawarc.rs` and `crates/sf-testkit/tests/parity_curved.rs`.

use sf_engine::config::Config;
use sf_engine::drawarc::{DRAW_MIN_RADIUS, arc_stations, arc_through};
use sf_engine::prop::curved::{bends, line_bend};

#[test]
fn min_radius_constant() {
    assert_eq!(DRAW_MIN_RADIUS, 5.0);
}

#[test]
fn arc_through_a_quarter_circle() {
    let a = arc_through(
        [10.0, 0.0],
        [0.0, 10.0],
        [7.071067811865476, 7.071067811865476],
    )
    .unwrap();
    assert!((a.r - 10.0).abs() < 1e-9 && a.cx.abs() < 1e-9 && a.cy.abs() < 1e-9);
    assert!((a.sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    // the same ends through the far side go the long way round, clockwise
    let b = arc_through(
        [10.0, 0.0],
        [0.0, 10.0],
        [-7.071067811865476, -7.071067811865476],
    )
    .unwrap();
    assert!(b.sweep < 0.0 && (b.sweep + 1.5 * std::f64::consts::PI).abs() < 1e-6);
}

#[test]
fn arc_through_rejects_lines_and_huge_radii() {
    assert!(arc_through([0.0, 0.0], [10.0, 0.0], [5.0, 0.0]).is_none());
    assert!(arc_through([0.0, 0.0], [0.0, 0.0], [1.0, 1.0]).is_none());
    assert!(arc_through([0.0, 0.0], [1e5, 1.0], [5e4, 0.5]).is_none()); // r >= 1e4
    assert!(arc_through([f64::NAN, 0.0], [1.0, 0.0], [0.5, 1.0]).is_none()); // NaN: !(r < 1e4)
}

#[test]
fn arc_stations_keep_the_clicked_ends_and_hints() {
    let a = [10.0, 0.0, 1.0];
    let b = [0.0, 10.0, 3.0];
    let m = [7.0710678118654755, 7.0710678118654755, 2.0];
    let s = arc_stations(a, b, m, 1.0, 3.0).unwrap();
    assert!(s.len() >= 16);
    assert_eq!(s[0], a);
    assert_eq!(*s.last().unwrap(), b);
    // the hint passes through m's height near the middle and rises monotonically
    for w in s.windows(2) {
        assert!(w[1][2] >= w[0][2] - 1e-12);
    }
    // min_pts wins over a coarse step
    assert_eq!(arc_stations(a, b, m, 100.0, 8.0).unwrap().len(), 8);
    assert!(arc_stations([0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [2.0, 0.0, 0.0], 1.0, 3.0).is_none());
}

fn arc_line(turn_deg: f64, step: f64) -> Vec<[f64; 3]> {
    let n = (turn_deg.to_radians() * 15.0 / step).ceil() as usize + 1;
    (0..n)
        .map(|i| {
            let a = turn_deg.to_radians() * i as f64 / (n - 1) as f64;
            [15.0 * a.cos(), 15.0 * a.sin(), 4.0]
        })
        .collect()
}

#[test]
fn line_bend_reads_the_turn() {
    let cfg = Config::default();
    assert_eq!(line_bend(&[], &cfg), 0.0);
    assert_eq!(line_bend(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], &cfg), 0.0);
    let straight: Vec<[f64; 3]> = (0..30).map(|i| [f64::from(i), 0.0, 5.0]).collect();
    assert_eq!(line_bend(&straight, &cfg), 0.0);
    // a 90 degree arc reads close to 90 minus the trimmed extremes and the chord's lag
    let b = line_bend(&arc_line(90.0, 0.5), &cfg);
    assert!(b > 60.0 && b < 95.0, "{b}");
    // unwrapping: a 360 degree circle reads far more than one that stays in a half-plane
    assert!(line_bend(&arc_line(360.0, 0.5), &cfg) > 250.0);
    // a line under two chords long is too short to tell a bend from a wobble
    assert_eq!(line_bend(&arc_line(5.0, 0.5), &cfg), 0.0);
}

#[test]
fn bends_uses_min_bend_and_treats_missing_lines_as_empty() {
    let cfg = Config::default();
    let curve = arc_line(180.0, 0.5);
    let straight: Vec<[f64; 3]> = (0..30).map(|i| [f64::from(i), 0.0, 5.0]).collect();
    let empty: &[[f64; 3]] = &[];
    assert!(bends([straight.as_slice(), empty, curve.as_slice()], &cfg));
    assert!(!bends([straight.as_slice(), empty], &cfg));
    assert!(!bends(std::iter::empty::<&[[f64; 3]]>(), &cfg));
}
