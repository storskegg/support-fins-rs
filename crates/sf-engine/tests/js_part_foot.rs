//! Port of upstream `tests/part_foot.test.js`: a WALL STANDING ON THE PART stops PROP.footGap above
//! it (local issue 009): the welded bottom tip scarred the part on every wall of the slenderness
//! coupon, and the foot coupon (prototype/calibration/foot/) printed a 0.2 gap clean.
use std::collections::HashMap;
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnOk, DrawnWall, drawn_wall};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::V4;
use sf_testkit::{block, is_closed};

fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| (x as f32) as f64).collect()
}

fn dw(a: [f64; 3], b: [f64; 3], pos: &[f64], cfg: &Config) -> DrawnWall {
    let tris = Arc::new(Tris::new(pos.to_vec()));
    let mut ctx = BuildCtx::new(cfg, Crest::new(None, 0.0, cfg));
    drawn_wall(a, b, &tris, 0.0, &DrawOpts::default(), &mut ctx)
}

fn ok_of(w: DrawnWall, what: &str) -> DrawnOk {
    match w {
        DrawnWall::Ok(o) => o,
        DrawnWall::Refused { reason } => panic!("wall failed ({what}): {reason}"),
    }
}

fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

fn lo(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::INFINITY, f64::min)
}

fn hi(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::NEG_INFINITY, f64::max)
}

const A: [f64; 3] = [-30.0, 0.0, 35.0];
const B: [f64; 3] = [30.0, 0.0, 35.0];

/// An L: a base block the wall stands on (top z=5), and a shelf 30 mm above it.
fn l_part() -> Vec<f64> {
    f32s(
        &[
            block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
            block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
        ]
        .concat(),
    )
}

/// `lowest(gap)`: PROP.footGap set for the build (the module-state tweak is a `Config` clone).
fn lowest(gap: f64) -> f64 {
    let mut cfg = Config::default();
    cfg.prop.foot_gap = gap;
    let r = ok_of(dw(A, B, &l_part(), &cfg), &format!("footGap {gap}"));
    lo(r.tris.iter().map(|p| p[2]))
}

// part_foot.test.js:22
#[test]
fn part_foot_a_wall_on_the_part_stops_foot_gap_above_it() {
    let g = Config::default().prop.foot_gap;
    assert!(g == 0.2, "footGap {g}: the foot coupon picked 0.2");
    let z = lowest(g);
    assert!(
        (z - 5.2).abs() < 1e-6,
        "lowest point {z}, expected 5.2 (the part top + 0.2)"
    );
}

// part_foot.test.js:28
#[test]
fn part_foot_a_lifted_bottom_keeps_the_full_wall_thickness() {
    // two lines over air, not one 0.6 mm tip line that could peel
    let cfg = Config::default();
    let r = ok_of(dw(A, B, &l_part(), &cfg), "L");
    let ys: Vec<f64> = r
        .tris
        .iter()
        .filter(|p| (p[2] - 5.2).abs() < 1e-6)
        .map(|p| p[1])
        .collect();
    let w = hi(ys.iter().copied()) - lo(ys.iter().copied());
    assert!(
        (w - cfg.prop.th).abs() < 1e-6,
        "bottom {w:.3} mm wide, expected th {}",
        cfg.prop.th
    );
}

// part_foot.test.js:36
#[test]
fn part_foot_the_gap_does_not_change_which_walls_exist_headroom_not_lifted_height() {
    // 1.6 mm from the part to the wall's top: past minHeight 1.5 as headroom, 1.4 once lifted --
    // hub_corner X60 lost a 31 mm wall to exactly this
    let cfg = Config::default();
    let top = 5.0 + 1.6 + cfg.prop.gap;
    let low = f32s(
        &[
            block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
            block(-40.0, 40.0, -10.0, 10.0, top, top + 4.0),
        ]
        .concat(),
    );
    let r = dw([-30.0, 0.0, top], [30.0, 0.0, top], &low, &cfg);
    assert!(r.ok(), "a 1.6 mm headroom wall was refused");
}

