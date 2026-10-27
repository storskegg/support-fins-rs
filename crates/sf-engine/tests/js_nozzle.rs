//! Port of upstream `tests/nozzle.test.js`: nozzle size (web/nozzle.js). The support widths scale from
//! their 0.4 numbers; the one-line features scale both ways; the structural widths only grow. JS
//! pokes `PROP`/`PERP`/`SWAY` globals and restores them; here a `Config` carries them.
mod builds;
use builds::*;
use sf_engine::config::{Config, Tunables, apply_tunables};
use sf_engine::fins::build::{FinsOpts, build_fins, build_fins_capturing};
use sf_engine::nozzle::{WALL_LINES, apply_nozzle, apply_wall_lines, wall_lines_fit};
use sf_engine::overhangs::analyze;
use sf_testkit::{assert_close, inside_count, load_model, rot_x, tilted_block_topo};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
struct Knobs {
    th: f64,
    tip: f64,
    tine_w: f64,
    perp_th: f64,
    sway_tine_w: f64,
    sway_th_min: f64,
    sway_th_max: f64,
    sway_th_per_mm: f64,
    wall_half: f64,
}
fn knobs(c: &Config) -> Knobs {
    Knobs {
        th: c.prop.th,
        tip: c.prop.tip,
        tine_w: c.prop.tine_w,
        perp_th: c.perp.th,
        sway_tine_w: c.sway.tine_w,
        sway_th_min: c.sway.th_min,
        sway_th_max: c.sway.th_max,
        sway_th_per_mm: c.sway.th_per_mm,
        wall_half: c.sway.wall_half,
    }
}

