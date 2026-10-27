//! Own tests for `sf_engine::prop::squat`. The JS tests that reach it build whole fin sets (M3,
//! docs/TEST-MAP.md); bit-for-bit replay against JS is `crates/sf-testkit/tests/parity_squat.rs`.

use sf_engine::config::Config;
use sf_engine::overhangs::build_topology;
use sf_engine::prop::config::TineDir;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::squat::{build_squat_bed, sweep_squat};
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, V4, Xyz};

const ORIGIN: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<f64> {
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
    t
}

fn line(len: usize, z: f64) -> Vec<[f64; 3]> {
    (0..=len)
        .map(|i| [i as f64 - len as f64 / 2.0, 0.0, z])
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
fn sweep_squat_builds_a_wall_on_a_brim_and_refuses_what_is_too_low() {
    let cfg = Config::default();
    let cx = ctx(&cfg);
    let z = cfg.prop.gap + 1.0; // 1.0 mm of wall: between minHeightSquat and minHeight
    let mut out = Vec::new();
    assert!(sweep_squat(&line(8, z), 0.0, &mut out, None, &cx));
    assert!(volume(&out) > 0.0 && out.len() % 3 == 0);
    // the brim is as wide as squatBrimW and as tall as squatBrimH
    let wide = out.iter().map(|v| v[1].abs()).fold(0.0, f64::max);
    assert!((wide - cfg.prop.squat_brim_w).abs() < 1e-12, "{wide}");
    // below minHeightSquat there is no wall
    let mut o2 = Vec::new();
    assert!(!sweep_squat(
        &line(8, cfg.prop.gap + 0.3),
        0.0,
        &mut o2,
        None,
        &cx
    ));
    // a repeated station has no direction
    assert!(!sweep_squat(
        &[[0.0, 0.0, z], [0.0, 0.0, z], [3.0, 0.0, z]],
        0.0,
        &mut o2,
        None,
        &cx
    ));
    assert!(o2.is_empty());
}

#[test]
fn inset_narrows_the_stem_tip_and_brim_and_missing_entries_count_as_zero() {
    let cfg = Config::default();
    let cx = ctx(&cfg);
    let z = cfg.prop.gap + 1.0;
    let (mut plain, mut inset, mut short) = (Vec::new(), Vec::new(), Vec::new());
    sweep_squat(&line(8, z), 0.0, &mut plain, None, &cx);
    sweep_squat(&line(8, z), 0.0, &mut inset, Some(&[0.2; 9]), &cx);
    let w = |o: &[V4]| o.iter().map(|v| v[1].abs()).fold(0.0, f64::max);
    assert!((w(&inset) - (cfg.prop.squat_brim_w - 0.2)).abs() < 1e-12);
    // an inset shorter than the line leaves the later stations at 0
    sweep_squat(&line(8, z), 0.0, &mut short, Some(&[0.2]), &cx);
    assert!((w(&short) - cfg.prop.squat_brim_w).abs() < 1e-12);
    // zeros equal no inset
    let mut zeros = Vec::new();
    sweep_squat(&line(8, z), 0.0, &mut zeros, Some(&[0.0; 9]), &cx);
    assert_eq!(zeros, plain);
}

#[test]
fn resin_pins_window_the_brim_unless_it_is_inset() {
    let mut cfg = Config::default();
    cfg.prop.tine_dir = TineDir::Vertical;
    let cx = ctx(&cfg);
    let z = cfg.prop.gap + 1.0;
    let l = line(24, z);
    let (mut windowed, mut inset) = (Vec::new(), Vec::new());
    sweep_squat(&l, 0.0, &mut windowed, None, &cx);
    sweep_squat(&l, 0.0, &mut inset, Some(&[0.1; 25]), &cx);
    assert_ne!(windowed.len(), inset.len());
    // an all-zero inset (Draw's all-squat line) is not an inset
    let mut zero = Vec::new();
    sweep_squat(&l, 0.0, &mut zero, Some(&[0.0; 25]), &cx);
    assert_eq!(zero, windowed);
}

/// A slab hanging at z = 1.0..5 over the plate: its underside is a 0.8 mm wall (squat band).
fn slab() -> (sf_engine::overhangs::Topology, Tris) {
    let pos = block(-20.0, 20.0, -5.0, 5.0, 1.0, 5.0);
    (build_topology(pos.clone()), Tris::new(pos))
}

#[test]
fn build_squat_bed_places_one_wall_under_a_low_overhang() {
    let cfg = Config::default();
    let cx = ctx(&cfg);
    let (topo, tris) = slab();
    let l = line(20, 1.0);
    let mut out = Vec::new();
    let placed = build_squat_bed(&l, &tris, &topo, &IDENTITY3, &ORIGIN, &mut out, None, &cx);
    assert_eq!(placed.len(), 1);
    let w = &placed[0];
    assert!(w.squat && w.stations == 21 && w.tri_range == [0, out.len()]);
    assert!((w.height - (1.0 - cfg.prop.gap)).abs() < 1e-12);
    assert!((w.span - 20.0).abs() < 1e-9);
    assert!((w.volume - volume(&out).abs()).abs() < 1e-12);
    assert!(
        w.line
            .iter()
            .all(|p| (p[2] - (1.0 - cfg.prop.gap)).abs() < 1e-12)
    );
    // a second call appends after the first wall
    let again = build_squat_bed(&l, &tris, &topo, &IDENTITY3, &ORIGIN, &mut out, None, &cx);
    assert_eq!(again[0].tri_range[0], w.tri_range[1]);
}

#[test]
fn build_squat_bed_skips_claimed_tall_and_short_runs() {
    let cfg = Config::default();
    let cx = ctx(&cfg);
    let (topo, tris) = slab();
    let run = |l: &[[f64; 3]], claimed: Option<&[bool]>| {
        let mut out = Vec::new();
        build_squat_bed(l, &tris, &topo, &IDENTITY3, &ORIGIN, &mut out, claimed, &cx).len()
    };
    let l = line(20, 1.0);
    // every station claimed by a tall wall's tail
    assert_eq!(run(&l, Some(&[true; 21])), 0);
    // the claimed half leaves a run of 10 stations, still a wall
    let half: Vec<bool> = (0..21).map(|i| i < 10).collect();
    assert_eq!(run(&l, Some(&half)), 1);
    // a tall line (>= minHeight) is the full wall's, not a squat's
    assert_eq!(run(&line(20, 4.0), None), 0);
    // below minHeightSquat
    assert_eq!(run(&line(20, 0.5), None), 0);
    // under minStations / minSpanSquat
    assert_eq!(run(&line(1, 1.0), None), 0);
    assert_eq!(
        run(
            &line(2, 1.0)[..2]
                .iter()
                .map(|p| [p[0] * 0.5, p[1], p[2]])
                .collect::<Vec<_>>(),
            None
        ),
        0
    );
}
