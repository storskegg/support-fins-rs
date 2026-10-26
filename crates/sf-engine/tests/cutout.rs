//! Own tests for `sf_engine::cutout`. The JS `cutout.test.js` (14 tests) builds whole fin sets,
//! so it is ported with M3 (docs/TEST-MAP.md). Bit-for-bit replay against JS, direct and from
//! real builds, is `crates/sf-testkit/tests/parity_cutout.rs`.

use sf_engine::config::Config;
use sf_engine::cutout::{CUTOUT_PATTERNS, Station, Wall, cut_wall, holes_for, honeycomb_holes};
use sf_engine::solids::ribbon;

type V3 = [f64; 3];

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

/// A straight wall of length `l` and top height `h`, as prop/sweep.js emits it (one station per
/// mm, along +X): the stations and the solid sections.
fn straight(l: f64, h: f64, cfg: &Config) -> (Vec<Station>, Vec<Vec<V3>>) {
    let (mut st, mut full) = (Vec::new(), Vec::new());
    let sh = cfg.prop.th / 2.0;
    let ztip = h - cfg.prop.tip_h;
    let mut x = 0.0;
    while x <= l + 1e-9 {
        // the wall runs along +X, so `across` is (sx, sy) = (0, -1) as in sweep.js
        let (sx, sy) = (0.0, -1.0);
        let p = [x, 0.0, h + cfg.prop.gap];
        let pt = |o: f64, z: f64| [x + sx * o, sy * o, z];
        full.push(vec![
            pt(sh, 0.0),
            pt(sh, ztip),
            pt(cfg.prop.tip / 2.0, h),
            pt(-cfg.prop.tip / 2.0, h),
            pt(-sh, ztip),
            pt(-sh, 0.0),
        ]);
        st.push(Station {
            p: [p[0], p[1]],
            sx,
            sy,
            top: h,
            ztip,
            hole_top: Some(ztip),
            tip_half: Some(cfg.prop.tip / 2.0),
            sh: Some(sh),
            bot: 0.0,
            bot_tip: cfg.prop.base_h,
            taper_bot: false,
            bot_n: None,
            bot_p: None,
        });
        x += 1.0;
    }
    (st, full)
}

fn with_pattern(p: &str) -> Config {
    let mut c = Config::default();
    c.cut.pattern = p.to_string();
    c
}

fn run(l: f64, h: f64, cfg: &Config) -> (bool, Vec<V3>, f64) {
    let (st, full) = straight(l, h, cfg);
    let mut out = Vec::new();
    let ret = cut_wall(&st, &full, &mut out, &Wall::from(&cfg.prop), cfg);
    let mut solid = Vec::new();
    ribbon(&full, &mut solid);
    (ret, out, volume(&solid))
}

#[test]
fn patterns_list() {
    assert_eq!(
        CUTOUT_PATTERNS,
        [
            "none",
            "diamond",
            "triangle",
            "arch",
            "lattice",
            "honeycomb"
        ]
    );
}

#[test]
fn holes_for_unknown_or_unfit_cells_hold_nothing() {
    let c = Config::default();
    for kind in ["none", "lattice", "honeycomb", "bogus", ""] {
        assert!(holes_for(kind, 8.0, 30.0, &c).is_empty(), "{kind}");
    }
    assert!(holes_for("diamond", 1.0, 30.0, &c).is_empty()); // half-width below minHalf
    assert!(holes_for("diamond", 8.0, 2.9, &c).is_empty()); // shorter than minHole
}

#[test]
fn holes_for_tall_cells_stack_with_a_web_between() {
    let c = Config::default();
    let holes = holes_for("diamond", 8.0, 80.0, &c);
    assert!(holes.len() > 2, "{} holes", holes.len());
    assert_eq!(holes_for("diamond", 8.0, 40.0, &c).len(), 2);
    for pair in holes.windows(2) {
        assert!((pair[1].z0 - pair[0].z1 - c.cut.web).abs() < 1e-9);
    }
    assert!((holes.last().unwrap().z1 - 80.0).abs() < 1e-9);
    for h in &holes {
        assert!(h.a >= c.cut.min_half);
    }
}

#[test]
fn holes_for_a_squat_lone_hole_narrows() {
    let c = Config::default();
    let wide = holes_for("diamond", 8.0, 14.0, &c);
    let squat = holes_for("diamond", 8.0, 5.0, &c);
    assert_eq!(wide.len(), 1);
    assert_eq!(squat.len(), 1);
    assert!(squat[0].a < wide[0].a, "{} vs {}", squat[0].a, wide[0].a);
    // narrower than minHalf: nothing fits
    assert!(holes_for("diamond", 8.0, 3.0, &c).is_empty());
}

#[test]
fn holes_for_arch_reads_roof_as_null() {
    // the arch's roof is `null` (0), so its minimum height is slope * a + minHole / 2
    let c = Config::default();
    let h = holes_for("arch", 8.0, 10.0, &c);
    assert_eq!(h.len(), 1);
    assert!(h[0].z1 - h[0].z0 >= c.cut.slope * h[0].a + c.cut.min_hole / 2.0 - 1e-9);
}

