//! Port of upstream `tests/tunables.test.js`: the clearance settings reaching the BUILD, wherever it
//! runs. JS sets module globals (`fins.FIN`, `fins.PAD`, `prop.PROP`) and restores them in
//! `finally`; the Rust engine has no globals, so a `Config` plays them and nothing needs restoring.
mod builds;
use builds::*;
use sf_engine::config::{Config, Tunables, apply_tunables};
use sf_engine::fins::build::FinsOpts;
use sf_engine::inside::Nearest;
use sf_engine::overhangs::analyze;
use sf_engine::prop::clearance::welds;
use sf_testkit::{bbox, tilted_block_topo};
use std::sync::Arc;

fn opts(bed_pad: bool, tunables: Tunables) -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(bed_pad),
        tines: Some(true),
        tunables: Some(tunables),
        ..Default::default()
    }
}

/// Highest point of the support built for a tilted block at this gap.
fn top_at(tunables: Tunables) -> f64 {
    let topo = Arc::new(tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 30.0, 45.0));
    let res = analyze(&topo, 45.0, &I);
    let built = sf_engine::fins::build::build_fins(
        &topo,
        &res,
        &I,
        &opts(true, tunables),
        &Config::default(),
    );
    assert!(!built.triangles.is_empty(), "no support built to measure");
    bbox(&xyz(&built.triangles)).hi[2]
}

// tunables.test.js:19
#[test]
fn tunables_a_bigger_support_gap_stops_the_support_lower_under_the_part() {
    let tight = top_at(Tunables {
        prop_gap: Some(0.2),
        ..Default::default()
    });
    let loose = top_at(Tunables {
        prop_gap: Some(0.45),
        ..Default::default()
    });
    // Same part, same pose: the only thing that moved is the clearance, so the wall top must drop
    // by about the extra gap.
    assert!(
        loose < tight,
        "gap 0.45 built no lower than gap 0.2 ({loose} vs {tight})"
    );
    assert!(
        ((tight - loose) - 0.25).abs() < 0.1,
        "top moved {:.3}mm for a 0.25mm gap change",
        tight - loose
    );
}

// :31
#[test]
fn tunables_the_petg_profile_reaches_the_build_as_petg_numbers() {
    let mut cfg = Config::default();
    apply_tunables(
        &mut cfg,
        Some(&Tunables {
            pad_h: Some(0.3),
            pad_grab: Some(-0.1),
            prop_gap: Some(0.3),
            ..Default::default()
        }),
    );
    assert!(cfg.fin.pad_h == 0.3, "pad thickness not applied");
    assert!(cfg.pad.grab == -0.1, "pad grip not applied");
    assert!(cfg.prop.gap == 0.3, "prop gap not applied");
}

// :45
#[test]
fn tunables_absent_or_junk_values_leave_the_defaults_alone() {
    let snap = |c: &Config| [c.fin.pad_h, c.pad.grab, c.prop.gap];
    let before = snap(&Config::default());
    // `undefined`, `{}`, and `{ padH: NaN, propGap: 'wide', padGrab: null }` (a string and a null
    // are not numbers, so they arrive as absent)
    for t in [
        None,
        Some(Tunables::default()),
        Some(Tunables {
            pad_h: Some(f64::NAN),
            ..Default::default()
        }),
    ] {
        let mut cfg = Config::default();
        apply_tunables(&mut cfg, t.as_ref());
        assert!(
            snap(&cfg) == before,
            "defaults changed: {:?} vs {:?}",
            snap(&cfg),
            before
        );
    }
}

// :54
#[test]
fn tunables_a_bigger_gap_keeps_the_walls_under_a_40_deg_underside_021() {
    // The weld gate measures the NEAREST distance, ~gap x cos(slope) on a slope.
    let topo = Arc::new(tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 30.0, 40.0));
    let res = analyze(&topo, 45.0, &I);
    let build = |gap: f64| {
        let o = opts(
            true,
            Tunables {
                prop_gap: Some(gap),
                ..Default::default()
            },
        );
        let b = sf_engine::fins::build::build_fins(&topo, &res, &I, &o, &Config::default());
        (b.props.len(), b.unserved)
    };
    let (pla, petg) = (build(0.2), build(0.3));
    assert!(pla.0 > 0, "PLA built no walls to compare against");
    // walls + unserved catch it: main lost them in stationCertified's trim, not skipped.weld
    assert!(
        petg.0 == pla.0 && petg.1 == 0.0,
        "gap 0.3: {petg:?} where gap 0.2 built {pla:?}"
    );
}

// :73
#[test]
fn tunables_the_weld_floor_scales_with_the_gap_pla_unchanged() {
    for gap in [0.2, 0.3, 0.45] {
        let mut cfg = Config::default();
        apply_tunables(
            &mut cfg,
            Some(&Tunables {
                prop_gap: Some(gap),
                ..Default::default()
            }),
        );
        let at40 = Nearest {
            d: gap * (40.0 * std::f64::consts::PI / 180.0).cos(),
            cos_up: 0.77,
        };
        assert!(
            !welds(&at40, &cfg),
            "gap {gap}: an on-spec 40 deg approach counted as a weld"
        );
        assert!(
            welds(
                &Nearest {
                    d: gap * 0.5,
                    cos_up: 0.9
                },
                &cfg
            ),
            "gap {gap}: half the gap passed"
        );
        assert!(
            welds(
                &Nearest {
                    d: 0.2,
                    cos_up: 0.0
                },
                &cfg
            ),
            "gap {gap}: a 0.2 mm flank passed"
        );
    }
    assert!(
        (0.2 * 0.675 - (0.2 - 0.065f64)).abs() < 1e-12,
        "PLA floor moved off 0.135"
    );
}

// :89
#[test]
fn tunables_tines_per_wall_puts_exactly_n_tines_on_every_tined_wall() {
    // The tine coupon's how-few-still-hold row. 0 (the default) leaves spacing alone.
    let topo = Arc::new(tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 30.0, 40.0));
    let res = analyze(&topo, 45.0, &I);
    let per_wall = |t: Tunables| -> Vec<usize> {
        let b = sf_engine::fins::build::build_fins(
            &topo,
            &res,
            &I,
            &opts(false, t),
            &Config::default(),
        );
        b.props
            .iter()
            .filter(|f| f.tines > 0)
            .map(|f| f.tines)
            .collect()
    };
    let base = per_wall(Tunables::default());
    let one = per_wall(Tunables {
        tines_per_wall: Some(1.0),
        ..Default::default()
    });
    let two = per_wall(Tunables {
        tines_per_wall: Some(2.0),
        ..Default::default()
    });
    assert!(
        !base.is_empty() && base.iter().any(|&n| n > 2),
        "nothing to thin: {base:?}"
    );
    assert!(
        !one.is_empty() && one.iter().all(|&n| n == 1),
        "tinesPerWall 1 gave {one:?}"
    );
    assert!(
        !two.is_empty() && two.iter().all(|&n| n == 2),
        "tinesPerWall 2 gave {two:?}"
    );
    assert!(
        Config::default().prop.tines_per_wall == 0.0,
        "default not restored"
    );
}
