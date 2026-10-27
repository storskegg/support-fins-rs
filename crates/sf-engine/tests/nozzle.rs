//! Own tests for `sf_engine::nozzle` and `sf_engine::materials`. (The JS `nozzle.test.js` builds
//! whole fin sets, so it is ported with M3; see docs/TEST-MAP.md.) Bit-for-bit replay against
//! V8 is in `crates/sf-testkit/tests/parity_config.rs`.

use sf_engine::config::Config;
use sf_engine::materials::{MATERIAL, PETG, PLA, material};
use sf_engine::nozzle::{
    NOZZLE_MAX, NOZZLE_REF, TH_MAX, WALL_LINES, apply_nozzle, apply_wall_lines, rescale,
    wall_lines_fit,
};

fn dbg(c: &Config) -> String {
    format!("{c:?}")
}

#[test]
fn constants_are_the_js_values() {
    assert_eq!(NOZZLE_REF, 0.4);
    assert_eq!(NOZZLE_MAX, 2.0);
    assert_eq!(TH_MAX, 6.0);
    assert_eq!(WALL_LINES, [2.0, 4.0, 6.0, 8.0]);
}

#[test]
fn reference_nozzle_and_two_lines_leave_every_number_as_it_was() {
    let mut c = Config::default();
    apply_nozzle(&mut c, 0.4);
    apply_wall_lines(&mut c, 2.0);
    assert_eq!(dbg(&c), dbg(&Config::default()));
    rescale(&mut c);
    assert_eq!(dbg(&c), dbg(&Config::default()));
}

#[test]
fn junk_nozzles_are_ignored() {
    for d in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -0.4,
        0.0999,
        2.0001,
        2.5,
    ] {
        let mut c = Config::default();
        apply_nozzle(&mut c, d);
        assert_eq!(dbg(&c), dbg(&Config::default()), "nozzle {d}");
    }
}

#[test]
fn nozzle_bounds_are_inclusive() {
    for d in [0.1, 2.0] {
        let mut c = Config::default();
        apply_nozzle(&mut c, d);
        assert_eq!(c.nozzle.nozzle, d);
    }
}

#[test]
fn junk_wall_lines_are_ignored() {
    for n in [f64::NAN, 0.0, 1.0, 3.0, 5.0, 10.0, -2.0, 2.5] {
        let mut c = Config::default();
        apply_wall_lines(&mut c, n);
        assert_eq!(dbg(&c), dbg(&Config::default()), "lines {n}");
    }
    for n in WALL_LINES {
        let mut c = Config::default();
        apply_wall_lines(&mut c, n);
        assert_eq!(c.nozzle.wall_lines, n);
    }
}

#[test]
fn finer_nozzle_scales_one_line_features_down_and_keeps_structure() {
    let mut c = Config::default();
    apply_nozzle(&mut c, 0.2);
    let d = Config::default();
    assert_eq!(c.prop.tip, d.prop.tip * 0.5);
    assert_eq!(c.prop.tine_w, d.prop.tine_w * 0.5);
    assert_eq!(c.sway.tine_w, d.sway.tine_w * 0.5);
    // structural widths only grow
    assert_eq!(c.prop.th, d.prop.th);
    assert_eq!(c.perp.th, d.perp.th);
    assert_eq!(c.sway.th_min, d.sway.th_min);
    // a tip narrower than the 0.4 one adds no reach (max(0, negative))
    assert_eq!(c.prop.tine_reach_tip, 0.0);
    assert_eq!(c.prop.edge_inset, d.prop.edge_inset);
}

#[test]
fn wall_is_capped_at_th_max() {
    let mut c = Config::default();
    apply_nozzle(&mut c, 2.0);
    apply_wall_lines(&mut c, 8.0);
    assert_eq!(c.prop.th, TH_MAX);
    assert_eq!(c.perp.th, TH_MAX);
    // the brim stays under half the row spacing
    assert!(c.prop.squat_brim_w <= (c.prop.max_unsupported_span - 1.0) / 2.0);
    // the brace rib is never thinner than the wall
    assert!(c.sway.th_min >= c.prop.th && c.sway.th_max >= c.prop.th);
}

#[test]
fn wall_lines_after_nozzle_rescale_from_the_base_not_the_previous_state() {
    let mut a = Config::default();
    apply_nozzle(&mut a, 0.8);
    apply_wall_lines(&mut a, 4.0);
    let mut b = Config::default();
    apply_wall_lines(&mut b, 4.0);
    apply_nozzle(&mut b, 0.8);
    assert_eq!(dbg(&a), dbg(&b));
    // and going back restores the 0.4 numbers exactly
    apply_nozzle(&mut a, 0.4);
    apply_wall_lines(&mut a, 2.0);
    assert_eq!(dbg(&a), dbg(&Config::default()));
}

#[test]
fn wall_lines_fit_matches_the_th_max_rule() {
    assert!(wall_lines_fit(2.0, 0.4));
    assert!(wall_lines_fit(8.0, 0.4)); // 1.0 * 1 * 8 / 2 = 4
    assert!(wall_lines_fit(6.0, 0.4));
    assert!(!wall_lines_fit(8.0, 0.8)); // 1.0 * 2 * 8 / 2 = 8 > 6
    assert!(wall_lines_fit(6.0, 0.8)); // 6 <= 6 + 1e-9
    assert!(!wall_lines_fit(8.0, 2.0));
    // a finer nozzle does not shrink the wall: max(1, d / 0.4)
    assert!(wall_lines_fit(8.0, 0.2));
    // NaN compares false
    assert!(!wall_lines_fit(f64::NAN, 0.4));
}

#[test]
fn materials_table() {
    assert_eq!(MATERIAL.len(), 2);
    assert_eq!(MATERIAL[0].0, "pla");
    assert_eq!(MATERIAL[1].0, "petg");
    assert_eq!(material("pla"), Some(PLA));
    assert_eq!(material("petg"), Some(PETG));
    assert_eq!(material("abs"), None);
    assert_eq!(PETG.pad_grab.to_bits(), (-0.10f64).to_bits());
    // the PLA numbers are the engine defaults
    let d = Config::default();
    assert_eq!(
        (PLA.pad_h, PLA.pad_grab, PLA.prop_gap),
        (d.fin.pad_h, d.pad.grab, d.prop.gap)
    );
}