// part_foot.test.js:45
#[test]
fn part_foot_foot_gap_0_still_welds_as_before() {
    let z = lowest(0.0);
    assert!(
        (z - 5.0).abs() < 1e-6,
        "lowest point {z}, expected 5 (on the part)"
    );
}

fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Vec<f64> {
    [a, b, c, a, c, d].concat()
}

/// A block whose top is z = zAt(y).
fn ramp(x0: f64, x1: f64, y0: f64, y1: f64, z_at: &dyn Fn(f64) -> f64) -> Vec<f64> {
    let v = [
        [x0, y0, 0.0],
        [x1, y0, 0.0],
        [x1, y1, 0.0],
        [x0, y1, 0.0],
        [x0, y0, z_at(y0)],
        [x1, y0, z_at(y0)],
        [x1, y1, z_at(y1)],
        [x0, y1, z_at(y1)],
    ];
    [
        quad(v[0], v[3], v[2], v[1]),
        quad(v[4], v[5], v[6], v[7]),
        quad(v[0], v[1], v[5], v[4]),
        quad(v[2], v[3], v[7], v[6]),
        quad(v[1], v[2], v[6], v[5]),
        quad(v[0], v[4], v[7], v[3]),
    ]
    .concat()
}

fn shelf() -> Vec<f64> {
    block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0)
}

// A floor sloping ACROSS the wall (56deg here). The lifted bottom is th wide, so the floor is read
// across th and footGap past it (read across the tip only, the bottom's outer edge sat 0.1 mm INSIDE
// the slope), and it TILTS with the floor: level, its downhill side hung 2 mm off the slope (gree:
// up to 2.6 mm, nothing to grip). Straight down is the gap that matters -- a slicer's bottom Z
// distance.

// part_foot.test.js:63
#[test]
fn part_foot_on_a_floor_sloping_across_the_wall_the_bottom_follows_it_foot_gap_above() {
    let cfg = Config::default();
    let z_at = |y: f64| 10.0 + 1.5 * y;
    let tris = f32s(&[ramp(-40.0, 40.0, -3.0, 3.0, &z_at), shelf()].concat());
    let r = ok_of(dw(A, B, &tris, &cfg), "slope");
    let (mut l, mut h) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &r.tris {
        if p[2] > 16.0 {
            continue; // the bottom, not the top
        }
        let d = p[2] - z_at(p[1]);
        l = l.min(d);
        h = h.max(d);
    }
    let g = cfg.prop.foot_gap;
    assert!(
        l > g - 1e-3,
        "bottom {l:.3} mm above the slope, expected >= {g}"
    );
    // uphill it clears the slope footGap past the wall's side too: 0.2 + 1.5 x 0.2
    assert!(
        h < g + 1.5 * g + 1e-3,
        "bottom hangs {h:.3} mm off the slope, expected <= {}",
        g + 1.5 * g
    );
}

// part_foot.test.js:80
#[test]
fn part_foot_a_cut_out_wall_keeps_the_tilted_bottom_and_stays_closed() {
    let z_at = |y: f64| 10.0 + 1.5 * y;
    let tris = f32s(&[ramp(-40.0, 40.0, -3.0, 3.0, &z_at), shelf()].concat());
    let build = |pattern: &str| -> Vec<V4> {
        let mut cfg = Config::default();
        cfg.cut.pattern = pattern.to_string();
        ok_of(dw(A, B, &tris, &cfg), pattern).tris
    };
    let th = Config::default().prop.th;
    // each side's lowest point at every x along the wall
    let lows = |t: &[V4]| -> HashMap<String, f64> {
        let mut m: HashMap<String, f64> = HashMap::new();
        for p in t {
            if (p[1].abs() - th / 2.0).abs() > 1e-3 {
                continue;
            }
            let sign = if p[1] > 0.0 { 1 } else { -1 };
            let k = format!("{}|{}", sf_engine::jsnum::to_fixed(p[0], 3), sign);
            let e = m.entry(k).or_insert(f64::INFINITY);
            *e = e.min(p[2]);
        }
        m
    };
    let cut = build("diamond");
    let none = build("none");
    let solid = lows(&none);
    assert!(is_closed(&xyz(&cut)), "cut wall on a slope is not closed");
    assert!(
        cut.len() != none.len(),
        "no holes were cut -- the test proves nothing"
    );
    let mut shared = 0;
    for (k, z) in lows(&cut) {
        let Some(s) = solid.get(&k) else { continue };
        shared += 1;
        assert!(
            (z - s).abs() < 1e-6,
            "cut bottom at {k} is {z:.3}, solid {s:.3}"
        );
    }
    assert!(shared > 20, "only {shared} bottom points compared");
}

