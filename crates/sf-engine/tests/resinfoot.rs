//! Own tests for `sf_engine::prop::resinfoot` (the JS `resinfoot.test.js` builds whole fin
//! sets: M3, docs/TEST-MAP.md). Bit-for-bit replay against JS is
//! `crates/sf-testkit/tests/parity_resinfoot.rs`.

use sf_engine::config::Config;
use sf_engine::prop::resinfoot::{Foot, windowed_flange};

fn feet(l: f64, n: usize) -> Vec<Foot> {
    (0..n)
        .map(|i| Foot {
            p: [l * i as f64 / (n - 1) as f64, 0.0],
            sx: 0.0,
            sy: 1.0,
            foot: 3.0,
            sh: 0.5,
            z_bed: 0.0,
            base_top: 0.6,
        })
        .collect()
}

fn run(l: f64, n: usize, cfg: &Config) -> (bool, Vec<[f64; 3]>) {
    let mut out = Vec::new();
    let ret = windowed_flange(&feet(l, n), &mut out, cfg);
    (ret, out)
}

#[test]
fn a_flange_shorter_than_pad_window_pad_stays_solid() {
    let c = Config::default();
    // 1.5 + 2 + 1.5 = 5: just under is solid, exactly at is windowed (`L >= ...`)
    let (ret, out) = run(4.999, 6, &c);
    assert!(!ret && out.is_empty());
    let (ret, out) = run(5.0, 6, &c);
    assert!(ret && !out.is_empty());
}

#[test]
fn nan_length_stays_solid() {
    let mut f = feet(10.0, 4);
    f[2].p[0] = f64::NAN;
    let mut out = Vec::new();
    assert!(!windowed_flange(&f, &mut out, &Config::default()));
    assert!(out.is_empty());
}

#[test]
fn a_single_station_stays_solid() {
    let mut out = Vec::new();
    assert!(!windowed_flange(
        &feet(0.0, 2)[..1],
        &mut out,
        &Config::default()
    ));
}

#[test]
fn pads_are_spread_evenly_one_at_each_end() {
    let c = Config::default();
    // n pads = floor((L - pad) / (pad + window)) + 1; each pad is a two-section ribbon
    // (4 side quads = 8 triangles + 2 caps * 2 triangles = 12 triangles = 36 vertices)
    let pad = 36;
    for (l, pads) in [(5.0, 2usize), (10.0, 3), (30.0, 9), (80.0, 23)] {
        let (ret, out) = run(l, 2 + l as usize, &c);
        assert!(ret);
        // strip under the stem: (stations - 1) segments * 8 triangles + 4 cap triangles
        let stations = 2 + l as usize;
        let strip_verts = 3 * (8 * (stations - 1) + 4);
        assert_eq!(out.len(), strip_verts + pads * pad, "L = {l}");
    }
}

#[test]
fn custom_resin_foot_numbers_change_the_windowing() {
    let mut c = Config::default();
    c.resin_foot.pad = 3.0;
    c.resin_foot.window = 4.0;
    // 3 + 4 + 3 = 10
    assert!(!run(9.9, 10, &c).0);
    assert!(run(10.0, 10, &c).0);
}

#[test]
fn the_pad_bevel_undercuts_to_no_narrower_than_the_strip() {
    let c = Config::default();
    let (_, out) = run(10.0, 12, &c);
    // all vertices stay within the flange footprint and above the plate
    for v in &out {
        assert!(v[1].abs() <= 3.0 + 1e-12);
        assert!(v[2] >= 0.0 && v[2] <= 0.6);
    }
    // the pads' bottom edge is |y| = max(min(foot, sh + strip), foot - bevel * 0.6) = 2.4
    assert!(
        out.iter()
            .any(|v| v[2] == 0.0 && (v[1].abs() - 2.4).abs() < 1e-12)
    );
}
