//! Port of upstream `tests/drawarc.test.js`: curved drawn walls (local issue 063). Draw's Curve tool
//! takes the two ends and a point the wall runs through, and builds one wall round the circular arc
//! through all three. These pin: a point in line with the ends is the straight wall to the byte, the
//! curve follows its circle (past a half circle too), it is refused when tighter than its foot can
//! follow, and a tined curved wall is watertight with only its tines in the part.
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnOk, DrawnWall, drawn_wall};
use sf_engine::drawarc::{DRAW_MIN_RADIUS, arc_stations, arc_through};
use sf_engine::overhangs::Topology;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::surface::surface_zs_at;
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_testkit::{assert_close, block_topo, inside_count, is_closed, tilted_block_topo};

const OFF: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};
const Z: f64 = 25.0;

/// A level ceiling 25 mm up.
fn slab() -> Topology {
    block_topo(-40.0, 40.0, -40.0, 40.0, Z, Z + 5.0)
}

fn dw(a: [f64; 3], b: [f64; 3], pos: &[f64], opts: &DrawOpts) -> DrawnWall {
    let cfg = Config::default();
    let tris = Arc::new(Tris::new(pos.to_vec()));
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    drawn_wall(a, b, &tris, 0.0, opts, &mut ctx)
}

fn ok(w: &DrawnWall) -> &DrawnOk {
    match w {
        DrawnWall::Ok(o) => o,
        DrawnWall::Refused { reason } => panic!("refused: {reason}"),
    }
}

fn reason(w: &DrawnWall) -> String {
    match w {
        DrawnWall::Refused { reason } => reason.clone(),
        DrawnWall::Ok(_) => String::new(),
    }
}

fn through(m: [f64; 3]) -> DrawOpts<'static> {
    DrawOpts {
        through: Some(m),
        ..Default::default()
    }
}

// drawarc.test.js:17
#[test]
fn drawarc_a_third_point_in_line_with_the_ends_builds_the_straight_wall_exactly() {
    let topo = slab();
    let straight = dw(
        [-10.0, 0.0, Z],
        [10.0, 0.0, Z],
        &topo.pos,
        &DrawOpts::default(),
    );
    let in_line = dw(
        [-10.0, 0.0, Z],
        [10.0, 0.0, Z],
        &topo.pos,
        &through([3.0, 0.0, Z]),
    );
    assert!(straight.ok() && in_line.ok(), "straight wall failed");
    assert!(
        ok(&straight).tris == ok(&in_line).tris,
        "in-line third point changed the wall"
    );
}

// drawarc.test.js:25
#[test]
fn drawarc_the_curve_runs_round_the_circle_through_all_three_clicks() {
    let r_ = 12.0;
    let r = dw(
        [-r_, 0.0, Z],
        [r_, 0.0, Z],
        &slab().pos,
        &through([0.0, r_, Z]),
    );
    assert!(r.ok(), "curved wall failed: {}", reason(&r));
    let o = ok(&r);
    assert_close(
        o.length,
        std::f64::consts::PI * r_,
        1e-6,
        "length is the half circle",
    );
    for p in &o.top {
        assert_close(p[0].hypot(p[1]), r_, 1e-6, "station off the circle");
    }
    assert!(
        o.top.iter().all(|p| p[1] >= -1e-9),
        "the wall went round the wrong side"
    );
}

// drawarc.test.js:34
#[test]
fn drawarc_a_third_click_beyond_the_ends_gives_more_than_a_half_circle() {
    // ends 10 mm apart, the arc out through (0, -8): the long way round
    let c = arc_through([-5.0, 0.0], [5.0, 0.0], [0.0, -8.0]).unwrap();
    assert!(
        c.sweep.abs() > std::f64::consts::PI,
        "sweep {:.0} deg",
        c.sweep.abs() * 180.0 / std::f64::consts::PI
    );
    let st = arc_stations([-5.0, 0.0, Z], [5.0, 0.0, Z], [0.0, -8.0, Z], 1.0, 3.0).unwrap();
    assert!(
        st.iter()
            .any(|p| p[0].abs() < 0.6 && (p[1] + 8.0).abs() < 0.6),
        "the stations miss the third click"
    );
    assert!(
        st[0] == [-5.0, 0.0, Z] && st[st.len() - 1] == [5.0, 0.0, Z],
        "the ends moved"
    );
}

fn nearest_to_third(st: &[[f64; 3]]) -> [f64; 3] {
    let d = |p: &[f64; 3]| p[0].hypot(p[1] - 10.0);
    st.iter()
        .copied()
        .reduce(|b, p| if d(&p) < d(&b) { p } else { b })
        .unwrap()
}

