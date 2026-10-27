//! Own tests of `sf_engine::fins::fill` and `sf_engine::fins::rows` (web/fins/fill.js, rows.js): the
//! slider, the guard rails and the shapes of the result. Parity with JS (every option of the pass on
//! real builds of the stress models) is `sf-testkit/tests/parity_fill.rs`; the JS `fill.test.js`,
//! `rows.test.js`, `islands.test.js` build through `buildFins` and are ported with it.

use sf_engine::config::Config;
use sf_engine::draw::DrawOpts;
use sf_engine::fins::build::FinsBuilt;
use sf_engine::fins::coverage::automatic_targets;
use sf_engine::fins::fill::*;
use sf_engine::fins::rows::{RowWallsArgs, row_walls};
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::overhangs::analyze;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_testkit::{load_model, rot_x};
use std::sync::Arc;

fn empty() -> FinsBuilt {
    FinsBuilt::empty(Seating {
        kind: SeatingKind::Face,
        span: 0.0,
        bed_area: 0.0,
    })
}

#[test]
fn the_slider_sets_the_reach_from_the_held_rule_to_the_dense_reach() {
    let cfg = Config::default();
    let held = cfg.prop.max_unsupported_span / 2.0;
    assert_eq!(fill_reach(0.0, &cfg), held);
    assert_eq!(fill_reach(0.5, &cfg), held);
    assert_eq!(fill_reach(1.0, &cfg), cfg.fill.dense_reach);
    let mid = fill_reach(0.75, &cfg);
    assert!(mid < held && mid > cfg.fill.dense_reach);
}

#[test]
fn a_part_with_no_overhang_has_no_red_and_gets_no_wall() {
    let cfg = Config::default();
    let topo = load_model("cube");
    let rot = rot_x(0.0);
    let res = analyze(&topo, 45.0, &rot);
    let targets = automatic_targets(&topo, &res, &rot);
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let f = fill_coverage(
        &topo,
        &targets,
        &rot,
        &FillOpts::default(),
        &empty(),
        &mut ctx,
    );
    assert!(f.props.is_empty() && f.triangles.is_empty() && f.tines == 0);
    assert_eq!(f.unserved, Unserved::default());
    // a run that found no red never reached the ribs and ties
    assert!(f.stats.ribs.is_none() && f.stats.ties.is_none());
    let (area, total) = bare_area(&topo, &targets, &rot, &FillOpts::default(), &[], &cfg);
    assert_eq!((area, total), (0.0, 0.0));
}

#[test]
fn a_tilted_cube_gets_walls_under_its_red_and_the_ranges_tile_the_triangles() {
    let cfg = Config::default();
    let topo = load_model("cube");
    let rot = rot_x(40.0);
    let res = analyze(&topo, 45.0, &rot);
    let targets = automatic_targets(&topo, &res, &rot);
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let opts = FillOpts {
        coverage: Some(0.5),
        ..Default::default()
    };
    let f = fill_coverage(&topo, &targets, &rot, &opts, &empty(), &mut ctx);
    assert!(!f.props.is_empty());
    assert!(f.stats.start_area > 0.0 && f.stats.ribs.is_some() && f.stats.ties.is_some());
    // every vertex belongs to exactly one wall/rib/tie range
    let mut covered = vec![0u8; f.triangles.len()];
    for p in &f.props {
        assert!(p.fill && p.line.len() >= 2);
        for [a, b] in &p.tri_ranges {
            for c in &mut covered[*a..*b] {
                *c += 1;
            }
        }
    }
    assert!(covered.iter().all(|&c| c == 1));
    // the tines add up
    assert_eq!(f.tines, f.props.iter().map(|p| p.tines).sum::<usize>());
    // under the sparse slider each group gets one wall and the rows never run
    let sparse = FillOpts {
        coverage: Some(0.1),
        ..Default::default()
    };
    let s = fill_coverage(&topo, &targets, &rot, &sparse, &empty(), &mut ctx);
    assert!(s.stats.rows.is_none() && s.props.len() <= f.props.len());
}

#[test]
fn rows_with_nothing_to_hold_lay_no_wall() {
    let cfg = Config::default();
    let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let tris = Arc::new(Tris::new(vec![]));
    let draw = DrawOpts::default();
    let r = row_walls(
        RowWallsArgs {
            pts: &[],
            heading: 0.0,
            r: 6.0,
            tris: &tris,
            draw_opts: &draw,
            fits: &mut |_| true,
            keep: &mut |_| {},
            min_gain: 0.5,
            room: &mut || true,
            n_faces: 0.0,
        },
        &mut ctx,
    );
    assert_eq!((r.walls, r.builds), (0, 0));
}
