//! Unit tests for `sf_io::scale` (port of web/scale.js). Parity with JS is in `parity_small.rs`.

use sf_io::scale::{scale_about, scale_from_percent};

#[test]
fn scales_about_a_point_and_lands_on_another() {
    let src = [1.0, 2.0, 3.0, -1.0, 0.0, 5.0];
    assert_eq!(
        scale_about(&src, 2.0, [0.0; 3], [0.0; 3]),
        [2.0, 4.0, 6.0, -2.0, 0.0, 10.0]
    );
    assert_eq!(
        scale_about(&src, 1.5, [0.0; 3], [10.0, 20.0, 30.0]),
        [11.5, 23.0, 34.5, 8.5, 20.0, 37.5]
    );
    let p = [10.0, 20.0, 30.0];
    assert_eq!(
        scale_about(&[10.0, 20.0, 30.0, 12.0, 20.0, 30.0], 3.0, p, p),
        [10.0, 20.0, 30.0, 16.0, 20.0, 30.0]
    );
}

#[test]
fn stores_round_to_f32_and_partial_triples_are_cut() {
    let out = scale_about(&[0.1, 0.2, 0.3], 1.0 / 3.0, [0.7; 3], [0.1; 3]);
    assert!(out.iter().all(|&v| (v as f32) as f64 == v));
    assert_eq!(
        scale_about(&[1.0, 2.0, 3.0, 4.0, 5.0], 2.0, [0.0; 3], [0.0; 3]).len(),
        5
    );
    assert!(scale_about(&[], 2.0, [0.0; 3], [0.0; 3]).is_empty());
    assert!(scale_about(&[1.0, 1.0, 1.0], f64::NAN, [0.0; 3], [0.0; 3])[0].is_nan());
}

#[test]
fn percent_clamps_and_rounds_to_two_decimals() {
    assert_eq!(scale_from_percent(150.0), Some(1.5));
    assert_eq!(scale_from_percent(100.0), Some(1.0));
    assert_eq!(scale_from_percent(5.0), Some(0.1));
    assert_eq!(scale_from_percent(5000.0), Some(10.0));
    assert_eq!(scale_from_percent(33.333), scale_from_percent(33.33));
    assert_eq!(scale_from_percent(0.0), None);
    assert_eq!(scale_from_percent(-3.0), None);
    assert_eq!(scale_from_percent(f64::NAN), None);
    assert_eq!(scale_from_percent(f64::INFINITY), None);
}
