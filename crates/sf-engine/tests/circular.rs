//! Tests of `sf_engine::prop::circular`: recognition of complete circular rims (and every way it
//! refuses), the caller-owned cache, the surrounding-geometry phase, and the early exits of
//! `circular_tracks`. (Its radial candidates need `sweep::foot_for`, track B; those tests are
//! ignored until it lands, and the parity corpus `circular/circularTracks.soup` pins them.)
mod support;
use sf_engine::config::Config;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::circular::*;
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;
use std::sync::Arc;
use support::*;

fn recess_region(
    segments: usize,
) -> (
    Vec<f64>,
    sf_engine::overhangs::Topology,
    sf_engine::overhangs::Analysis,
) {
    let topo = build_topology(
        recessed_body(segments, [0.0; 3])
            .iter()
            .map(|&v| v as f32 as f64)
            .collect(),
    );
    let res = analyze(&topo, 70.0, &IDENTITY3);
    let tris = seated_region(&topo, &res, &IDENTITY3, 0);
    (tris, topo, res)
}

#[test]
fn a_conical_recess_is_a_complete_circular_footprint() {
    let (tris, _, _) = recess_region(120);
    let f = circular_footprint(&tris).expect("a circular annulus");
    assert!((f.outer_radius - 25.0).abs() < 1e-3, "{}", f.outer_radius);
    assert!((f.inner_radius - 15.0).abs() < 1e-3);
    assert!(f.centre[0].abs() < 1e-3 && f.centre[1].abs() < 1e-3);
    assert_eq!(f.angular_extent, [0.0, 2.0 * std::f64::consts::PI]);
    // the cone climbs from the opening (outer, seated z 0) to the bore floor (inner)
    assert!(f.radial_profile[0][1] > f.radial_profile[1][1]);
    assert_eq!(f.radial_profile[0][0], f.inner_radius);
    assert_eq!(f.radial_profile[1][0], f.outer_radius);
    assert_eq!(f.axis, [f.phase.cos(), f.phase.sin()]);
    assert!(f.surrounding.is_none(), "added by circular_regions");
}

#[test]
fn a_full_disk_is_a_footprint_with_no_inner_loop() {
    // a flat disk, 48 sectors, hanging at z = 5
    let n = 48;
    let r = 10.0;
    let pt = |i: usize| {
        let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        [r * a.cos(), r * a.sin(), 5.0]
    };
    let mut tris = Vec::new();
    for i in 0..n {
        tris.extend([0.0, 0.0, 5.0]);
        tris.extend(pt(i));
        tris.extend(pt(i + 1));
    }
    let f = circular_footprint(&tris).expect("a disk");
    assert_eq!(f.inner_radius, 0.0);
    assert!((f.outer_radius - 10.0).abs() < 1e-9);
    assert_eq!(f.radial_profile[0][1], f.radial_profile[1][1]);
}

