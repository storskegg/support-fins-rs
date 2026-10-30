//! Port of upstream `tests/tine_density.test.js`: the "Tine grip" slider, and the regression it exists
//! to make impossible (commit c0fcf8e scaled tine spacing by tip-over risk, so a stable squat part
//! fell to a sparse comb). The knob moves spacing the way the UI promises, the per-wall grip FLOOR
//! holds at the sparsest setting, and a STABLE part still gets a dense comb by default.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins_capturing};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::{BuildCtx, TineSeed};
use sf_engine::prop::surface::surface_z_at;
use sf_engine::prop::tines::{TineOpts, emit_tines, tine_step_for};
use sf_engine::tris::Tris;
use sf_engine::types::{V4, Xyz};
use sf_testkit::{assert_close, block_topo, tilted_block_topo};
use std::sync::Arc;

const ORIGIN: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

// --- 1. the knob itself ---------------------------------------------------------------------------

// tine_density.test.js:23
#[test]
fn tine_step_for_dense_by_default_sparse_loosens_monotonic() {
    let c = Config::default();
    let p = &c.prop;
    assert_close(
        tine_step_for(Some(1.0), &c),
        p.tine_step,
        1e-9,
        "density 1 should be the dense comb",
    );
    assert_close(
        tine_step_for(Some(0.0), &c),
        p.tine_step_sparse,
        1e-9,
        "density 0 should be the sparse comb",
    );
    assert_close(
        tine_step_for(None, &c),
        p.tine_step,
        1e-9,
        "DEFAULT must be dense, not sparse",
    );
    let mid = tine_step_for(Some(0.5), &c);
    assert!(
        mid > p.tine_step && mid < p.tine_step_sparse,
        "mid should sit between"
    );
    assert!(
        tine_step_for(Some(0.0), &c) > mid && mid > tine_step_for(Some(1.0), &c),
        "spacing must fall as density rises"
    );
    // out-of-range is clamped, not extrapolated (a rogue input can't over-densify).
    assert_close(
        tine_step_for(Some(9.0), &c),
        p.tine_step,
        1e-9,
        "clamp above 1",
    );
    assert_close(
        tine_step_for(Some(-9.0), &c),
        p.tine_step_sparse,
        1e-9,
        "clamp below 0",
    );
}

/// A `BuildCtx` whose tine capture is on (`globalThis.__TINECAP = []`).
fn capturing_ctx(cfg: &Config) -> BuildCtx<'_> {
    let ctx = BuildCtx::new(cfg, Crest::new(None, 0.0, cfg));
    *ctx.tine_cap.borrow_mut() = Some(vec![]);
    ctx
}

fn emit(line: &[[f64; 3]], topo: &Topology, step: f64, ctx: &BuildCtx) -> usize {
    let mut out: Vec<V4> = vec![];
    emit_tines(
        line,
        None,
        topo,
        &I,
        &ORIGIN,
        &mut out,
        TineOpts {
            step: Some(step),
            ..Default::default()
        },
        ctx,
    )
}

// A long straight contact line on a solid block: interior at y>=0, line along +Y.
fn tines_along_line(step: f64) -> usize {
    let topo = block_topo(-20.0, 20.0, 0.0, 40.0, 0.0, 40.0);
    let mut line = vec![];
    let mut y = -18.0;
    while y <= 18.0 {
        line.push([0.0, y, 20.0]); // ~36mm run
        y += 0.5;
    }
    let cfg = Config::default();
    emit(
        &line,
        &topo,
        step,
        &BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg)),
    )
}

// :47
#[test]
fn emit_tines_a_sparser_step_yields_fewer_tines_on_the_same_long_wall() {
    let c = Config::default();
    let dense = tines_along_line(tine_step_for(Some(1.0), &c)); // 2mm
    let sparse = tines_along_line(tine_step_for(Some(0.0), &c)); // 5mm
    assert!(
        sparse < dense,
        "slider is inert on a long wall: dense {dense} vs sparse {sparse}"
    );
    assert!(
        sparse >= 4,
        "sparse still has to be a comb, not a couple of nubs: {sparse}"
    );
}

// :53 -- A SHORT but fully-grippable wall at the sparsest spacing must still get minGripTines.
#[test]
fn emit_tines_the_per_wall_grip_floor_survives_the_sparsest_setting() {
    let topo = tilted_block_topo(-15.0, 15.0, -20.0, 20.0, 0.0, 30.0, 40.0); // ceiling rises with +Y
    let tris = Tris::new(topo.pos.clone());
    let mut line = vec![];
    let mut x = -2.0;
    while x <= 2.0 {
        // ~4mm run, < one sparse step
        if let Some(z) = surface_z_at(&tris, x, 0.0) {
            line.push([x, 0.0, z]);
        }
        x += 0.5;
    }
    let cfg = Config::default();
    let n = emit(
        &line,
        &topo,
        999.0,
        &BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg)),
    );
    assert!(
        n as f64 >= cfg.prop.min_grip_tines,
        "floor breached: short wall at max sparsity got {n} tines (< {})",
        cfg.prop.min_grip_tines
    );
}

// --- 2. through the pipeline ----------------------------------------------------------------------

fn cap_tines(topo: &Arc<Topology>, res: &Analysis, density: Option<f64>) -> Vec<TineSeed> {
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        tine_density: density,
        ..Default::default()
    };
    build_fins_capturing(topo, res, &I, &o, &Config::default()).1
}

