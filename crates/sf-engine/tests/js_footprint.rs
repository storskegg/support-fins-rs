//! Port of upstream `tests/footprint.test.js` (footprints and Arrange, local issue 052 PR 4): 15 of 15 tests,
//! every assertion, and `tests/platerow.test.js` (2 of 2).

use sf_engine::footprint::{
    Grid, area, bed_grid, dilate, drag_step, find_spot, pack, placed, plate_under, rasterize,
    shared, take, union,
};

/// a flat box's two top triangles, x0..x1 by y0..y1 (z matters not)
fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<f64> {
    vec![
        x0, y0, 1.0, x1, y0, 1.0, x1, y1, 1.0, x0, y0, 1.0, x1, y1, 1.0, x0, y1, 1.0,
    ]
}

fn ras(t: &[f64]) -> Grid {
    // Float32Array in the JS: the values are f32-rounded
    let t: Vec<f64> = t.iter().map(|&v| v as f32 as f64).collect();
    rasterize(&t, None).unwrap()
}

fn cells(g: &Grid) -> i64 {
    area(Some(g))
}

fn sh(a: &Grid, b: &Grid) -> usize {
    shared(Some(a), Some(b)).len()
}

// tests/footprint.test.js:26
#[test]
fn rasterize_a_10_x_4_box_covers_10_x_4_cells_no_more() {
    let g = ras(&bx(0.0, 0.0, 10.0, 4.0));
    // the far edges sit on cell lines: the cells past them are touched at the line
    assert!(cells(&g) >= 40 && cells(&g) <= 11 * 5, "got {}", cells(&g));
    let h = ras(&bx(0.2, 0.2, 9.8, 3.8));
    assert!(cells(&h) == 40, "inside the lines: {}", cells(&h));
}

// tests/footprint.test.js:34
#[test]
fn rasterize_a_thin_diagonal_sliver_still_marks_its_cells() {
    // a wall seen from above: a near-zero-width triangle from (0.5,0.5) to (20.5,10.5)
    let g = ras(&[0.5, 0.5, 0.0, 20.5, 10.5, 0.0, 20.5, 10.5, 5.0]);
    assert!(
        cells(&g) >= 21,
        "a 20 mm diagonal crosses at least 21 cells, got {}",
        cells(&g)
    );
    // and leaves the far corners of its box alone
    let at = |x: i64, y: i64| g.bits[((y - g.y0) * g.w + x - g.x0) as usize] != 0;
    assert!(
        !at(20, 0) && !at(0, 10),
        "box corners away from the line stay free"
    );
}

// tests/footprint.test.js:43
#[test]
fn rasterize_a_matrix_moves_the_vertices_first() {
    let m = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 30.0, -5.0, 0.0, 1.0,
    ]; // translate (30,-5)
    let t: Vec<f64> = bx(0.2, 0.2, 2.8, 2.8)
        .iter()
        .map(|&v| v as f32 as f64)
        .collect();
    let g = rasterize(&t, Some(&m)).unwrap();
    assert!(
        g.x0 == 30 && g.y0 == -5 && g.w == 3 && g.h == 3,
        "{:?}",
        [g.x0, g.y0, g.w, g.h]
    );
}

// an L: 30 x 10 along X plus 10 x 30 up Y, its inner corner (10..30, 10..30) empty
fn l_shape() -> Grid {
    union(&[
        Some(&ras(&bx(0.2, 0.2, 29.8, 9.8))),
        Some(&ras(&bx(0.2, 0.2, 9.8, 29.8))),
    ])
    .unwrap()
}

// tests/footprint.test.js:55
#[test]
fn shared_a_cube_in_an_ls_corner_is_clear_the_boxes_would_clash() {
    let cube = ras(&bx(0.2, 0.2, 9.8, 9.8));
    let in_corner = placed(&cube, [15.0, 15.0]);
    assert!(
        sh(&placed(&l_shape(), [0.0, 0.0]), &in_corner) == 0,
        "no shared cell"
    );
    let touching = placed(&cube, [5.0, 5.0]);
    assert!(
        sh(&placed(&l_shape(), [0.0, 0.0]), &touching) > 0,
        "overlapping shares cells"
    );
}

