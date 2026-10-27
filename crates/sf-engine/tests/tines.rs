//! Own tests for `sf_engine::prop::{tines, pins}`. The JS `tines.test.js`, `tine_shape.test.js`,
//! `tine_density.test.js` and `tines_realparts.test.js` are ported separately (docs/TEST-MAP.md);
//! bit-for-bit replay against JS is `crates/sf-testkit/tests/parity_tines.rs`.

use sf_engine::config::Config;
use sf_engine::overhangs::{Topology, build_topology};
use sf_engine::prop::config::{Iface, TineDir};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::pins::emit_pins;
use sf_engine::prop::tines::{TineOpts, emit_tines, tine_step_for};
use sf_engine::types::{IDENTITY3, V4, Xyz};

const ORIGIN: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

/// A closed box as a position soup (outward winding).
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

/// `tinesOnBlock` of tines.test.js: block x[-20,20] y[0,40] z[0,40], a wall line along +Y at
/// x = 0, z = 20, y = -5..5.
fn block_topo() -> Topology {
    build_topology(block(-20.0, 20.0, 0.0, 40.0, 0.0, 40.0))
}

fn line() -> Vec<[f64; 3]> {
    (-5..=5).map(|y| [0.0, f64::from(y), 20.0]).collect()
}

fn ctx(cfg: &Config) -> BuildCtx<'_> {
    BuildCtx::new(cfg, Crest::new(None, 0.2, cfg))
}

fn tines(cfg: &Config, opts: TineOpts) -> (usize, Vec<V4>) {
    let topo = block_topo();
    let mut out = Vec::new();
    let n = emit_tines(
        &line(),
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        opts,
        &ctx(cfg),
    );
    (n, out)
}

#[test]
fn tine_step_maps_the_slider_between_sparse_and_dense() {
    let c = Config::default();
    assert_eq!(tine_step_for(None, &c), c.prop.tine_step); // default: dense
    assert_eq!(tine_step_for(Some(1.0), &c), c.prop.tine_step);
    assert_eq!(tine_step_for(Some(0.0), &c), c.prop.tine_step_sparse);
    assert_eq!(tine_step_for(Some(-4.0), &c), c.prop.tine_step_sparse); // clamped
    assert_eq!(tine_step_for(Some(9.0), &c), c.prop.tine_step);
    assert!(tine_step_for(Some(f64::NAN), &c).is_nan());
    let mid = tine_step_for(Some(0.5), &c);
    assert!(mid > c.prop.tine_step && mid < c.prop.tine_step_sparse);
}