/// nearest-neighbour spacing percentile across a tine cloud (the dense end anchors are the low
/// percentile; the comb is EDGE-BIASED).
fn pct_nn(caps: &[TineSeed], f: f64) -> f64 {
    let mut nn: Vec<f64> = caps
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut best = f64::INFINITY;
            for (j, b) in caps.iter().enumerate() {
                if i != j {
                    best = best.min(sf_engine::jsnum::hypot(&[a.x - b.x, a.y - b.y, a.z - b.z]));
                }
            }
            best
        })
        .collect();
    nn.sort_by(|p, q| p.partial_cmp(q).unwrap());
    nn[(nn.len() - 1).min((f * nn.len() as f64).floor() as usize)]
}

// :98
#[test]
fn build_fins_the_slider_moves_tine_count_monotonically_dense_over_sparse() {
    let topo = Arc::new(tilted_block_topo(-40.0, 40.0, -45.0, 45.0, -6.0, 6.0, 55.0)); // broad tilted plate
    let res = analyze(&topo, 60.0, &I);
    let dense = cap_tines(&topo, &res, Some(1.0)).len();
    let mid = cap_tines(&topo, &res, Some(0.5)).len();
    let sparse = cap_tines(&topo, &res, Some(0.0)).len();
    assert!(
        dense > sparse,
        "slider inert through the pipeline: dense {dense} vs sparse {sparse}"
    );
    assert!(
        dense >= mid && mid >= sparse,
        "not monotonic: dense {dense} -> mid {mid} -> sparse {sparse}"
    );
}

// :107
#[test]
fn build_fins_the_default_no_tine_density_is_the_dense_comb_not_sparse() {
    let topo = Arc::new(tilted_block_topo(-40.0, 40.0, -45.0, 45.0, -6.0, 6.0, 55.0));
    let res = analyze(&topo, 60.0, &I);
    let dflt = cap_tines(&topo, &res, None).len(); // omit tineDensity entirely
    let dense = cap_tines(&topo, &res, Some(1.0)).len();
    assert_eq!(
        dflt, dense,
        "omitting the slider must equal the dense comb (default != sparse)"
    );
}

// :115
#[test]
fn regression_c0fcf8e_a_stable_squat_part_still_gets_a_dense_comb_by_default() {
    // A low, wide part (low tip-over risk) is where the removed tip-risk scale handed out a sparse comb.
    let squat = Arc::new(tilted_block_topo(-45.0, 45.0, -45.0, 45.0, -3.0, 3.0, 30.0)); // wide + shallow = stable
    let res = analyze(&squat, 45.0, &I);
    let caps = cap_tines(&squat, &res, None); // default density
    assert!(
        caps.len() >= 12,
        "stable part starved of grip: only {} tines (the c0fcf8e symptom)",
        caps.len()
    );
    // ~2.3mm here: the 30deg tilt projects the 2mm arc-length step to 2/cos30.
    let p25 = pct_nn(&caps, 0.25);
    assert!(
        p25 <= 3.0,
        "stable part's dense end anchors are gone (p25 spacing {p25:.1}mm) -- tip-risk sparsening is back"
    );
}

// --- 3. edge bias ---------------------------------------------------------------------------------

// :135
#[test]
fn emit_tines_edge_biased_a_long_wall_is_denser_at_its_ends_than_its_middle() {
    let topo = tilted_block_topo(-15.0, 15.0, -25.0, 25.0, 0.0, 40.0, 40.0); // grippable end to end
    let tris = Tris::new(topo.pos.clone());
    let mut line = vec![];
    let mut y = -20.0;
    while y <= 20.0 {
        if let Some(z) = surface_z_at(&tris, 0.0, y) {
            line.push([0.0, y, z]);
        }
        y += 0.5;
    }
    let cfg = Config::default();
    let ctx = capturing_ctx(&cfg);
    emit(&line, &topo, cfg.prop.tine_step, &ctx); // dense request
    let caps = ctx.tine_cap.take().unwrap();
    assert!(
        caps.len() >= 8,
        "too few tines to judge the bias: {}",
        caps.len()
    );
    // classify each consecutive-tine gap by whether its midpoint lies in the END fifths or the MIDDLE
    let mut ys: Vec<f64> = caps.iter().map(|c| c.y).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (lo, hi) = (ys[0], *ys.last().unwrap());
    let (mid, half_run) = ((lo + hi) / 2.0, (hi - lo) / 2.0);
    let (mut end_gaps, mut mid_gaps) = (vec![], vec![]);
    for i in 1..ys.len() {
        let g = ys[i] - ys[i - 1];
        let frac = ((ys[i] + ys[i - 1]) / 2.0 - mid).abs() / half_run; // 0 at centre, 1 at an end
        if frac > 0.6 {
            end_gaps.push(g);
        } else {
            mid_gaps.push(g);
        }
    }
    let med = |a: &Vec<f64>| {
        let mut s = a.clone();
        s.sort_by(|p, q| p.partial_cmp(q).unwrap());
        s[a.len() >> 1]
    };
    assert!(
        !end_gaps.is_empty() && !mid_gaps.is_empty(),
        "run too short to split end/middle ({} tines)",
        ys.len()
    );
    assert!(
        med(&mid_gaps) > 1.4 * med(&end_gaps),
        "not edge-biased: middle gap {:.2} not looser than end gap {:.2}",
        med(&mid_gaps),
        med(&end_gaps)
    );
}