#[test]
fn shapes_that_are_not_complete_circles_keep_the_old_path() {
    assert!(circular_footprint(&[]).is_none(), "no loop at all");
    // one triangle: an open boundary of 3 points (fewer than 12)
    assert!(circular_footprint(&[0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 5.0, 0.0]).is_none());
    // a degenerate triangle with a repeated corner
    assert!(circular_footprint(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]).is_none());
    // an edge shared by three triangles
    let t = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| [a, b, c].concat();
    let three = [
        t([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        t([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
        t([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.0, 1.0]),
    ]
    .concat();
    assert!(circular_footprint(&three).is_none());
    // an ellipse: the same recess stretched 20 % in y
    let (mut tris, _, _) = recess_region(120);
    for v in tris.chunks_mut(3) {
        v[1] *= 1.2;
    }
    assert!(circular_footprint(&tris).is_none());
    // a rim with a gap: drop the triangles of a 60 degree sector (more than a twelfth missing)
    let (tris, _, _) = recess_region(120);
    let kept: Vec<f64> = tris
        .chunks(9)
        .filter(|f| {
            let a = (f[1] + f[4] + f[7]).atan2(f[0] + f[3] + f[6]).to_degrees();
            !(0.0..60.0).contains(&a)
        })
        .flatten()
        .copied()
        .collect();
    assert!(circular_footprint(&kept).is_none());
    // coarse rims (fewer than 12 rim points) are refused
    let (tris, _, _) = recess_region(8);
    assert!(circular_footprint(&tris).is_none());
}

#[test]
fn regions_are_found_per_region_index_and_cached_by_content() {
    let (_, topo, res) = recess_region(120);
    let cfg = Config::default();
    let cache = CircularCache::new();
    let a = circular_regions(&topo, &res, &IDENTITY3, &cache, &cfg);
    assert_eq!(a.len(), 1);
    assert!(a.contains_key(&0));
    let s = a[&0]
        .surrounding
        .as_ref()
        .expect("circular_regions adds it");
    assert_eq!(
        s.ceiling,
        a[&0].radial_profile[0][1].max(a[&0].radial_profile[1][1])
    );
    // the same inputs: the cache hands back the same map
    let b = circular_regions(&topo, &res, &IDENTITY3, &cache, &cfg);
    assert!(Arc::ptr_eq(&a, &b));
    // a different foot reach is a different key
    let mut cfg2 = Config::default();
    cfg2.prop.foot_max += 1.0;
    let c = circular_regions(&topo, &res, &IDENTITY3, &cache, &cfg2);
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!(c[&0].centre, a[&0].centre);
    let (ra, rc) = (
        a[&0].surrounding.as_ref().unwrap().reach,
        c[&0].surrounding.as_ref().unwrap().reach,
    );
    assert_eq!(rc, ra + 1.0, "the reach follows footMax");
    // a different pose is a different key
    let rot = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let mut moved = res.clone();
    moved.offset.x += 1.0;
    let d = circular_regions(&topo, &moved, &rot, &cache, &cfg2);
    assert!(!Arc::ptr_eq(&c, &d));
}

#[test]
fn small_regions_are_skipped_and_a_sloped_part_has_no_circular_frames() {
    // a tilted recess is no longer horizontal: no region is a complete horizontal circle
    let topo = build_topology(
        recessed_body(120, [0.0; 3])
            .iter()
            .map(|&v| v as f32 as f64)
            .collect(),
    );
    let tilt = {
        let (s, c) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
        [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
    };
    let res = analyze(&topo, 70.0, &tilt);
    let frames = circular_regions(
        &topo,
        &res,
        &tilt,
        &CircularCache::new(),
        &Config::default(),
    );
    assert!(frames.is_empty());
}

#[test]
fn a_neighbouring_rib_fixes_the_phase() {
    let parts = [
        recessed_body(120, [0.0; 3])
            .chunks(3)
            .map(|v| [v[0], v[1], v[2]])
            .collect::<Vec<_>>(),
        block(26.0, 34.0, -1.0, 1.0, 0.0, 6.0),
    ];
    let topo = build_topology(flat(&parts).iter().map(|&v| v as f32 as f64).collect());
    let res = analyze(&topo, 70.0, &IDENTITY3);
    let frames = circular_regions(
        &topo,
        &res,
        &IDENTITY3,
        &CircularCache::new(),
        &Config::default(),
    );
    let f = frames.values().next().expect("the recess is still a frame");
    let ph = f
        .surrounding
        .as_ref()
        .unwrap()
        .phase
        .expect("the rib breaks the symmetry");
    let two_pi = 2.0 * std::f64::consts::PI;
    assert_eq!(f.phase, ((ph % two_pi) + two_pi) % two_pi);
    assert_eq!(f.axis, [f.phase.cos(), f.phase.sin()]);
}

fn frame_with(inner: f64, outer: f64, z_in: f64, z_out: f64) -> CircularFrame {
    CircularFrame {
        centre: [0.0, 0.0],
        inner_radius: inner,
        outer_radius: outer,
        radial_profile: [[inner, z_in], [outer, z_out]],
        angular_extent: [0.0, 2.0 * std::f64::consts::PI],
        phase: 0.0,
        axis: [1.0, 0.0],
        surrounding: None,
    }
}

#[test]
fn tracks_refuse_before_any_foot_is_measured() {
    let cfg = Config::default();
    let tris = Tris::new(vec![]);
    // an inner radius under 1 mm
    assert!(circular_tracks(&frame_with(0.5, 15.0, 4.0, 10.0), &tris, 1.0, 12.0, &cfg).is_none());
    // a flat profile (no slope to prop)
    assert!(circular_tracks(&frame_with(5.0, 15.0, 4.0, 4.005), &tris, 1.0, 12.0, &cfg).is_none());
}

#[test]
#[ignore = "needs sweep::foot_for (M2 track B)"]
fn tracks_lay_radial_walls_over_an_annular_slope() {
    let cfg = Config::default();
    let (tris, _, _) = recess_region(120);
    let tris = Tris::new(tris);
    let frame = {
        let (t, topo, res) = recess_region(120);
        let _ = t;
        circular_regions(&topo, &res, &IDENTITY3, &CircularCache::new(), &cfg)[&0].clone()
    };
    let lines = circular_tracks(&frame, &tris, 1.0, 12.0, &cfg).expect("tracks");
    assert!(lines.len() >= 6, "{}", lines.len());
    for l in &lines {
        assert!(l.len() as f64 >= cfg.prop.min_stations);
        let r0 = l[0][0].hypot(l[0][1]);
        let r1 = l.last().unwrap()[0].hypot(l.last().unwrap()[1]);
        assert!(r1 > r0, "walls run outward from the inner end");
    }
    // a thin ring is too short for a wall
    let thin = frame_with(14.0, 14.8, 4.0, 10.0);
    assert!(circular_tracks(&thin, &tris, 1.0, 12.0, &cfg).is_none());
}
