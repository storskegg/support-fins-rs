//! Port of upstream `tests/neck.test.js`: a wide wall's neck on a low wall (local issue 044).
//! nozzle.js grows the tip taper so a wide wall (Wall lines, a big nozzle) necks into its tip no
//! flatter than PROP.neckDeg, but a wall lower than that taper squeezed the neck into what was
//! left: its shoulder leant flatter -- an overhang on the wall's own side. Now the body narrows to
//! what the height can neck (sweep.js stemHalf); the 2-line wall's shoulder (under half a line)
//! never narrows.
//!
//! JS mutates the module config (`applyNozzle`, `applyWallLines`, `CUT.pattern`) and restores it in
//! a `finally`; here each test owns its `Config`.

use sf_engine::config::Config;
use sf_engine::jsnum;
use sf_engine::nozzle::{apply_nozzle, apply_wall_lines};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::squat::sweep_squat;
use sf_engine::prop::sweep::{profile_half, sweep};
use sf_engine::types::V4;

/// The neck's lean from level, in degrees, over its overhanging part (past half a line)
/// (neck.test.js:12-17).
fn neck_deg(top: f64, cfg: &Config) -> f64 {
    let p = &cfg.prop;
    let ztip = jsnum::max(top - p.tip_h, p.base_h + 0.1);
    let step = profile_half(ztip + 1e-6, top, cfg) - p.tip / 2.0 - p.tip / 3.0;
    if step <= 0.0 {
        90.0
    } else {
        jsnum::atan2(top - ztip, step) * 180.0 / std::f64::consts::PI
    }
}

fn ctx(cfg: &Config) -> BuildCtx<'_> {
    BuildCtx::new(cfg, Crest::new(None, 0.0, cfg))
}

// neck.test.js:20
#[test]
fn neck_a_low_wide_wall_leans_its_shoulder_no_flatter_than_neck_deg() {
    let mut cfg = Config::default();
    for (d, n) in [(0.4, 8.0), (0.4, 6.0), (1.2, 4.0), (2.0, 2.0)] {
        apply_nozzle(&mut cfg, d);
        apply_wall_lines(&mut cfg, n);
        for top in [1.5, 2.0, 2.5, 3.0, 4.0, 6.0] {
            let a = neck_deg(top, &cfg);
            assert!(
                a >= cfg.prop.neck_deg - 1e-6,
                "{d} x{n} at top {top}: the neck leans {a:.1}deg"
            );
        }
        // a tall wall keeps its full body
        assert!(
            (profile_half(cfg.prop.base_h + 0.5, 20.0, &cfg) - cfg.prop.th / 2.0).abs() < 1e-12,
            "{d} x{n}: a tall wall narrowed"
        );
    }
}

// neck.test.js:35
#[test]
fn neck_the_2_line_0_4_wall_never_narrows_even_on_a_low_tail() {
    let cfg = Config::default();
    for top in [0.5, 0.8, 1.2, 1.5, 3.0] {
        assert!(
            profile_half(cfg.prop.base_h + 0.05, top, &cfg) == cfg.prop.th / 2.0
                || top < cfg.prop.base_h + 0.1,
            "top {top}: the default wall narrowed"
        );
    }
}

// neck.test.js:42
#[test]
fn neck_the_swept_low_wide_wall_matches_the_profile_body_narrowed_in_the_mesh() {
    let mut cfg = Config::default();
    apply_nozzle(&mut cfg, 0.4);
    apply_wall_lines(&mut cfg, 8.0);
    let ctx = ctx(&cfg);
    let line = [[0.0, 0.0, 2.2], [10.0, 0.0, 2.2]];
    let mut out: Vec<V4> = Vec::new();
    // sweep(line, 0, out): minHeight defaults to PROP.minHeight
    assert!(
        sweep(&line, 0.0, &mut out, cfg.prop.min_height, &ctx),
        "no wall swept"
    );
    // the stem's widest |y| below the flange-free band (z > baseH) is the narrowed body
    let mut w = 0.0f64;
    for v in &out {
        if v[2] > cfg.prop.base_h + 1e-6 {
            w = jsnum::max(w, jsnum::abs(v[1]));
        }
    }
    assert!(
        w < cfg.prop.th / 2.0 - 0.1,
        "the low wall kept its full {} mm body (half {w})",
        cfg.prop.th
    );
}

// neck.test.js:55
#[test]
fn neck_a_low_wide_squat_wall_narrows_too_the_lowest_walls_there_are() {
    let mut cfg = Config::default();
    apply_nozzle(&mut cfg, 0.4);
    apply_wall_lines(&mut cfg, 8.0);
    let ctx = ctx(&cfg);
    let mut out: Vec<V4> = Vec::new();
    assert!(
        sweep_squat(
            &[
                [0.0, 0.0, 1.2 + cfg.prop.gap],
                [10.0, 0.0, 1.2 + cfg.prop.gap]
            ],
            0.0,
            &mut out,
            None,
            &ctx
        ),
        "no squat wall swept"
    );
    let mut w = 0.0f64;
    for v in &out {
        if v[2] > cfg.prop.squat_brim_h + 1e-6 {
            w = jsnum::max(w, jsnum::abs(v[1]));
        }
    }
    assert!(
        w < cfg.prop.th / 2.0 - 0.5,
        "the squat wall kept a {} mm body under a {} mm neck",
        2.0 * w,
        cfg.prop.tip_h
    );
}

// neck.test.js:68
#[test]
fn neck_a_cut_walls_bands_and_strips_keep_to_the_narrowed_body_cutouts() {
    let mut cfg = Config::default();
    apply_nozzle(&mut cfg, 0.4);
    apply_wall_lines(&mut cfg, 8.0);
    cfg.cut.pattern = "arch".to_string();
    // low (2.6 mm) for x 0-8, rising to 12 mm in the middle: the low stretch sits between the end
    // posts, inside the bands
    let mut line: Vec<[f64; 3]> = Vec::new();
    let mut x = 0.0f64;
    while x <= 40.0 {
        let t = jsnum::min(1.0, jsnum::max(0.0, (jsnum::min(x, 40.0 - x) - 8.0) / 6.0));
        line.push([x, 0.0, 2.6 + 9.4 * t + cfg.prop.gap]);
        x += 0.5;
    }
    let ctx = ctx(&cfg);
    let mut out: Vec<V4> = Vec::new();
    assert!(
        sweep(&line, 0.0, &mut out, cfg.prop.min_height, &ctx),
        "no wall swept"
    );
    let mut w = 0.0f64;
    for v in &out {
        if v[0] > 3.0 && v[0] < 5.0 && v[2] > cfg.prop.base_h + 0.01 {
            w = jsnum::max(w, jsnum::abs(v[1]));
        }
    }
    assert!(
        w < cfg.prop.th / 2.0 - 0.1,
        "the cut wall's low stretch is {} mm wide, the full body",
        2.0 * w
    );
}
