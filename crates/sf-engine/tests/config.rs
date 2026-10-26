//! Own tests for `sf_engine::config`: defaults and `apply_tunables`. The JS-side replay (every
//! config object at import, and ~140 tunables sequences) is `crates/sf-testkit/tests/parity_config.rs`.

use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_engine::cutout::CUTOUT_PATTERNS;
use sf_engine::fins::pad::PadStyle;
use sf_engine::jsnum;
use sf_engine::prop::config::{Iface, TineDir};

fn s(v: &str) -> Option<TunableValue> {
    Some(TunableValue::Str(v.to_string()))
}

fn applied(t: Tunables) -> Config {
    let mut c = Config::default();
    apply_tunables(&mut c, Some(&t));
    c
}

#[test]
fn computed_defaults_use_the_js_expressions() {
    let c = Config::default();
    assert_eq!(c.trace.bridge, c.prop.max_unsupported_span / 2.0);
    assert_eq!(c.brace.aspect, c.prop.max_short_aspect);
    assert_eq!(
        c.tie.slope.to_bits(),
        jsnum::tan((55.0 * std::f64::consts::PI) / 180.0).to_bits()
    );
    assert_eq!(c.sway.max_back_lean_deg, 45.0);
    assert_eq!(c.prop.iface, Iface::Off);
    assert_eq!(c.prop.tine_dir, TineDir::Horizontal);
    assert_eq!(c.pad.style, PadStyle::Auto);
    assert_eq!(c.cut.pattern, "none");
    assert_eq!(c.fill.shifts, vec![-2.0, 2.0, -4.0, 4.0]);
    assert_eq!(c.rows.full_faces, 250_000.0);
}

#[test]
fn null_switches_round_feet_off_and_changes_nothing_else() {
    let mut c = Config::default();
    c.prop.round_feet = true;
    c.fin.round_feet = true;
    c.perp.round_feet = true;
    c.prop.gap = 0.9;
    apply_tunables(&mut c, None);
    assert!(!c.prop.round_feet && !c.fin.round_feet && !c.perp.round_feet);
    assert_eq!(c.prop.gap, 0.9);
}

#[test]
fn round_feet_needs_exactly_true() {
    let on = applied(Tunables {
        round_feet: Some(TunableValue::Bool(true)),
        ..Tunables::default()
    });
    assert!(on.prop.round_feet && on.fin.round_feet && on.perp.round_feet);
    for v in [
        s("true"),
        Some(TunableValue::Num(1.0)),
        Some(TunableValue::Null),
    ] {
        let c = applied(Tunables {
            round_feet: v,
            ..Tunables::default()
        });
        assert!(!c.prop.round_feet);
    }
    // an empty object turns it back off
    let mut c = on;
    apply_tunables(&mut c, Some(&Tunables::default()));
    assert!(!c.prop.round_feet);
}

#[test]
fn numeric_tunables_ignore_non_finite_values() {
    let c = applied(Tunables {
        pad_h: Some(f64::NAN),
        pad_grab: Some(f64::INFINITY),
        prop_gap: Some(f64::NEG_INFINITY),
        ..Tunables::default()
    });
    assert_eq!(c.fin.pad_h, 0.5);
    assert_eq!(c.pad.grab, 0.05);
    assert_eq!(c.prop.gap, 0.2);
    assert_eq!(c.perp.gap, 0.2);
}

#[test]
fn prop_gap_reaches_the_wedge_too() {
    let c = applied(Tunables {
        prop_gap: Some(0.3),
        ..Tunables::default()
    });
    assert_eq!((c.prop.gap, c.perp.gap), (0.3, 0.3));
}

#[test]
fn pad_style_and_custom() {
    for (name, want) in [
        ("auto", PadStyle::Auto),
        ("light", PadStyle::Light),
        ("sure", PadStyle::Sure),
        ("custom", PadStyle::Custom),
    ] {
        let c = applied(Tunables {
            pad_style: s(name),
            ..Tunables::default()
        });
        assert_eq!(c.pad.style, want);
    }
    let c = applied(Tunables {
        pad_style: s("bogus"),
        ..Tunables::default()
    });
    assert_eq!(c.pad.style, PadStyle::Auto);
    let c = applied(Tunables {
        pad_custom: Some(vec![
            ("h".into(), 1.5),
            ("grip".into(), f64::NAN),
            ("bogus".into(), 9.0),
            ("margin".into(), 6.0),
        ]),
        ..Tunables::default()
    });
    assert_eq!(
        (
            c.pad.custom.h,
            c.pad.custom.gap,
            c.pad.custom.grip,
            c.pad.custom.margin
        ),
        (1.5, 0.0, 0.05, 6.0)
    );
}