// part_foot.test.js:114
#[test]
fn part_foot_a_ridge_between_the_two_sides_lifts_the_bottom_clear_of_it() {
    // the part bulges up under the middle of the wall: a straight bottom through the two sides'
    // floors would cut the crest
    let cfg = Config::default();
    let up = |y: f64| 10.0 + 1.5 * y;
    let down = |y: f64| 10.0 - 1.5 * y;
    let tris = f32s(
        &[
            ramp(-40.0, 40.0, -3.0, 0.0, &up),
            ramp(-40.0, 40.0, 0.0, 3.0, &down),
            shelf(),
        ]
        .concat(),
    );
    let r = ok_of(dw(A, B, &tris, &cfg), "ridge");
    let mut l = f64::INFINITY;
    for p in &r.tris {
        if p[2] < 16.0 {
            l = l.min(p[2] - up(p[1]).min(down(p[1])));
        }
    }
    let g = cfg.prop.foot_gap;
    assert!(
        l > g - 1e-3,
        "bottom {l:.3} mm above the ridge, expected >= {g}"
    );
    let bottom = lo(r.tris.iter().map(|p| p[2]));
    assert!(
        bottom >= 10.0 + g - 1e-3,
        "bottom at {bottom:.3} dips below the crest + gap"
    );
}

// A rounded bump under the wall, rising along it (gree's leg): stations 1 mm apart bridged its curve
// with straight edges that cut into it between stations. The bottom must MOLD to it: clear it
// everywhere, along every edge, and hug it.
fn bump_floor(r: f64, y0: f64, y1: f64) -> (Vec<f64>, impl Fn(f64) -> f64) {
    let z_at = move |x: f64| {
        if x.abs() < r {
            5.0 + (r * r - x * x).sqrt()
        } else {
            5.0
        }
    };
    let mut xs = Vec::new();
    let mut x = -40.0;
    while x <= 40.0 + 1e-9 {
        // +x.toFixed(4)
        xs.push(sf_engine::jsnum::string_to_number(
            &sf_engine::jsnum::to_fixed(x, 4),
        ));
        x += 0.25;
    }
    let mut t: Vec<f64> = Vec::new();
    let p = |x: f64, y: f64, z: f64| [x, y, z];
    for i in 0..xs.len() - 1 {
        let (a, b) = (xs[i], xs[i + 1]);
        let (za, zb) = (z_at(a), z_at(b));
        t.extend(quad(p(a, y0, za), p(a, y1, za), p(b, y1, zb), p(b, y0, zb))); // top
        t.extend(quad(
            p(a, y0, 0.0),
            p(b, y0, 0.0),
            p(b, y1, 0.0),
            p(a, y1, 0.0),
        )); // bottom
        t.extend(quad(
            p(a, y0, 0.0),
            p(a, y0, za),
            p(b, y0, zb),
            p(b, y0, 0.0),
        )); // -y side
        t.extend(quad(
            p(a, y1, 0.0),
            p(b, y1, 0.0),
            p(b, y1, zb),
            p(a, y1, za),
        )); // +y side
    }
    let (e0, e1) = (xs[0], xs[xs.len() - 1]);
    t.extend(quad(
        p(e0, y0, 0.0),
        p(e0, y1, 0.0),
        p(e0, y1, 5.0),
        p(e0, y0, 5.0),
    ));
    t.extend(quad(
        p(e1, y0, 0.0),
        p(e1, y0, 5.0),
        p(e1, y1, 5.0),
        p(e1, y1, 0.0),
    ));
    (t, z_at)
}