// tests/footprint.test.js:63
#[test]
fn placed_off_the_mm_grid_a_footprint_widens_never_narrows() {
    let cube = ras(&bx(0.2, 0.2, 9.8, 9.8)); // 10 cells wide
    let a = placed(&cube, [0.0, 0.0]);
    // 0.3 mm off a whole-mm gap of 0: still the neighbouring cell
    let b = placed(&cube, [10.3, 0.0]);
    assert!(b.w == 11 && b.x0 == 10, "widened to {} from {}", b.w, b.x0);
    assert!(
        sh(&a, &placed(&cube, [9.7, 0.0])) > 0,
        "a 0.3 mm overlap warns"
    );
    assert!(
        sh(&a, &placed(&cube, [11.0, 0.0])) == 0,
        "a 1 mm gap on the grid is clear"
    );
}

// tests/footprint.test.js:73
#[test]
fn dilate_grows_by_r_every_way() {
    let g = dilate(&ras(&bx(0.2, 0.2, 0.8, 0.8)), 3);
    assert!(
        g.w == 7 && g.h == 7 && cells(&g) == 49,
        "{}x{} {}",
        g.w,
        g.h,
        cells(&g)
    );
}

// tests/footprint.test.js:78
#[test]
fn pack_five_models_on_a_180_bed_all_on_it_10_mm_clear_of_each_other() {
    let grids = [
        ras(&bx(-20.0, -20.0, 20.0, 20.0)),
        l_shape(),
        ras(&bx(-30.0, -5.0, 30.0, 5.0)),
        ras(&bx(-8.0, -8.0, 8.0, 8.0)),
        ras(&bx(-25.0, -12.0, 25.0, 12.0)),
    ];
    let refs: Vec<Option<&Grid>> = grids.iter().map(Some).collect();
    let at = pack(&refs, 180.0, 180.0, 10);
    assert!(at.iter().all(Option::is_some), "room for all");
    let pl: Vec<Grid> = grids
        .iter()
        .zip(&at)
        .map(|(g, a)| {
            let a = a.unwrap();
            placed(g, [a[0] as f64, a[1] as f64])
        })
        .collect();
    for p in &pl {
        assert!(
            p.x0 >= -90 && p.x0 + p.w <= 90 && p.y0 >= -90 && p.y0 + p.h <= 90,
            "on the bed"
        );
    }
    for a in 0..pl.len() {
        for b in (a + 1)..pl.len() {
            // 10 cells of growth on one still leaves them clear: 10 empty cells between
            assert!(
                sh(&dilate(&pl[a], 10), &pl[b]) == 0,
                "models {a},{b} too close"
            );
        }
    }
    // the biggest takes the middle
    let big = &pl[0];
    assert!(
        ((big.x0 as f64 + big.w as f64 / 2.0).abs() <= 1.0)
            && ((big.y0 as f64 + big.h as f64 / 2.0).abs() <= 1.0),
        "biggest at the centre"
    );
}

// tests/footprint.test.js:103
#[test]
fn pack_a_model_the_bed_has_no_room_left_for_gets_null() {
    let g = ras(&bx(-40.0, -40.0, 40.0, 40.0));
    let at = pack(&[Some(&g), Some(&g), Some(&g)], 100.0, 100.0, 10);
    assert!(at.iter().flatten().count() == 1, "{at:?}");
    let too_big = ras(&bx(0.0, 0.0, 120.0, 5.0));
    assert!(
        pack(&[Some(&too_big)], 100.0, 100.0, 10)[0].is_none(),
        "too big for the bed"
    );
}

// tests/footprint.test.js:109
#[test]
fn find_spot_beside_what_is_taken_gap_kept() {
    let mut bed = bed_grid(200.0, 200.0);
    let g = ras(&bx(-10.0, -10.0, 10.0, 10.0));
    take(&mut bed, Some(&placed(&g, [0.0, 0.0])));
    let at = find_spot(&bed, &g, 10, 5);
    let at = at.expect("found one");
    assert!(
        sh(
            &dilate(&placed(&g, [0.0, 0.0]), 10),
            &placed(&g, [at[0] as f64, at[1] as f64])
        ) == 0,
        "kept clear"
    );
    assert!(
        ((at[0] * at[0] + at[1] * at[1]) as f64).sqrt() <= 33.0,
        "near the middle: {at:?}"
    );
}

// a 10 x 10 model dragged toward one standing at x 20..30, y 0..10
fn drag_world() -> (Grid, Grid) {
    let me = ras(&bx(0.2, 0.2, 9.8, 9.8));
    let wall = placed(&me, [20.0, 0.0]);
    (me, wall)
}