#[test]
fn cutout_accepts_only_known_patterns() {
    for p in CUTOUT_PATTERNS {
        let c = applied(Tunables {
            cutout: s(p),
            ..Tunables::default()
        });
        assert_eq!(c.cut.pattern, p);
    }
    let c = applied(Tunables {
        cutout: s("bogus"),
        ..Tunables::default()
    });
    assert_eq!(c.cut.pattern, "none");
}

#[test]
fn iface_accepts_false_flat_all() {
    let mut c = Config::default();
    for (v, want) in [
        (s("flat"), Iface::Flat),
        (s("all"), Iface::All),
        (Some(TunableValue::Bool(false)), Iface::Off),
    ] {
        apply_tunables(
            &mut c,
            Some(&Tunables {
                iface: v,
                ..Tunables::default()
            }),
        );
        assert_eq!(c.prop.iface, want);
    }
    apply_tunables(
        &mut c,
        Some(&Tunables {
            iface: s("flat"),
            ..Tunables::default()
        }),
    );
    for junk in [
        Some(TunableValue::Bool(true)),
        s("sideways"),
        Some(TunableValue::Null),
    ] {
        apply_tunables(
            &mut c,
            Some(&Tunables {
                iface: junk,
                ..Tunables::default()
            }),
        );
        assert_eq!(c.prop.iface, Iface::Flat);
    }
}

#[test]
fn tine_dir_vertical_zeroes_the_foot_gap_and_horizontal_restores_it() {
    let mut c = Config::default();
    apply_tunables(
        &mut c,
        Some(&Tunables {
            tine_dir: s("vertical"),
            ..Tunables::default()
        }),
    );
    assert_eq!((c.prop.tine_dir, c.prop.foot_gap), (TineDir::Vertical, 0.0));
    apply_tunables(
        &mut c,
        Some(&Tunables {
            tine_dir: s("horizontal"),
            ..Tunables::default()
        }),
    );
    assert_eq!(
        (c.prop.tine_dir, c.prop.foot_gap),
        (TineDir::Horizontal, 0.2)
    );
    apply_tunables(
        &mut c,
        Some(&Tunables {
            tine_dir: s("diagonal"),
            ..Tunables::default()
        }),
    );
    assert_eq!(c.prop.tine_dir, TineDir::Horizontal);
}

#[test]
fn clamps() {
    let c = applied(Tunables {
        tine_contact_offset: Some(5.0),
        pin_depth: Some(0.0),
        tall_ribs_height: Some(5000.0),
        ..Tunables::default()
    });
    assert_eq!(c.prop.tine_contact_offset, 0.2);
    assert_eq!(c.prop.pin_depth, 0.05);
    assert_eq!(c.brace.tall_h, 2000.0);
    let c = applied(Tunables {
        tine_contact_offset: Some(-5.0),
        pin_depth: Some(50.0),
        tall_ribs_height: Some(0.5),
        ..Tunables::default()
    });
    assert_eq!(c.prop.tine_contact_offset, -0.2);
    assert_eq!(c.prop.pin_depth, c.prop.pin_depth_max);
    assert_eq!(c.brace.tall_h, 1.0);
    // tallRibsHeight must be > 0
    let c = applied(Tunables {
        tall_ribs_height: Some(0.0),
        ..Tunables::default()
    });
    assert_eq!(c.brace.tall_h, 100.0);
}

#[test]
fn tines_per_wall_must_be_a_non_negative_integer() {
    for (v, want) in [
        (3.0, 3.0),
        (0.0, 0.0),
        (-1.0, 0.0),
        (1.5, 0.0),
        (f64::NAN, 0.0),
    ] {
        let c = applied(Tunables {
            tines_per_wall: Some(v),
            ..Tunables::default()
        });
        assert_eq!(c.prop.tines_per_wall, want, "{v}");
    }
}

#[test]
fn nozzle_and_wall_lines_go_through_apply_tunables() {
    let c = applied(Tunables {
        nozzle: Some(0.8),
        wall_lines: Some(4.0),
        ..Tunables::default()
    });
    assert_eq!((c.nozzle.nozzle, c.nozzle.wall_lines), (0.8, 4.0));
    assert!(c.prop.th > 1.0);
    // NaN nozzle is passed to applyNozzle and ignored there
    let c = applied(Tunables {
        nozzle: Some(f64::NAN),
        ..Tunables::default()
    });
    assert_eq!(c.nozzle.nozzle, 0.4);
}