fn opts(bed_pad: bool, tines: bool, nozzle: f64, wall_lines: f64) -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(bed_pad),
        tines: Some(tines),
        tunables: Some(Tunables {
            nozzle: Some(nozzle),
            wall_lines: Some(wall_lines),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Volume of the supports built for a tilted block (no bed pad), at this nozzle.
fn volume_at(nozzle: f64, wall_lines: f64) -> f64 {
    let topo = Arc::new(tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 30.0, 45.0));
    let res = analyze(&topo, 45.0, &I);
    let built = build_fins(
        &topo,
        &res,
        &I,
        &opts(false, true, nozzle, wall_lines),
        &Config::default(),
    );
    let t = &built.triangles;
    assert!(!t.is_empty(), "nothing built at nozzle {nozzle}");
    // Overlapping solids count twice -- fine for comparing one part at two nozzles.
    t.chunks(3)
        .map(|c| {
            let (a, b, c) = (c[0], c[1], c[2]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

// nozzle.test.js:37
#[test]
fn nozzle_0_4_is_the_tuned_numbers_unchanged() {
    let mut c = Config::default();
    let before = knobs(&c);
    apply_nozzle(&mut c, 0.6);
    apply_nozzle(&mut c, 0.4);
    assert!(knobs(&c) == before, "a round trip to 0.4 moved a width");
    assert!(
        c.prop.th == 1.0 && c.prop.tip == 0.6 && c.prop.tine_w == 0.5,
        "the 0.4 wall / tip / tine are no longer 1.0 / 0.6 / 0.5"
    );
}

// :46
#[test]
fn nozzle_a_bigger_nozzle_widens_everything_a_smaller_one_only_the_one_line_features() {
    let mut c = Config::default();
    let base = knobs(&c);
    apply_nozzle(&mut c, 0.6);
    let k = knobs(&c);
    assert!(
        k.wall_half > base.wall_half,
        "braces keep no further off a wider wedge"
    );
    for (name, v, b) in [
        ("th", k.th, base.th),
        ("tip", k.tip, base.tip),
        ("tineW", k.tine_w, base.tine_w),
        ("perpTh", k.perp_th, base.perp_th),
        ("swayTineW", k.sway_tine_w, base.sway_tine_w),
        ("swayThMin", k.sway_th_min, base.sway_th_min),
        ("swayThMax", k.sway_th_max, base.sway_th_max),
        ("swayThPerMm", k.sway_th_per_mm, base.sway_th_per_mm),
    ] {
        assert_close(v, b * 1.5, 1e-9, &format!("{name} at 0.6"));
    }
    apply_nozzle(&mut c, 0.2);
    assert_close(c.prop.tine_w, base.tine_w / 2.0, 1e-9, "tine at 0.2");
    assert_close(c.prop.tip, base.tip / 2.0, 1e-9, "tip at 0.2");
    assert_close(
        c.sway.tine_w,
        base.sway_tine_w / 2.0,
        1e-9,
        "brace tine at 0.2",
    );
    // A free-standing wall gets no stiffer from a finer nozzle: never below 0.4's.
    let k = knobs(&c);
    assert!(
        k.th == base.th
            && k.perp_th == base.perp_th
            && k.sway_th_min == base.sway_th_min
            && k.sway_th_max == base.sway_th_max
            && k.sway_th_per_mm == base.sway_th_per_mm
            && k.wall_half == base.wall_half,
        "a width shrank at 0.2"
    );
}

// :67
#[test]
fn nozzle_junk_leaves_the_widths_alone() {
    // JS also passes `undefined` and '0.6', which a typed f64 cannot carry.
    let mut c = Config::default();
    let before = knobs(&c);
    for v in [f64::NAN, 0.0, -0.4, 5.0] {
        apply_nozzle(&mut c, v);
    }
    apply_tunables(&mut c, Some(&Tunables::default()));
    assert!(knobs(&c) == before, "junk nozzle moved a width");
}

// :74
#[test]
fn nozzle_reaches_the_build_through_tunables() {
    let (v4, v8, v2) = (
        volume_at(0.4, 2.0),
        volume_at(0.8, 2.0),
        volume_at(0.2, 2.0),
    );
    assert!(
        v8 > v4 * 1.3,
        "0.8 nozzle built {v8:.0} mm3, not wider than 0.4's {v4:.0}"
    );
    assert!(
        v2 <= v4,
        "0.2 nozzle built more ({v2:.0}) than 0.4 ({v4:.0})"
    );
}

// :87
#[test]
fn wall_lines_2_is_the_tuned_wall_more_lines_thicken_only_the_structure() {
    let mut c = Config::default();
    let base = knobs(&c);
    let feet = |c: &Config| {
        [
            c.prop.foot_min,
            c.prop.foot_max,
            c.prop.squat_brim_w,
            c.prop.tip_h,
        ]
    };
    let feet0 = feet(&c);
    apply_wall_lines(&mut c, 4.0);
    assert_close(c.prop.th, 2.0 * base.th, 1e-9, "wall at 4 lines");
    assert_close(c.perp.th, 2.0 * base.perp_th, 1e-9, "wedge at 4 lines");
    assert!(
        c.prop.tip == base.tip && c.prop.tine_w == base.tine_w && c.sway.tine_w == base.sway_tine_w,
        "the tip or a tine widened with the wall lines"
    );
    assert!(
        c.sway.th_min >= c.prop.th,
        "a brace rib is thinner than the wall"
    );
    apply_wall_lines(&mut c, 8.0);
    assert_close(
        c.prop.foot_min - c.prop.th / 2.0,
        feet0[0] - base.th / 2.0,
        1e-9,
        "foot margin past the face",
    );
    let lean = (c.prop.tip_h).atan2((c.prop.th - c.prop.tip) / 2.0) * 180.0 / std::f64::consts::PI;
    assert!(lean >= 55.0 - 1e-9, "an 8-line neck leans {lean:.1}deg");
    for v in [3.0, 0.0, f64::NAN] {
        apply_wall_lines(&mut c, v);
    }
    assert_close(
        c.prop.th,
        4.0 * base.th,
        1e-9,
        "junk wall lines moved the wall",
    );
    apply_wall_lines(&mut c, 2.0);
    assert!(
        knobs(&c) == base && feet(&c) == feet0,
        "back to 2 lines is not the tuned numbers"
    );
}

// :112
#[test]
fn wall_lines_reach_the_build_through_tunables() {
    let (v2, v4) = (volume_at(0.4, 2.0), volume_at(0.4, 4.0));
    assert!(
        v4 > v2 * 1.5,
        "4 lines built {v4:.0} mm3, not thicker than 2's {v2:.0}"
    );
}

// :122 -- a thick wedge's flat top rode the centreline's contact, so on a curved face its edges dug in.
#[test]
fn wall_lines_a_thick_wedge_stays_out_of_a_curved_face() {
    for name in ["cylinder", "tube"] {
        let topo = Arc::new(load_model(name));
        let rot = rot_x(45.0);
        let res = analyze(&topo, 45.0, &rot);
        let b = build_fins(
            &topo,
            &res,
            &rot,
            &opts(false, false, 0.4, 8.0),
            &Config::default(),
        );
        assert!(
            b.fins
                .iter()
                .any(|f| f.kind == sf_engine::fins::build::FinKind::Wedge),
            "{name}: no wedge built"
        );
        let n = inside_count(&topo, &rot, &res.offset, &xyz(&b.triangles));
        assert!(
            n == 0,
            "{name}: {n} wedge vertices inside the part at 8 lines"
        );
    }
}

// :135
#[test]
fn wall_lines_a_squat_brim_stays_under_half_the_row_spacing() {
    let mut c = Config::default();
    apply_nozzle(&mut c, 0.8);
    apply_wall_lines(&mut c, 8.0);
    assert!(
        c.prop.squat_brim_w < c.prop.max_unsupported_span / 2.0,
        "brim half-width {} fuses neighbouring rows",
        c.prop.squat_brim_w
    );
}

// :145
#[test]
fn nozzle_large_format_sizes_scale_like_the_rest_up_to_2_0_mm() {
    let mut c = Config::default();
    let base = knobs(&c);
    apply_nozzle(&mut c, 2.0);
    assert_close(c.prop.th, base.th * 5.0, 1e-9, "wall at 2.0");
    assert_close(c.prop.tine_w, base.tine_w * 5.0, 1e-9, "tine at 2.0");
    assert_close(c.prop.tip, base.tip * 5.0, 1e-9, "tip at 2.0");
    apply_nozzle(&mut c, 2.5);
    assert_close(c.prop.tip, base.tip * 5.0, 1e-9, "a 2.5 nozzle was taken");
    let (v4, v20) = (volume_at(0.4, 2.0), volume_at(2.0, 2.0));
    assert!(
        v20 > v4 * 2.0,
        "2.0 nozzle built {v20:.0} mm3 vs 0.4's {v4:.0}"
    );
}

// :163
#[test]
fn nozzle_every_offered_nozzle_x_wall_lines_stays_under_half_the_row_spacing() {
    let mut c = Config::default();
    for d in [0.2, 0.25, 0.4, 0.6, 0.8, 1.0, 1.2, 1.4, 1.6, 1.8, 2.0] {
        apply_nozzle(&mut c, d);
        for n in WALL_LINES {
            apply_wall_lines(&mut c, n);
            let half = c.prop.max_unsupported_span / 2.0;
            assert!(
                c.prop.th <= half && c.perp.th <= half,
                "{d} x {n}: wall {}, wedge {}",
                c.prop.th,
                c.perp.th
            );
            assert!(
                c.prop.foot_min < half - 0.5,
                "{d} x {n}: foot {} fuses the row",
                c.prop.foot_min
            );
            if wall_lines_fit(n, d) {
                assert_close(
                    c.prop.th,
                    0.5 * f64::max(1.0, d / 0.4) * n,
                    1e-9,
                    &format!("{d} x {n} offered but capped"),
                );
            }
        }
    }
}

// :182 -- the contact tip grows with the nozzle past the 0.5 reach; the tines all but vanished
#[test]
fn nozzle_a_large_nozzle_still_grips_a_curved_underside_nothing_inside_the_part() {
    let topo = Arc::new(load_model("sphere"));
    let rot = rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    let at = |nozzle: f64, tines: bool| {
        build_fins(
            &topo,
            &res,
            &rot,
            &opts(false, tines, nozzle, 2.0),
            &Config::default(),
        )
    };
    let (t4, t20) = (at(0.4, true).tines, at(2.0, true).tines);
    assert!(
        t20 as f64 >= t4 as f64 * 0.7,
        "sphere X45: {t20} tines at 2.0 vs {t4} at 0.4"
    );
    let bare = at(2.0, false);
    let n = inside_count(&topo, &rot, &res.offset, &xyz(&bare.triangles));
    assert!(
        n == 0,
        "{n} support vertices inside the sphere at 2.0 (tines off)"
    );
}

// :196 -- the edge inset grows with the tip; every tine stays inside the cube's 40 mm.
#[test]
fn nozzle_an_edge_row_keeps_its_tip_and_tines_under_the_part() {
    let topo = Arc::new(load_model("cube"));
    let rot = rot_x(30.0);
    let res = analyze(&topo, 45.0, &rot);
    for nozzle in [0.4, 1.0, 2.0] {
        let (_, cap) = build_fins_capturing(
            &topo,
            &res,
            &rot,
            &opts(false, true, nozzle, 2.0),
            &Config::default(),
        );
        assert!(!cap.is_empty(), "no tines at {nozzle}");
        // PROP.tip / tineW as the build left them: the nozzle's, applied to a fresh Config
        let mut c = Config::default();
        apply_nozzle(&mut c, nozzle);
        let out = cap
            .iter()
            .map(|t| t.x.abs())
            .fold(f64::NEG_INFINITY, f64::max)
            + c.prop.tip.max(c.prop.tine_w) / 2.0;
        assert!(
            out < 20.0,
            "{nozzle}: an edge tip reaches x {out:.2}, past the cube's side at 20"
        );
    }
}