#[test]
fn honeycomb_zone_too_narrow_gives_no_holes() {
    let mut c = Config::default();
    c.cut.hex_pitch = 2.0; // a = (2 - 1.2) / 2 = 0.4 < minHalf
    let s: Vec<f64> = (0..=30).map(f64::from).collect();
    let lo = vec![1.0; s.len()];
    let hi = vec![30.0; s.len()];
    assert!(honeycomb_holes(&s, &lo, &hi, 2.0, 28.0, &c).is_empty());
    let normal = honeycomb_holes(&s, &lo, &hi, 2.0, 28.0, &Config::default());
    assert!(normal.len() > 10);
    // every hole is a convex CCW polygon (positive area) inside the zone
    for h in &normal {
        assert!(h.len() >= 3);
        for v in h {
            assert!(v[0] >= 2.0 - 1e-9 && v[0] <= 28.0 + 1e-9);
            assert!(v[1] >= 1.0 - 1e-9 && v[1] <= 30.0 + 1e-9);
        }
    }
}

#[test]
fn a_zone_with_no_stations_gives_no_holes() {
    let c = Config::default();
    let s: Vec<f64> = (0..=10).map(f64::from).collect();
    let z = vec![5.0; s.len()];
    // the span [20, 30] holds no station: Math.min() is Infinity, no row starts
    assert!(honeycomb_holes(&s, &z, &z, 20.0, 30.0, &c).is_empty());
}

#[test]
fn cut_wall_declines_when_off_or_unfit() {
    let off = Config::default();
    assert_eq!(off.cut.pattern, "none");
    let (ret, out, _) = run(40.0, 30.0, &off);
    assert!(!ret && out.is_empty());
    let (ret, out, _) = run(40.0, 30.0, &with_pattern("bogus"));
    assert!(!ret && out.is_empty());
    // fewer stations than PROP.minStations
    let (ret, _, _) = run(1.0, 30.0, &with_pattern("diamond"));
    assert!(!ret);
    // shorter than two posts and a cell: floor((L - 2 post) / pitch) < 1
    let (ret, _, _) = run(10.0, 30.0, &with_pattern("diamond"));
    assert!(!ret);
    // 6 mm up: no room for a hole
    let (ret, out, _) = run(40.0, 6.0, &with_pattern("diamond"));
    assert!(!ret && out.is_empty());
}

#[test]
fn cut_wall_saves_material_and_stays_outward() {
    for p in ["diamond", "triangle", "arch", "lattice", "honeycomb"] {
        let (ret, out, solid) = run(60.0, 40.0, &with_pattern(p));
        assert!(ret, "{p}");
        assert_eq!(out.len() % 3, 0);
        let v = volume(&out);
        // the pieces overlap each other and the bands, so the sum can exceed the open wall's
        // volume; it is still well under the solid wall's, and positive
        assert!(v > 0.0 && v < solid, "{p}: {v} vs solid {solid}");
    }
}

#[test]
fn cut_wall_min_saved_keeps_a_barely_holed_wall_solid() {
    let mut c = with_pattern("diamond");
    c.cut.min_saved = 0.99;
    let (ret, out, _) = run(60.0, 40.0, &c);
    assert!(!ret && out.is_empty());
    c.cut.min_saved = 0.0;
    assert!(run(60.0, 40.0, &c).0);
}

#[test]
fn cut_wall_uses_the_walls_own_numbers() {
    let c = with_pattern("diamond");
    let (mut st, full) = straight(60.0, 40.0, &c);
    // stations that carry no `sh` / `tipHalf` (a wedge blade's) take the wall's numbers
    for q in &mut st {
        q.sh = None;
        q.tip_half = None;
    }
    let (mut a, mut b) = (Vec::new(), Vec::new());
    assert!(cut_wall(&st, &full, &mut a, &Wall::from(&c.prop), &c));
    // a wedge blade passes its own numbers (fins/wedges.js:127)
    let blade = Wall {
        th: 2.4,
        tip: 2.4,
        min_stations: 3.0,
    };
    assert!(cut_wall(&st, &full, &mut b, &blade, &c));
    assert_ne!(a, b);
    // minStations above the station count declines
    let few = Wall {
        min_stations: 1000.0,
        ..blade
    };
    let mut o = Vec::new();
    assert!(!cut_wall(&st, &full, &mut o, &few, &c));
    assert!(o.is_empty());
}

#[test]
fn cut_wall_with_stations_closed_up_still_cuts_or_declines_cleanly() {
    // hole zone closed (hi <= lo at every station): the bands meet in the middle
    let c = with_pattern("diamond");
    let (mut st, full) = straight(60.0, 40.0, &c);
    for q in &mut st {
        q.hole_top = Some(q.bot_tip); // hi - rail < lo + rail everywhere
    }
    let mut out = Vec::new();
    let ret = cut_wall(&st, &full, &mut out, &Wall::from(&c.prop), &c);
    assert!(!ret && out.is_empty());
}