// drawarc.test.js:43
#[test]
fn drawarc_the_clicked_heights_carry_round_the_arc_a_to_the_third_click_to_b() {
    let st = arc_stations(
        [-10.0, 0.0, 10.0],
        [10.0, 0.0, 30.0],
        [0.0, 10.0, 20.0],
        1.0,
        3.0,
    )
    .unwrap();
    let zs: Vec<f64> = st.iter().map(|p| p[2]).collect();
    assert!(
        zs.iter()
            .enumerate()
            .all(|(i, z)| i == 0 || *z >= zs[i - 1] - 1e-9),
        "hint heights not monotonic a -> m -> b"
    );
    assert_close(
        nearest_to_third(&st)[2],
        20.0,
        0.6,
        "the third click's height",
    );
}

// drawarc.test.js:50
#[test]
fn drawarc_a_clockwise_arc_meets_its_third_click_at_that_clicks_height() {
    // m above a left-to-right chord (y up): the arc runs clockwise (negative sweep), and the a -> m
    // -> b height ramp must place m by going round that way
    let (a, b, m) = ([-10.0, 0.0, 10.0], [10.0, 0.0, 30.0], [0.0, 10.0, 20.0]);
    assert!(
        arc_through([a[0], a[1]], [b[0], b[1]], [m[0], m[1]])
            .unwrap()
            .sweep
            < 0.0,
        "expected a clockwise arc"
    );
    let st = arc_stations(a, b, m, 1.0, 3.0).unwrap();
    assert_close(
        nearest_to_third(&st)[2],
        20.0,
        0.6,
        "the third click's height on a clockwise arc",
    );
}

// drawarc.test.js:60
#[test]
fn drawarc_a_curve_tighter_than_the_foot_can_follow_is_refused_with_the_radius() {
    let r_ = DRAW_MIN_RADIUS - 1.0;
    let r = dw(
        [-r_, 0.0, Z],
        [r_, 0.0, Z],
        &slab().pos,
        &through([0.0, r_, Z]),
    );
    assert!(
        !r.ok() && reason(&r).contains("curve too tight \u{2014} 4.0mm radius, needs 5mm"),
        "got {}",
        if r.ok() {
            "a wall".to_string()
        } else {
            reason(&r)
        }
    );
    let k = dw(
        [-DRAW_MIN_RADIUS, 0.0, Z],
        [DRAW_MIN_RADIUS, 0.0, Z],
        &slab().pos,
        &through([0.0, DRAW_MIN_RADIUS, Z]),
    );
    assert!(
        k.ok(),
        "a curve at the minimum radius refused: {}",
        reason(&k)
    );
}

// drawarc.test.js:68
#[test]
fn drawarc_a_tined_curved_wall_is_watertight_and_only_its_tines_enter_the_part() {
    // draw.test.js's tilted block: its 45 deg underside has the sideways normal tines grip by (a
    // level ceiling takes none, straight or curved). Each click on the underside.
    let topo = tilted_block_topo(-20.0, 20.0, -30.0, 30.0, -6.0, 6.0, 45.0);
    let tris = Tris::new(topo.pos.clone());
    let at = |x: f64, y: f64| {
        let zs = surface_zs_at(&tris, x, y);
        [x, y, zs.iter().copied().fold(f64::INFINITY, f64::min)]
    };
    let (a, b, m) = (at(-10.0, 4.0), at(10.0, 4.0), at(0.0, 10.0));
    let opts = DrawOpts {
        through: Some(m),
        tines: true,
        topo: Some(&topo),
        rot: Some(IDENTITY3),
        offset: Some(OFF),
        tine_density: Some(1.0),
        ..Default::default()
    };
    let tined = dw(a, b, &topo.pos, &opts);
    let plain = dw(a, b, &topo.pos, &through(m));
    assert!(
        tined.ok() && plain.ok(),
        "curved wall failed: {}{}",
        reason(&tined),
        reason(&plain)
    );
    assert!(ok(&tined).tines > 0, "no tines on the curved wall");
    let xyz = |w: &DrawnWall| {
        ok(w)
            .tris
            .iter()
            .map(|v| [v[0], v[1], v[2]])
            .collect::<Vec<_>>()
    };
    assert!(is_closed(&xyz(&tined)), "tined curved wall is not closed");
    assert!(
        inside_count(&topo, &IDENTITY3, &OFF, &xyz(&plain)) == 0,
        "the curved wall itself is inside the part"
    );
}