#[test]
fn a_grippable_overhang_yields_36_vertex_tines_one_bead_wide_and_one_layer_tall() {
    let c = Config::default();
    let (n, out) = tines(&c, TineOpts::default());
    assert!(n >= 1);
    // each tine is one loft box (36 vertices); a wall step adds another box where the layer
    // snap leaves the tine above the wall
    assert!(out.len() >= n * 36);
    assert!(out.iter().all(|v| v[3] == 0.0));
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for v in &out {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    assert!((hi[0] - lo[0] - c.prop.tine_w).abs() < 0.05); // one bead wide, across the run
    assert!(hi[1] > 1.0); // bites into the part (y > 0 is inside)
}

#[test]
fn no_part_to_grip_and_degenerate_lines_give_nothing() {
    let c = Config::default();
    let topo = block_topo();
    let mut out = Vec::new();
    // a line far from the part: every bite lands in air
    let air: Vec<[f64; 3]> = (-5..=5).map(|y| [100.0, f64::from(y), 20.0]).collect();
    let n = emit_tines(
        &air,
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        TineOpts::default(),
        &ctx(&c),
    );
    assert_eq!((n, out.len()), (0, 0));
    // under the wall's floor: nothing to attach to
    let low: Vec<[f64; 3]> = (-5..=5).map(|y| [0.0, f64::from(y), 0.5]).collect();
    assert_eq!(
        emit_tines(
            &low,
            None,
            &topo,
            &IDENTITY3,
            &ORIGIN,
            &mut out,
            TineOpts::default(),
            &ctx(&c)
        ),
        0
    );
    // one station, a zero-length run
    assert_eq!(
        emit_tines(
            &line()[..1],
            None,
            &topo,
            &IDENTITY3,
            &ORIGIN,
            &mut out,
            TineOpts::default(),
            &ctx(&c)
        ),
        0
    );
    let same = vec![[0.0, 0.0, 20.0]; 4];
    assert_eq!(
        emit_tines(
            &same,
            None,
            &topo,
            &IDENTITY3,
            &ORIGIN,
            &mut out,
            TineOpts::default(),
            &ctx(&c)
        ),
        0
    );
    assert!(out.is_empty());
}

#[test]
fn a_sparser_step_gives_fewer_tines_but_the_grip_floor_holds() {
    let c = Config::default();
    let dense = tines(
        &c,
        TineOpts {
            step: Some(c.prop.tine_step),
            ..TineOpts::default()
        },
    )
    .0;
    let sparse = tines(
        &c,
        TineOpts {
            step: Some(5.0),
            ..TineOpts::default()
        },
    )
    .0;
    assert!(sparse <= dense);
    // however sparse the request, a wall gets at least minGripTines' worth of spacing
    let extreme = tines(
        &c,
        TineOpts {
            step: Some(999.0),
            ..TineOpts::default()
        },
    )
    .0;
    assert!(extreme >= 1);
}

#[test]
fn tines_per_wall_asks_for_an_exact_count() {
    let mut c = Config::default();
    c.prop.tines_per_wall = 3.0;
    let (n, out) = tines(&c, TineOpts::default());
    assert!(n <= 3 && n >= 1, "{n}");
    assert!(out.len() >= n * 36);
}

#[test]
fn the_grip_out_parameter_takes_the_lowest_tine_bottom_and_never_rises() {
    let c = Config::default();
    let mut z = f64::INFINITY;
    let (n, _) = tines(
        &c,
        TineOpts {
            grip: Some(&mut z),
            ..TineOpts::default()
        },
    );
    assert!(n >= 1 && z.is_finite());
    // one layer under the layer-snapped underside
    assert!((z - (20.0 - c.prop.tine_h)).abs() < 1e-9, "{z}");
    // an already lower grip stays
    let mut low = -5.0;
    tines(
        &c,
        TineOpts {
            grip: Some(&mut low),
            ..TineOpts::default()
        },
    );
    assert_eq!(low, -5.0);
}

#[test]
fn the_capture_sees_each_tine_and_is_off_by_default() {
    let c = Config::default();
    let topo = block_topo();
    let cx = ctx(&c);
    let mut out = Vec::new();
    let n0 = emit_tines(
        &line(),
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        TineOpts::default(),
        &cx,
    );
    assert_eq!(cx.cap_len(), None);
    *cx.tine_cap.borrow_mut() = Some(Vec::new());
    let n = emit_tines(
        &line(),
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        TineOpts::default(),
        &cx,
    );
    assert_eq!(n, n0);
    let seeds = cx.tine_cap.borrow().clone().unwrap();
    assert_eq!(seeds.len(), n);
    for s in &seeds {
        // bite heading is a unit horizontal vector, into the part (+Y)
        assert!((s.bite_x.hypot(s.bite_y) - 1.0).abs() < 1e-12);
        assert!(s.bite_y > 0.5);
        assert!((s.z - (20.0 - c.prop.tine_h / 2.0)).abs() < 0.2);
    }
}

#[test]
fn the_crest_everywhere_tags_the_whole_comb_but_flat_mode_does_not() {
    let mut c = Config::default();
    c.prop.iface = Iface::All;
    let (n, out) = tines(&c, TineOpts::default());
    assert!(n >= 1 && out.iter().all(|v| v[3] == 1.0));
    c.prop.iface = Iface::Flat;
    let (_, out) = tines(&c, TineOpts::default());
    assert!(out.iter().all(|v| v[3] == 0.0));
    c.prop.iface = Iface::All;
    c.prop.iface_layers = 0.0; // crest off
    let (_, out) = tines(&c, TineOpts::default());
    assert!(out.iter().all(|v| v[3] == 0.0));
}

#[test]
fn vertical_tines_are_resin_pins() {
    let mut c = Config::default();
    c.prop.tine_dir = TineDir::Vertical;
    let (n, out) = tines(&c, TineOpts::default());
    assert!(n >= 1 || out.is_empty());
    assert_eq!(out.len() % 3, 0);
    // pins need the line to be the part's surface from below: on the block (surface at z = 20
    // seen from a wall top at 20 - gap) the corners are not inside, so they are skipped
    let topo = block_topo();
    let mut o2 = Vec::new();
    let direct = emit_pins(
        &line(),
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut o2,
        TineOpts::default(),
        &ctx(&c),
    );
    assert_eq!(direct, n);
    assert_eq!(o2, out);
}

#[test]
fn pins_on_a_slab_underside_bite_it_and_report_the_wall_bottom_as_grip() {
    let mut c = Config::default();
    c.prop.tine_dir = TineDir::Vertical;
    // a slab hanging at z 20..30; the contour is its underside
    let topo = build_topology(block(-20.0, 20.0, -20.0, 20.0, 20.0, 30.0));
    let wall: Vec<[f64; 3]> = (-5..=5).map(|x| [f64::from(x), 0.0, 20.0]).collect();
    let mut grip = f64::INFINITY;
    let mut out = Vec::new();
    let n = emit_pins(
        &wall,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        TineOpts {
            grip: Some(&mut grip),
            ..TineOpts::default()
        },
        &ctx(&c),
    );
    assert!(n >= 3, "{n}");
    // the pin reaches pinDepth above the surface, and starts tineOverlap under the wall top
    let top = out.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
    assert!((top - (20.0 + c.prop.pin_depth)).abs() < 0.06, "{top}");
    assert!(
        (grip - (20.0 - c.prop.gap - c.prop.tine_overlap)).abs() < 1e-9,
        "{grip}"
    );
    // a pin needs wall height above the floor: a surface under minTop gets none
    let none = emit_pins(
        &wall.iter().map(|p| [p[0], p[1], 0.5]).collect::<Vec<_>>(),
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut Vec::new(),
        TineOpts::default(),
        &ctx(&c),
    );
    assert_eq!(none, 0);
}
