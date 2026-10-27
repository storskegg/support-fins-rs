//! Port of upstream `tests/scale.test.js` (part scale): 3 of 3 tests, every assertion.

use sf_engine::jsnum::string_to_number;
use sf_io::scale::{scale_about, scale_from_percent};

fn near(a: &[f64], b: &[f64]) {
    assert!(
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5),
        "{a:?} !== {b:?}"
    );
}

fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| x as f32 as f64).collect()
}

/// `scaleFromPercent('150')`: the JS takes any value and applies `Number(v)`.
fn pct(s: &str) -> Option<f64> {
    scale_from_percent(string_to_number(s))
}

// tests/scale.test.js:11
#[test]
fn scale_about_a_point_landing_on_another_source_untouched() {
    let src = f32s(&[1.0, 2.0, 3.0, -1.0, 0.0, 5.0]);
    near(
        &scale_about(&src, 2.0, [0.0; 3], [0.0; 3]),
        &[2.0, 4.0, 6.0, -2.0, 0.0, 10.0],
    );
    // the centred part back in the file frame: c + s * p
    near(
        &scale_about(&src, 1.5, [0.0; 3], [10.0, 20.0, 30.0]),
        &[11.5, 23.0, 34.5, 8.5, 20.0, 37.5],
    );
    // a slicer part in the file frame, about the same centre: the centre stays put
    let slicer = f32s(&[10.0, 20.0, 30.0, 12.0, 20.0, 30.0]);
    near(
        &scale_about(&slicer, 3.0, [10.0, 20.0, 30.0], [10.0, 20.0, 30.0]),
        &[10.0, 20.0, 30.0, 16.0, 20.0, 30.0],
    );
    near(&src, &[1.0, 2.0, 3.0, -1.0, 0.0, 5.0]);
}

// tests/scale.test.js:22
#[test]
fn scale_centred_part_and_file_frame_extras_agree_after_scaling() {
    let c = [5.0, -3.0, 7.0];
    let file = f32s(&[5.0, -3.0, 7.0, 6.0, -1.0, 8.0]); // the part as the file had it
    let centred: Vec<f64> = file.iter().enumerate().map(|(i, v)| v - c[i % 3]).collect(); // as setPart leaves it
    let centred = f32s(&centred);
    near(
        &scale_about(&centred, 2.5, [0.0; 3], c),
        &scale_about(&file, 2.5, c, c),
    );
}

// tests/scale.test.js:28
#[test]
fn scale_percent_field_to_factor_clamped_and_rounded_junk_refused() {
    assert_eq!(pct("150"), Some(1.5));
    assert_eq!(scale_from_percent(100.0), Some(1.0));
    assert_eq!(pct("5"), Some(0.1)); // the weld's floor (10 %)
    assert_eq!(pct("5000"), Some(10.0));
    assert_eq!(pct("33.333"), pct("33.33")); // what the field shows
    for v in ["", "abc", "0", "-20"] {
        assert_eq!(pct(v), None, "{v:?}");
    }
    assert_eq!(scale_from_percent(f64::NAN), None);
}
