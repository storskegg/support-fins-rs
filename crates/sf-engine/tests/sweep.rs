//! Own tests for `sf_engine::prop::sweep`. The JS tests that reach it build whole fin sets (M3,
//! docs/TEST-MAP.md); bit-for-bit replay against JS is `crates/sf-testkit/tests/parity_sweep.rs`.

use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::prop::config::{Iface, TineDir};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::sweep::{foot_for, profile_half, stem_half, sweep, sweep_between};
use sf_engine::tris::Tris;
use sf_engine::types::V4;

fn line(len: f64, z: f64) -> Vec<[f64; 3]> {
    (0..=len as usize)
        .map(|i| [-len / 2.0 + i as f64, 0.0, z])
        .collect()
}

fn volume(tris: &[V4]) -> f64 {
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

fn ctx(cfg: &Config) -> BuildCtx<'_> {
    BuildCtx::new(cfg, Crest::new(None, 0.2, cfg))
}

#[test]
fn foot_for_scales_then_clamps() {
    let c = Config::default();
    assert_eq!(foot_for(0.0, &c), c.prop.foot_min);
    assert_eq!(foot_for(-5.0, &c), c.prop.foot_min);
    assert_eq!(foot_for(1000.0, &c), c.prop.foot_max);
    let mid = foot_for(20.0, &c);
    assert_eq!(mid, 20.0 * c.prop.foot_ratio);
    // the span cap: half the row spacing less 1 mm
    let mut c2 = Config::default();
    c2.prop.max_unsupported_span = 6.0;
    assert_eq!(foot_for(1000.0, &c2), 2.5);
    // a cap below footMin is lifted to footMin
    c2.prop.max_unsupported_span = 2.0;
    assert_eq!(foot_for(1000.0, &c2), c.prop.foot_min);
    assert!(foot_for(f64::NAN, &c).is_nan());
}

#[test]
fn stem_half_is_full_unless_the_neck_is_too_low() {
    let c = Config::default();
    assert_eq!(stem_half(5.0, &c), c.prop.th / 2.0);
    // a wide wall on a short taper narrows, never below the tip
    let mut w = Config::default();
    w.prop.th = 4.0;
    let low = stem_half(0.1, &w);
    assert!(low < 2.0 && low >= w.prop.tip / 2.0);
    assert_eq!(stem_half(-3.0, &w), stem_half(0.0, &w)); // negative rise counts as 0
    // at rise 0 only the half-line of slack is left above the tip's half-width
    assert_eq!(
        stem_half(0.0, &w),
        w.prop.tip / 2.0 + (w.prop.tip / 3.0 + 1e-9)
    );
}

#[test]
fn profile_half_has_flange_stem_and_tip() {
    let c = Config::default();
    let top = 12.0;
    assert_eq!(profile_half(0.0, top, &c), foot_for(top, &c));
    assert_eq!(
        profile_half(c.prop.base_h - 1e-9, top, &c),
        foot_for(top, &c)
    );
    assert_eq!(profile_half(3.0, top, &c), c.prop.th / 2.0);
    assert_eq!(profile_half(top, top, &c), c.prop.tip / 2.0);
    // monotone down the neck
    let ztip = top - c.prop.tip_h;
    assert!(profile_half(ztip + 0.5, top, &c) < profile_half(ztip, top, &c));
}

#[test]
fn sweep_builds_a_closed_outward_t_and_declines_bad_lines() {
    let cfg = Config::default();
    let cx = ctx(&cfg);
    let mut out = Vec::new();
    assert!(sweep(
        &line(10.0, 12.0),
        0.0,
        &mut out,
        cfg.prop.min_height,
        &cx
    ));
    assert!(!out.is_empty() && out.len() % 3 == 0);
    assert!(volume(&out) > 0.0);
    assert!(out.iter().all(|v| v[3] == 0.0), "crest is off");
    // z never above the contact less the gap, never below the bed
    let top = 12.0 - cfg.prop.gap;
    assert!(out.iter().all(|v| v[2] >= 0.0 && v[2] <= top + 1e-12));

    // a repeated station: no direction
    let mut o2 = Vec::new();
    assert!(!sweep(
        &[[0.0, 0.0, 12.0], [0.0, 0.0, 12.0], [3.0, 0.0, 12.0]],
        0.0,
        &mut o2,
        1.5,
        &cx
    ));
    // too little headroom (z - gap - bed < minH)
    assert!(!sweep(
        &line(6.0, 1.5),
        0.0,
        &mut o2,
        cfg.prop.min_height,
        &cx
    ));
    // out is left as it was by a refusal that happens before any push
    assert!(o2.is_empty());
    // a lone station has a zero direction too
    assert!(!sweep(&[[0.0, 0.0, 12.0]], 0.0, &mut o2, 1.5, &cx));
}