// part_foot.test.js:151
#[test]
fn part_foot_the_bottom_molds_to_a_rounded_bump_clear_along_every_edge_and_close() {
    let (floor, z_at) = bump_floor(6.0, -3.0, 3.0);
    let tris = f32s(&[floor, shelf()].concat());
    let r = ok_of(dw(A, B, &tris, &Config::default()), "bump");
    let (mut l, mut hugging, mut on_bump) = (f64::INFINITY, 0usize, 0usize);
    for i in (0..r.tris.len()).step_by(3) {
        let t = [r.tris[i], r.tris[i + 1], r.tris[i + 2]];
        if hi(t.iter().map(|p| p[2])) > 14.0 {
            continue; // bottom region only
        }
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            for k in 0..=8 {
                // along the edge, not just its ends
                let u = k as f64 / 8.0;
                let x = a[0] + (b[0] - a[0]) * u;
                let z = a[2] + (b[2] - a[2]) * u;
                l = l.min(z - z_at(x));
            }
        }
        for p in &t {
            if p[0].abs() > 5.0 || p[2] - z_at(p[0]) > 3.0 {
                continue; // bottom vertices on the bump
            }
            on_bump += 1;
            if p[2] - z_at(p[0]) < 0.6 {
                hugging += 1;
            }
        }
    }
    assert!(
        l > 0.15,
        "bottom comes {l:.3} mm from the bump (straight down), expected ~0.2"
    );
    assert!(
        on_bump > 30 && hugging as f64 / on_bump as f64 > 0.8,
        "bottom hugs the bump at {hugging}/{on_bump} vertices"
    );
}

// A wall standing at a LEDGE's edge, the ledge ending under it (local issue 032): one side reads no
// floor at all (open to the plate) or a step far below. It used to tilt down MAX_DROP (2.75) toward
// it anyway -- a corner starting mid-air, over nothing (a slicer island; Auto built these on the
// lighthouse, gree, artichoke...). It stays level on the ledge; a step within MAX_DROP still gets
// the tilt. The edge sits under the wall between its centre and one face (y -0.4 of th 1.0): an
// edge past the centre makes the straight bottom clear the ledge, level anyway.
fn ledge(step: f64) -> Vec<f64> {
    let mut v = block(-40.0, 40.0, -10.0, -0.4, 0.0, 5.0);
    if step > 0.0 {
        v.extend(block(-40.0, 40.0, -0.4, 10.0, 0.0, step));
    }
    v.extend(shelf());
    f32s(&v)
}

// part_foot.test.js:187
#[test]
fn part_foot_at_a_ledge_edge_over_open_air_the_bottom_stays_level_on_the_ledge() {
    let g = Config::default().prop.foot_gap;
    for step in [0.0, 1.0] {
        // nothing beside the ledge; a step 4 mm down
        let r = ok_of(
            dw(A, B, &ledge(step), &Config::default()),
            &format!("step {step}"),
        );
        let z = lo(r.tris.iter().map(|p| p[2]));
        assert!(
            (z - (5.0 + g)).abs() < 1e-6,
            "step {step}: lowest point {z:.3}, expected {} (level on the ledge, not hanging past it)",
            5.0 + g
        );
    }
}

// part_foot.test.js:197
#[test]
fn part_foot_at_a_ledge_edge_over_a_step_within_max_drop_the_bottom_still_tilts_to_it() {
    let g = Config::default().prop.foot_gap;
    let r = ok_of(dw(A, B, &ledge(3.5), &Config::default()), "ledge 3.5");
    let z = lo(r.tris.iter().map(|p| p[2]));
    assert!(
        (z - (3.5 + g)).abs() < 1e-6,
        "lowest point {z:.3}, expected {} (footGap over the step)",
        3.5 + g
    );
}

