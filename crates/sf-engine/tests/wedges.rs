//! Own tests of `sf_engine::fins::wedges` (web/fins/wedges.js) and `sf_engine::fins::pad`: the
//! branches the ported JS tests and the parity corpus do not name. Parity
//! (`sf-testkit/tests/parity_wedges.rs`, `parity_pad.rs`) pins equality with JS.

use indexmap::IndexSet;
use sf_engine::config::Config;
use sf_engine::fins::pad::{PadStyle, build_pad};
use sf_engine::fins::seating::seated_part_tris;
use sf_engine::fins::wedges::*;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::build::PropRecord;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_testkit::{block, load_model, rot_x};
use std::sync::Arc;

fn ctx(cfg: &Config) -> BuildCtx<'_> {
    BuildCtx::new(cfg, Crest::new(None, 0.0, cfg))
}

/// The cube stood on its edge: its down-facing patches are the wedge candidates.
fn cube_x45() -> (
    sf_engine::overhangs::Topology,
    [f64; 9],
    sf_engine::overhangs::Analysis,
) {
    let topo = load_model("cube");
    let rot = rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    (topo, rot, res)
}

#[test]
fn grip_patches_lists_only_downward_faces_and_maps_each_face_to_its_first_patch() {
    let (topo, rot, res) = cube_x45();
    let g = grip_patches(&topo, &res, &rot);
    assert!(!g.patches.is_empty());
    assert!(g.patches.iter().all(|p| p.n.z < -0.05));
    for (&f, &i) in &g.face_map {
        assert!(g.patches[i].faces.contains(&f));
        // the first patch that lists the face wins
        assert!(g.patches[..i].iter().all(|p| !p.faces.contains(&f)));
    }
}

#[test]
fn a_wedge_is_built_under_the_leaning_face_and_its_range_is_its_own() {
    let (topo, rot, res) = cube_x45();
    let cfg = Config::default();
    let c = ctx(&cfg);
    let p = grip_patches(&topo, &res, &rot)
        .patches
        .into_iter()
        .find(|p| p.area >= cfg.perp.min_area && p.u1 - p.u0 >= cfg.perp.min_width)
        .expect("a broad down-facing patch");
    let w = build_perp_fins(&p, &topo, &rot, &res.offset, &WedgeOpts::default(), &c);
    assert!(w.count > 0 && w.count == w.wedges.len());
    // the ranges tile the triangles contiguously from 0
    let mut at = 0;
    for r in &w.wedges {
        assert_eq!(r.tri_range[0], at);
        at = r.tri_range[1];
        assert!(r.tri_range[1] > r.tri_range[0]);
        assert!(r.line.len() >= 2 && r.height > 0.0 && r.span > 0.0);
    }
    assert_eq!(at, w.triangles.len());
    // tines off: the same wedges, no tines counted
    let nt = build_perp_fins(
        &p,
        &topo,
        &rot,
        &res.offset,
        &WedgeOpts {
            tines: Some(false),
            ..Default::default()
        },
        &c,
    );
    assert_eq!(nt.tines, 0);
    assert!(w.tines > 0 && nt.triangles.len() < w.triangles.len());
}

#[test]
fn a_patch_narrower_than_twice_the_inset_builds_nothing() {
    let (topo, rot, res) = cube_x45();
    let cfg = Config::default();
    let mut p = grip_patches(&topo, &res, &rot).patches.remove(0);
    p.u1 = p.u0 + 2.0 * cfg.perp.inset; // hi - lo == 0
    let w = build_perp_fins(
        &p,
        &topo,
        &rot,
        &res.offset,
        &WedgeOpts::default(),
        &ctx(&cfg),
    );
    assert_eq!(w, PerpFins::default());
}

#[test]
fn prop_serves_patch_counts_a_wall_in_the_footprint_plus_margin_but_not_a_low_tail() {
    let (topo, rot, res) = cube_x45();
    let cfg = Config::default();
    let p = grip_patches(&topo, &res, &rot).patches.remove(0);
    let (u, t) = ((p.u0 + p.u1) / 2.0, (p.t0 + p.t1) / 2.0);
    let m = sf_engine::planes::patch_point(&p, 0.0, u, t);
    let wall = |z: f64, squat: bool| PropRecord {
        line: vec![[m[0], m[1], z]],
        squat,
        ..Default::default()
    };
    assert!(!prop_serves_patch(&p, &[], &cfg));
    assert!(prop_serves_patch(&p, &[wall(5.0, false)], &cfg));
    // a tall wall's sub-minHeight tail is not a wall under this face; a squat one still counts
    assert!(!prop_serves_patch(&p, &[wall(0.5, false)], &cfg));
    assert!(prop_serves_patch(&p, &[wall(0.5, true)], &cfg));
    // far outside the box (8 mm margin)
    let far = PropRecord {
        line: vec![[m[0] + 500.0, m[1], 5.0]],
        ..Default::default()
    };
    assert!(!prop_serves_patch(&p, &[far], &cfg));
}