fn clash_of<'a>(me: &'a Grid, wall: &'a Grid) -> impl Fn([f64; 2]) -> f64 + 'a {
    move |xy| sh(&placed(me, xy), wall) as f64
}

// tests/footprint.test.js:126
#[test]
fn drag_step_a_clear_step_is_taken_whole() {
    let (me, wall) = drag_world();
    let xy = drag_step([0.0, 0.0], [5.0, -3.0], clash_of(&me, &wall));
    assert!(xy[0] == 5.0 && xy[1] == -3.0, "got {xy:?}");
}

// tests/footprint.test.js:131
#[test]
fn drag_step_a_step_into_another_model_stops_where_it_is() {
    let (me, wall) = drag_world();
    let clash = clash_of(&me, &wall);
    let at = [10.0, 0.0]; // touching: a cell further shares the wall's first column
    assert!(
        clash(at) == 0.0 && clash([10.1, 0.0]) > 0.0,
        "the setup touches"
    );
    assert!(
        drag_step(at, [15.0, 0.0], &clash) == at,
        "straight into it: it stays"
    );
}

// tests/footprint.test.js:137
#[test]
fn drag_step_a_diagonal_into_another_model_slides_along_its_edge() {
    let (me, wall) = drag_world();
    let clash = clash_of(&me, &wall);
    // pressing right and up against its side: the x goes nowhere, the y still does
    let xy = drag_step([10.0, 0.0], [14.0, 4.0], &clash);
    assert!(xy[0] == 10.0 && xy[1] == 4.0, "got {xy:?}");
    // below its corner, clear of it: the whole step
    let yx = drag_step([9.0, -12.0], [12.0, -11.0], &clash);
    assert!(yx[0] == 12.0 && yx[1] == -11.0, "clear whole: {yx:?}");
}

// tests/footprint.test.js:145
#[test]
fn drag_step_one_already_in_a_clash_may_come_out_not_go_deeper() {
    let (me, wall) = drag_world();
    let clash = clash_of(&me, &wall);
    let at = [15.0, 0.0];
    assert!(clash(at) > 0.0, "starts inside");
    assert!(drag_step(at, [18.0, 0.0], &clash) == at, "deeper: it stays");
    let out = drag_step(at, [12.0, 0.0], &clash);
    assert!(out[0] == 12.0 && clash(out) < clash(at), "out: {out:?}");
}

// tests/footprint.test.js:153
#[test]
fn drag_step_a_fast_step_into_another_model_still_stops_at_its_edge() {
    let (me, wall) = drag_world();
    let clash = clash_of(&me, &wall);
    // 15 mm in one pointer event, straight at it: it gets to within a cell, not 15 short
    let xy = drag_step([0.0, 0.0], [15.0, 0.0], &clash);
    assert!(clash(xy) == 0.0 && xy[0] >= 9.5, "got {xy:?}");
}

// tests/footprint.test.js:159
#[test]
fn drag_step_a_jump_to_a_clear_spot_past_another_model_is_taken() {
    let (me, wall) = drag_world();
    let xy = drag_step([0.0, 0.0], [40.0, 0.0], clash_of(&me, &wall));
    assert!(xy[0] == 40.0 && xy[1] == 0.0, "got {xy:?}");
}

// three 250 x 220 beds, 280 mm apart, the middle one edited
fn under(xy: [f64; 2]) -> Option<(usize, [f64; 2])> {
    plate_under(xy, 1.0, 3.0, 280.0, 250.0, 220.0)
}

// tests/platerow.test.js:11
#[test]
fn plate_under_over_the_next_plate_its_spot_from_that_plates_middle() {
    let r = under([300.5, -40.0]).unwrap();
    assert!(r.0 == 2 && r.1[0] == 20.5 && r.1[1] == -40.0, "{r:?}");
    let l = under([-280.0, 0.0]).unwrap();
    assert!(l.0 == 0 && l.1[0] == 0.0, "{l:?}");
}

// tests/platerow.test.js:18
#[test]
fn plate_under_the_edited_plate_a_gap_or_past_the_row_is_nothing() {
    assert!(under([100.0, 0.0]).is_none(), "own plate");
    assert!(under([140.0, 0.0]).is_none(), "the gap between beds");
    assert!(
        under([290.0, 115.0]).is_none(),
        "off the next bed's far side"
    );
    assert!(under([560.0, 0.0]).is_none(), "past the last plate");
}