#[test]
fn pins_and_round_feet_change_the_flange_only() {
    let plain = Config::default();
    let mut a = Vec::new();
    sweep(&line(12.0, 15.0), 0.0, &mut a, 1.5, &ctx(&plain));
    let mut vert = Config::default();
    vert.prop.tine_dir = TineDir::Vertical;
    let mut b = Vec::new();
    sweep(&line(12.0, 15.0), 0.0, &mut b, 1.5, &ctx(&vert));
    assert_ne!(a.len(), b.len()); // windowed flange
    let mut round = Config::default();
    round.prop.round_feet = true;
    let mut r = Vec::new();
    sweep(&line(12.0, 15.0), 0.0, &mut r, 1.5, &ctx(&round));
    assert_ne!(a.len(), r.len());
    // rounded feet are off under vertical pins
    round.prop.tine_dir = TineDir::Vertical;
    let mut rv = Vec::new();
    sweep(&line(12.0, 15.0), 0.0, &mut rv, 1.5, &ctx(&round));
    assert_eq!(rv, b);
}

#[test]
fn the_crest_tags_its_band_only() {
    let mut cfg = Config::default();
    cfg.prop.iface = Iface::Flat;
    // a flat underside 12 above the bed, directly over the line
    let slab = {
        let (x0, x1, y0, y1, z0, z1) = (-30.0, 30.0, -20.0, 20.0, 12.0, 16.0);
        let v = [
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
        ];
        let mut t = Vec::new();
        for [a, b, c] in [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ] {
            t.extend(v[a]);
            t.extend(v[b]);
            t.extend(v[c]);
        }
        Arc::new(Tris::new(t))
    };
    let cx = BuildCtx::new(&cfg, Crest::new(Some(slab), 0.2, &cfg));
    let mut out = Vec::new();
    assert!(sweep(&line(10.0, 12.0), 0.0, &mut out, 1.5, &cx));
    let tagged = out.iter().filter(|v| v[3] == 1.0).count();
    assert!(tagged > 0 && tagged < out.len());
    // the tag is per triangle: first vertex tagged iff all three are
    for t in out.chunks(3) {
        assert!(t.iter().all(|v| v[3] == t[0][3]));
    }
    // without the part the 'flat' test finds nothing to crest
    let none = ctx(&cfg);
    let mut o = Vec::new();
    sweep(&line(10.0, 12.0), 0.0, &mut o, 1.5, &none);
    assert!(o.iter().all(|v| v[3] == 0.0));
}

fn floor(top: &[[f64; 3]], f: impl Fn(f64) -> Vec<f64>) -> Vec<Vec<f64>> {
    top.iter()
        .map(|p| {
            let mut v = vec![p[0], p[1]];
            v.extend(f(p[0]));
            v
        })
        .collect()
}

#[test]
fn sweep_between_welded_lifted_and_declined() {
    let cfg = Config::default();
    let cx = ctx(&cfg);
    let top = line(10.0, 20.0);
    let level = floor(&top, |_| vec![2.0]);
    let mut lifted = Vec::new();
    assert!(sweep_between(&top, &level, &mut lifted, 1.5, &cx));
    assert!(volume(&lifted) > 0.0);
    // bottom stops footGap above the floor
    let lo = lifted.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
    assert!((lo - (2.0 + cfg.prop.foot_gap)).abs() < 1e-12, "{lo}");
    // over the plate (floor 0) the wall stands on it
    let plate = floor(&top, |_| vec![0.0]);
    let mut on = Vec::new();
    assert!(sweep_between(&top, &plate, &mut on, 1.5, &cx));
    assert_eq!(on.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min), 0.0);
    // welded: footGap 0
    let mut w = Config::default();
    w.prop.foot_gap = 0.0;
    let mut welded = Vec::new();
    assert!(sweep_between(&top, &level, &mut welded, 1.5, &ctx(&w)));
    assert_ne!(welded.len(), lifted.len()); // 8-vertex sections
    assert_eq!(
        welded.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min),
        2.0
    );
    // side floors tilt the lifted bottom
    let sides = floor(&top, |_| vec![2.0, 1.5, 2.5]);
    let mut tilted = Vec::new();
    assert!(sweep_between(&top, &sides, &mut tilted, 1.5, &cx));
    assert_ne!(tilted, lifted);
    // not enough headroom: top - floor < minH
    let high = floor(&top, |_| vec![19.5]);
    let mut none = Vec::new();
    assert!(!sweep_between(&top, &high, &mut none, 1.5, &cx));
    assert!(none.is_empty());
}