#[test]
fn wedge_veto_refuses_a_raster_wall_only_under_a_patch_the_normal_walls_leave_to_a_wedge() {
    let (topo, rot, res) = cube_x45();
    let cfg = Config::default();
    let patches: Vec<_> = grip_patches(&topo, &res, &rot)
        .patches
        .into_iter()
        .filter(|p| p.area >= cfg.perp.min_area && p.u1 - p.u0 >= cfg.perp.min_width)
        .collect();
    assert!(!patches.is_empty());
    let p = patches[0].clone();
    let m = sf_engine::planes::patch_point(&p, 0.0, (p.u0 + p.u1) / 2.0, (p.t0 + p.t1) / 2.0);
    let under = PropRecord {
        line: vec![[m[0], m[1], 8.0]],
        ..Default::default()
    };
    let far = PropRecord {
        line: vec![[m[0] + 500.0, m[1], 8.0]],
        ..Default::default()
    };
    let veto = wedge_veto(
        patches,
        Arc::new(topo),
        rot,
        res.offset,
        WedgeOpts::default(),
        cfg.clone(),
        Crest::new(None, 0.0, &cfg),
    );
    assert!(
        veto(&under, &[]),
        "a wedge builds here and no wall serves it"
    );
    assert!(!veto(&far, &[]));
    // a normal wall that already serves the patch lifts the veto (and the memo notices the new set)
    assert!(!veto(&under, std::slice::from_ref(&under)));
    assert!(veto(&under, &[]));
}

#[test]
fn bare_after_wedges_credits_a_wedge_vertex_close_in_plan_and_just_below() {
    // a post under a wide slab: the slab's underside is an overhang region
    let mut pos = block(-2.0, 2.0, -2.0, 2.0, 0.0, 10.0);
    pos.extend(block(-10.0, 10.0, -10.0, 10.0, 10.0, 12.0));
    let topo = build_topology(pos);
    let rot = IDENTITY3;
    let res = analyze(&topo, 45.0, &rot);
    let cfg = Config::default();
    let none: IndexSet<usize> = IndexSet::new();
    let bare = bare_after_wedges(&topo, &rot, &res, &none, &[], &cfg);
    assert!(!bare.is_empty());
    assert_eq!(
        unserved_after_wedges(&topo, &rot, &res, &none, &[], &cfg),
        bare.len()
    );
    // a region some prop wall serves is never bare
    let served: IndexSet<usize> = (0..res.regions.len()).collect();
    assert!(bare_after_wedges(&topo, &rot, &res, &served, &[], &cfg).is_empty());
    // wedge vertices within maxUnsupportedSpan in plan, 0..3mm under the underside credit its regions
    let o = res.offset;
    let grid = |dz: f64| -> Vec<[f64; 4]> {
        let mut v = Vec::new();
        for i in -5..=5 {
            for j in -5..=5 {
                v.push([
                    o.x + i as f64 * 2.0,
                    o.y + j as f64 * 2.0,
                    o.z + 10.0 + dz,
                    0.0,
                ]);
            }
        }
        v
    };
    assert!(bare_after_wedges(&topo, &rot, &res, &none, &grid(-1.0), &cfg).is_empty());
    // 4 mm below is out of reach; so is one above the face by more than 0.5
    assert_eq!(
        bare_after_wedges(&topo, &rot, &res, &none, &grid(-4.0), &cfg),
        bare
    );
    assert_eq!(
        bare_after_wedges(&topo, &rot, &res, &none, &grid(0.6), &cfg),
        bare
    );
}

// ---- the bed pad ------------------------------------------------------------------------------

fn pad_of(style: PadStyle, grab: f64) -> (Option<sf_engine::fins::pad::PadInfo>, usize) {
    let mut cfg = Config::default();
    cfg.pad.style = style;
    cfg.pad.grab = grab;
    let topo = load_model("cube");
    let rot = rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    let part: Arc<Tris> = seated_part_tris(&topo, &rot, &res.offset);
    let contact = sf_engine::fins::seating::bed_contact(&topo, &res, &rot).pts;
    let mut out = Vec::new();
    let info = build_pad(&contact, &part, &mut out, 0.2, &cfg);
    (info, out.len())
}

#[test]
fn fewer_than_three_contact_points_build_no_pad() {
    let cfg = Config::default();
    let mut out = Vec::new();
    let part = Tris::new(block(-5.0, 5.0, -5.0, 5.0, 0.0, 5.0));
    assert!(build_pad(&[[0.0, 0.0], [1.0, 1.0]], &part, &mut out, 0.2, &cfg).is_none());
    assert!(out.is_empty());
}

#[test]
fn a_pad_on_the_cube_edge_is_the_requested_style_and_auto_swaps_only_on_a_small_foot() {
    let (sure, n_sure) = pad_of(PadStyle::Sure, 0.05);
    let sure = sure.unwrap();
    assert_eq!(sure.style, PadStyle::Sure);
    assert_eq!(sure.auto_sure, Some(false));
    assert!(n_sure > 0 && sure.small_foot.is_some() && sure.outline.is_some());
    let (light, _) = pad_of(PadStyle::Light, 0.05);
    let light = light.unwrap();
    assert_eq!(light.style, PadStyle::Light);
    assert_eq!(light.auto_sure, None);
    // the cube's edge is a long outline: Auto stays Light
    let (auto, _) = pad_of(PadStyle::Auto, 0.05);
    assert_eq!(auto.unwrap().style, PadStyle::Light);
    let (custom, _) = pad_of(PadStyle::Custom, 0.05);
    assert_eq!(custom.unwrap().style, PadStyle::Custom);
}

#[test]
fn a_sphere_foot_is_small_so_auto_builds_sure_hold_and_says_so() {
    let topo = load_model("sphere");
    let rot = IDENTITY3;
    let res = analyze(&topo, 45.0, &rot);
    let part = seated_part_tris(&topo, &rot, &res.offset);
    let contact = sf_engine::fins::seating::bed_contact(&topo, &res, &rot).pts;
    let mut out = Vec::new();
    let info = build_pad(&contact, &part, &mut out, 0.2, &Config::default());
    let info = info.expect("a pad");
    assert_eq!(info.style, PadStyle::Sure);
    assert_eq!(info.auto_sure, Some(true));
    assert_eq!(info.small_foot, Some(true));
    let _ = Xyz::default();
}