// part_foot.test.js:204
#[test]
fn part_foot_a_floor_just_past_max_drop_keeps_the_capped_tilt_its_corner_just_over_it() {
    // 3.25 mm down (MAX_DROP 2.75 + 0.5): a steep slope, not an open edge -- levelled, the corner sat
    // 3 mm over it (bore_bracket x30y60's bore)
    // (and 3.7, just inside MAX_DROP + MAX_HANG 3.75, still tilts)
    let g = Config::default().prop.foot_gap;
    for down in [3.25, 3.7] {
        let r = ok_of(
            dw(A, B, &ledge(5.0 - down), &Config::default()),
            &format!("{down} down"),
        );
        let z = lo(r.tris.iter().map(|p| p[2]));
        assert!(
            (z - (5.0 - 2.75 + g)).abs() < 1e-6,
            "{down} down: lowest point {z:.3}, expected {} (tilted MAX_DROP)",
            5.0 - 2.75 + g
        );
    }
}

// part_foot.test.js:217
#[test]
fn part_foot_where_the_floor_is_open_plate_the_bottom_stands_on_the_plate_not_foot_gap_over_it() {
    // the base block under only the left half: the wall stands on the part there, and runs down to
    // the plate past its edge -- lifted, that end floated 0.2 mm over the plate with nothing for the
    // slicer to start it on (a figure's hands)
    let half = f32s(&[block(-40.0, 0.0, -10.0, 10.0, 0.0, 5.0), shelf()].concat());
    let r = ok_of(dw(A, B, &half, &Config::default()), "half");
    assert!(
        r.part_attached == Some(true),
        "expected a part-attached wall"
    );
    let low_at = |keep: &dyn Fn(&V4) -> bool| lo(r.tris.iter().filter(|p| keep(p)).map(|p| p[2]));
    let plate = low_at(&|p| p[0] > 2.0);
    let part = low_at(&|p| p[0] < -2.0);
    assert!(
        plate.abs() < 1e-6,
        "over the plate the lowest point is {plate:.3}, expected 0"
    );
    assert!(
        (part - 5.2).abs() < 1e-6,
        "over the part the lowest point is {part:.3}, expected 5.2"
    );
}

// part_foot.test.js:230
#[test]
fn part_foot_a_station_over_a_sloped_toe_with_plate_under_one_side_keeps_foot_gap_on_both_sides() {
    // a 45deg chamfer whose toe (z 0) sits 0.45 mm off the wall's centre: the floor under the near
    // side is plate, under the far side the slope. Lifting only the side over part left the tilted
    // bottom 0.07 mm off the slope near the toe
    let z_at = |y: f64| (y + 0.45).max(0.001);
    let tris = f32s(&[ramp(-40.0, 40.0, -0.45, 3.0, &z_at), shelf()].concat());
    let r = ok_of(dw(A, B, &tris, &Config::default()), "toe");
    assert!(
        r.part_attached == Some(true),
        "expected a part-attached wall"
    );
    // along each station's bottom edge, side to side (its vertices are only at the sides)
    let mut at: Vec<(String, Vec<V4>)> = Vec::new();
    for p in &r.tris {
        if p[2] < 6.0 {
            let k = sf_engine::jsnum::to_fixed(p[0], 2);
            match at.iter_mut().find(|(kk, _)| *kk == k) {
                Some((_, v)) => v.push(*p),
                None => at.push((k, vec![*p])),
            }
        }
    }
    let mut l = f64::INFINITY;
    for (_, vs) in &at {
        let a = vs
            .iter()
            .copied()
            .reduce(|m, p| if p[1] < m[1] { p } else { m })
            .unwrap();
        let b = vs
            .iter()
            .copied()
            .reduce(|m, p| if p[1] > m[1] { p } else { m })
            .unwrap();
        if b[1] - a[1] < 0.5 {
            continue;
        }
        let mut t = 0.0;
        while t <= 1.0 {
            let y = a[1] + (b[1] - a[1]) * t;
            if y >= -0.45 {
                l = l.min(a[2] + (b[2] - a[2]) * t - z_at(y));
            }
            t += 0.02;
        }
    }
    let g = Config::default().prop.foot_gap;
    assert!(
        l > g - 0.05,
        "bottom {l:.3} mm off the slope, expected about footGap {g}"
    );
}
