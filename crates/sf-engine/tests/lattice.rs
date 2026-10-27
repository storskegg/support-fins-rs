//! Tests of `sf_engine::prop::lattice`: which regions are nets, how a net is cut into struts, and
//! the patches `strut_patches` makes (the parity corpus `lattice/*.soup` pins them bit for bit).
mod support;
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::lattice::*;
use sf_engine::prop::surface::seat;
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;
use std::sync::Arc;
use support::*;

fn setup(pos: Vec<f64>) -> (Topology, Analysis) {
    let topo = build_topology(pos);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    (topo, res)
}

fn struts_of(topo: &Topology, res: &Analysis, cfg: &Config) -> Option<Struts> {
    let seated = |f: usize| -> [[f64; 3]; 3] {
        let off = &res.offset;
        [
            seat(&topo.pos, f * 9, &IDENTITY3, off),
            seat(&topo.pos, f * 9 + 3, &IDENTITY3, off),
            seat(&topo.pos, f * 9 + 6, &IDENTITY3, off),
        ]
    };
    lattice_struts(topo, &res.regions[0].faces, &seated, cfg)
}

fn bars(w: usize, gap: usize) -> impl Fn(usize, usize) -> bool {
    move |i, j| i % gap < w || j % gap < w
}

#[test]
fn a_grid_of_bars_is_cut_into_its_struts() {
    let (topo, res) = setup(net(24, 1.5, &bars(2, 6), &flat_bottom, 26.0));
    let s = struts_of(&topo, &res, &Config::default()).expect("a net");
    assert!(s.groups.len() >= 5);
    let mut seen = std::collections::HashSet::new();
    for g in &s.groups {
        assert!(!g.is_empty());
        for f in g {
            assert!(seen.insert(*f), "a face belongs to one strut only");
            assert!(res.regions[0].faces.contains(f));
        }
    }
    assert_eq!(seen.len() + s.lost, res.regions[0].faces.len());
}

#[test]
fn only_a_net_qualifies() {
    let cfg = Config::default();
    // one band: no junction
    let (t, r) = setup(net(24, 1.5, &|_, j| j < 2, &flat_bottom, 26.0));
    assert!(struts_of(&t, &r, &cfg).is_none());
    // a broad plate: not narrow everywhere
    let (t, r) = setup(net(14, 1.5, &|_, _| true, &flat_bottom, 26.0));
    assert!(struts_of(&t, &r, &cfg).is_none());
    // a ring: no junction either
    let ring = |i: usize, j: usize| {
        let d = ((i as f64 - 11.5).powi(2) + (j as f64 - 11.5).powi(2)).sqrt();
        d > 9.0 && d < 11.0
    };
    let (t, r) = setup(net(24, 1.5, &ring, &flat_bottom, 26.0));
    assert!(struts_of(&t, &r, &cfg).is_none());
    // under tubeMinArea: the small-tube route already serves it
    let (t, r) = setup(net(10, 1.5, &bars(2, 5), &flat_bottom, 26.0));
    assert!(struts_of(&t, &r, &cfg).is_none());
    // too big a raster (cells of 800 mm): not a strut net
    let (t, r) = setup(net(3, 800.0, &|_, _| true, &flat_bottom, 26.0));
    assert!(struts_of(&t, &r, &cfg).is_none());
}

#[test]
fn a_net_with_too_few_struts_is_refused() {
    // two crossing bars make four arms: under MIN_STRUTS (5)
    let (t, r) = setup(net(
        24,
        1.5,
        &|i, j| (11..13).contains(&i) || (11..13).contains(&j),
        &flat_bottom,
        26.0,
    ));
    let cfg = Config::default();
    assert!(struts_of(&t, &r, &cfg).is_none());
}

#[test]
fn flat_struts_become_whole_strut_patches_along_their_axis() {
    let cfg = Config::default();
    let (topo, res) = setup(net(24, 1.5, &bars(2, 6), &flat_bottom, 26.0));
    let s = struts_of(&topo, &res, &cfg).unwrap();
    let off = res.offset;
    let seated = |f: usize| -> [[f64; 3]; 3] {
        [
            seat(&topo.pos, f * 9, &IDENTITY3, &off),
            seat(&topo.pos, f * 9 + 3, &IDENTITY3, &off),
            seat(&topo.pos, f * 9 + 6, &IDENTITY3, &off),
        ]
    };
    let region_tris = Arc::new(Tris::new(vec![]));
    let patches = strut_patches(
        &topo,
        &IDENTITY3,
        &s.groups,
        &seated,
        1.0,
        7,
        &region_tris,
        &cfg,
    );
    assert_eq!(patches.len(), s.groups.len());
    for (p, g) in patches.iter().zip(&s.groups) {
        assert_eq!(&p.faces, g);
        assert_eq!(p.region, 7);
        assert!(p.strut);
        assert!(Arc::ptr_eq(&p.tris, &region_tris));
        // a flat underside is no tube: one patch along the strut's length
        assert!(p.lines.is_none() && p.reach.is_none() && p.small_tube.is_none());
        let a = p.axis.expect("axis");
        assert!((a[0].hypot(a[1]) - 1.0).abs() < 1e-12);
        // bars here run along x or y
        assert!(a[0].abs() > 0.99 || a[1].abs() > 0.99, "{a:?}");
        let want: f64 = g.iter().map(|&f| topo.area[f]).sum();
        assert_eq!(p.area, want);
    }
}

#[test]
fn round_struts_take_the_tube_route_and_race_their_split_patches_when_small() {
    let cfg = Config::default();
    let k = 0.6;
    let cell = 0.5;
    let centre = |v: usize, w: usize, gap: usize| -> f64 {
        let c = w as f64 * cell / 2.0;
        let m = (v as f64 * cell).rem_euclid(gap as f64 * cell);
        (m - c).abs().min((m - c - gap as f64 * cell).abs())
    };
    let bottom = |i: usize, j: usize| 20.0 + k * centre(i, 6, 20).min(centre(j, 6, 20)).powi(2);
    let (topo, res) = setup(net(60, cell, &bars(6, 20), &bottom, 26.0));
    let s = struts_of(&topo, &res, &cfg).expect("a round net");
    let off = res.offset;
    let seated = |f: usize| -> [[f64; 3]; 3] {
        [
            seat(&topo.pos, f * 9, &IDENTITY3, &off),
            seat(&topo.pos, f * 9 + 3, &IDENTITY3, &off),
            seat(&topo.pos, f * 9 + 6, &IDENTITY3, &off),
        ]
    };
    let rt = Arc::new(Tris::new(vec![]));
    let patches = strut_patches(&topo, &IDENTITY3, &s.groups, &seated, 1.0, 0, &rt, &cfg);
    let tubes: Vec<&StrutPatch> = patches.iter().filter(|p| p.lines.is_some()).collect();
    assert!(!tubes.is_empty(), "round struts route as tubes");
    for p in tubes {
        assert!(p.axis.is_none());
        assert!(p.reach.as_ref().unwrap().v.len() == p.faces.len() * 9);
        // a small tube (under tubeMinArea) is raced against its split patches
        let small = p.area < cfg.prop.tube_min_area;
        assert_eq!(p.small_tube.as_ref().unwrap().is_some(), small);
        if let Some(Some(split)) = &p.small_tube {
            assert!(
                split
                    .iter()
                    .all(|sp| (sp.axis[0].hypot(sp.axis[1]) - 1.0).abs() < 1e-12)
            );
        }
    }
}
