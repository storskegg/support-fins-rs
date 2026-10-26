//! Direct tests of `sf_engine::gridsize::grid_dims` (web/gridsize.js:23-30), one assertion per
//! branch (the clamps, the minimum cell width, the zero/negative extents).

use sf_engine::gridsize::grid_dims;

#[test]
fn small_parts_get_the_minimum_64_cells() {
    assert_eq!(grid_dims(40.0, 40.0, 12.0), [64, 64]);
    assert_eq!(grid_dims(1.0, 1.0, 1.0), [64, 64]);
}

#[test]
fn cells_follow_the_face_count() {
    // h = sqrt(100 * 100 / 10000) = 1 mm -> 100 cells a side
    assert_eq!(grid_dims(100.0, 100.0, 10000.0), [100, 100]);
    // h = sqrt(200 * 100 / 20000) = 1 -> 200 x 100
    assert_eq!(grid_dims(200.0, 100.0, 20000.0), [200, 100]);
    // ceil: 100.5 mm over 1 mm cells is 101 cells
    assert_eq!(grid_dims(100.5, 100.5, 10100.25), [101, 101]);
}

#[test]
fn cells_never_exceed_2048_a_side() {
    assert_eq!(grid_dims(1e6, 1e6, 1e9), [2048, 2048]);
    assert_eq!(grid_dims(5000.0, 10.0, 1e12), [2048, 64]);
}

#[test]
fn cells_are_never_narrower_than_half_a_millimetre() {
    // sqrt(100*100/1e9) is far below 0.5, so h = 0.5 and 100 mm needs 200 cells
    assert_eq!(grid_dims(100.0, 100.0, 1e9), [200, 200]);
    assert_eq!(grid_dims(100.0, 100.0, f64::INFINITY), [200, 200]);
}

#[test]
fn degenerate_extents_and_counts() {
    // extents are floored at 1e-6, a count below 1 is treated as 1
    assert_eq!(grid_dims(0.0, 0.0, 0.0), [64, 64]);
    assert_eq!(grid_dims(-5.0, -5.0, -3.0), [64, 64]);
    assert_eq!(grid_dims(100.0, 0.0, 0.5), [200, 64]); // 1e-6 mm side, 0.5 mm cells
    // the 768 mm figure of the JS comment: h = sqrt(768^2 / 250000) = 1.536 mm
    assert_eq!(grid_dims(768.0, 768.0, 250000.0), [500, 500]);
}
