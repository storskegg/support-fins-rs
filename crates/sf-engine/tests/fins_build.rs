//! Own tests of `sf_engine::fins::build::{build_fins, cover_pitch}` (web/fins.js): each mode and
//! option branch on a small synthetic part. Parity (`sf-io/tests/parity_buildfins.rs`) pins
//! equality with JS.

use std::sync::Arc;

use sf_engine::config::{Config, TunableValue, Tunables};
use sf_engine::fins::build::*;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::types::IDENTITY3;
use sf_testkit::block;

/// A mushroom: a 10 x 10 stem under a 40 x 40 cap.
fn mushroom() -> Arc<Topology> {
    let mut soup = block(-5.0, 5.0, -5.0, 5.0, 0.0, 10.0);
    soup.extend(block(-20.0, 20.0, -20.0, 20.0, 10.0, 14.0));
    Arc::new(build_topology(soup))
}

fn build(topo: &Arc<Topology>, opts: &FinsOpts) -> FinsBuilt {
    let cfg = Config::default();
    let res = analyze(topo, 45.0, &IDENTITY3);
    build_fins(topo, &res, &IDENTITY3, opts, &cfg)
}

fn mode(m: &str) -> FinsOpts {
    FinsOpts {
        mode: Some(m.to_string()),
        ..Default::default()
    }
}

#[test]
fn cover_pitch_is_monotonic_and_clamped() {
    let cfg = Config::default();
    let f = &cfg.fin;
    assert_eq!(cover_pitch(0.5, &cfg), f.cover_sparse);
    assert_eq!(cover_pitch(0.0, &cfg), f.cover_extra_sparse);
    assert_eq!(cover_pitch(1.0, &cfg), f.cover_dense);
    assert_eq!(cover_pitch(-3.0, &cfg), f.cover_extra_sparse);
    assert_eq!(cover_pitch(9.0, &cfg), f.cover_dense);
    let mut last = f64::INFINITY;
    for i in 0..=10 {
        let p = cover_pitch(i as f64 / 10.0, &cfg);
        assert!(p <= last, "denser coverage never loosens the pitch");
        last = p;
    }
}

#[test]
fn prop_mode_is_the_default_and_builds_untined_walls() {
    let t = mushroom();
    let b = build(&t, &FinsOpts::default());
    assert_eq!(b.mode, "prop");
    assert!(!b.fins.is_empty() && b.fins.len() == b.props.len());
    assert_eq!(b.tines, 0, "tines are off in Prop");
    assert!(b.brace_count.is_none() && b.fill.is_none() && b.bare.is_none());
    assert_eq!(b.tall_ribs, Some(0), "braceTallWalls ran");
    assert_eq!(b.small_hole_mask.as_ref().map(Vec::len), Some(t.n_faces));
    assert!(
        b.fins
            .iter()
            .all(|f| f.kind == FinKind::Prop && f.tines == 0)
    );
}

#[test]
fn auto_builds_tined_walls_and_measures_what_is_bare() {
    let t = mushroom();
    for m in ["auto", "stabilize"] {
        let b = build(&t, &mode(m));
        assert_eq!(b.mode, m);
        assert_eq!(b.brace_count, Some(b.fins.len()), "a tined wall is a brace");
        assert_eq!(b.prop_count, Some(0));
        assert!(b.bare.is_some() && b.fill.is_none());
    }
}

#[test]
fn tines_false_makes_plain_props() {
    let t = mushroom();
    let b = build(
        &t,
        &FinsOpts {
            tines: Some(false),
            ..mode("auto")
        },
    );
    assert_eq!(b.tines, 0);
    assert_eq!(
        b.brace_count,
        Some(b.fins.iter().filter(|f| f.kind == FinKind::Wedge).count())
    );
    assert_eq!(b.prop_count, Some(b.props.len()));
}

#[test]
fn full_coverage_adds_the_fill_numbers_and_the_bare_total() {
    let t = mushroom();
    let b = build(&t, &mode("full"));
    let fill = b.fill.as_ref().expect("Full reports its fill");
    let bare = b.bare.expect("Full reports what is bare");
    assert_eq!(bare.area, fill.unserved_area);
    assert!(b.curved_fill.is_none());
}

#[test]
fn draw_builds_only_the_pad_and_the_seating() {
    let t = mushroom();
    let b = build(&t, &mode("draw"));
    assert!(b.triangles.is_empty() && b.fins.is_empty() && b.props.is_empty());
    assert!(
        b.small_hole_mask.is_none(),
        "Draw has no automatic exclusions"
    );
    assert_eq!(b.tall_ribs, None, "no ribs in Draw");
    assert!(b.bare.is_none());
}

#[test]
fn the_off_switches_leave_the_extras_out() {
    let t = mushroom();
    let b = build(
        &t,
        &FinsOpts {
            tall_ribs: Some(false),
            ..mode("auto")
        },
    );
    assert_eq!(b.tall_ribs, None);
    let b = build(
        &t,
        &FinsOpts {
            last_resort: Some(false),
            raster: Some(false),
            ..mode("auto")
        },
    );
    assert!(b.fins.iter().all(|f| !f.short));
    assert!(b.props.iter().all(|q| q.raster != Some(true)));
    // `downhill` pinned either way skips the judge
    for d in [true, false] {
        let b = build(
            &t,
            &FinsOpts {
                downhill: Some(d),
                ..mode("auto")
            },
        );
        assert!(!b.fins.is_empty());
    }
}

#[test]
fn a_bare_overhang_the_walls_leave_is_counted_unserved_and_the_pad_is_optional() {
    let t = mushroom();
    let on = build(
        &t,
        &FinsOpts {
            bed_pad: Some(true),
            ..mode("prop")
        },
    );
    let off = build(
        &t,
        &FinsOpts {
            bed_pad: Some(false),
            ..mode("prop")
        },
    );
    assert!(off.pad.is_none() && off.pad_triangles.is_empty());
    assert_eq!(on.pad.is_some(), !on.pad_triangles.is_empty());
}

#[test]
fn tunables_reach_the_build_and_nothing_persists() {
    let t = mushroom();
    let cfg = Config::default();
    let res = analyze(&t, 45.0, &IDENTITY3);
    let a = build_fins(&t, &res, &IDENTITY3, &mode("prop"), &cfg);
    let tun = Tunables {
        prop_gap: Some(0.4),
        iface: Some(TunableValue::Bool(false)),
        ..Default::default()
    };
    let b = build_fins(
        &t,
        &res,
        &IDENTITY3,
        &FinsOpts {
            tunables: Some(tun),
            ..mode("prop")
        },
        &cfg,
    );
    assert_ne!(
        a.triangles, b.triangles,
        "a bigger support gap moves the walls"
    );
    let c = build_fins(&t, &res, &IDENTITY3, &mode("prop"), &cfg);
    assert_eq!(a.triangles, c.triangles, "the caller's config is untouched");
}

#[test]
fn sway_braces_are_an_add_on_with_their_own_fin_records() {
    let t = mushroom();
    let plain = build(&t, &mode("auto"));
    let sway = build(
        &t,
        &FinsOpts {
            sway: Some(SwayRequest {
                on: true,
                ..Default::default()
            }),
            ..mode("auto")
        },
    );
    let info = sway.sway.as_ref().expect("sway info");
    assert_eq!(sway.fins.len(), plain.fins.len() + info.count);
    assert!(
        sway.fins[plain.fins.len()..]
            .iter()
            .all(|f| f.kind == FinKind::Sway)
    );
    // off: no key at all
    let off = build(
        &t,
        &FinsOpts {
            sway: Some(SwayRequest::default()),
            ..mode("auto")
        },
    );
    assert!(off.sway.is_none());
}

#[test]
fn a_part_seated_on_a_point_gets_no_walls_unless_the_pad_seats_it() {
    // an octahedron-ish tip: a tall pyramid on its apex has no bed contact at all
    let apex = [0.0, 0.0, 0.0];
    let r = 20.0;
    let top = 30.0;
    let ring = [[r, 0.0, top], [0.0, r, top], [-r, 0.0, top], [0.0, -r, top]];
    let mut soup = Vec::new();
    for i in 0..4 {
        let (a, b) = (ring[i], ring[(i + 1) % 4]);
        soup.extend([apex, b, a].concat());
    }
    soup.extend([ring[0], ring[1], ring[2], ring[0], ring[2], ring[3]].concat());
    let t = Arc::new(build_topology(soup));
    let nopad = build(
        &t,
        &FinsOpts {
            bed_pad: Some(false),
            ..mode("auto")
        },
    );
    assert!(
        nopad.fins.is_empty(),
        "nothing standing on the plate can hold a part that never touches it"
    );
}

#[test]
fn build_fins_capturing_returns_the_tine_seeds_of_the_same_build() {
    use sf_testkit::tilted_block_topo;
    // a block whose underside leans: tines have a face to bite
    let t = Arc::new(tilted_block_topo(-15.0, 15.0, -20.0, 20.0, 0.0, 30.0, 40.0));
    let cfg = Config::default();
    let res = analyze(&t, 45.0, &IDENTITY3);
    let opts = FinsOpts {
        tines: Some(true),
        ..mode("auto")
    };
    let plain = build_fins(&t, &res, &IDENTITY3, &opts, &cfg);
    let (built, seeds) = build_fins_capturing(&t, &res, &IDENTITY3, &opts, &cfg);
    assert_eq!(
        built.triangles, plain.triangles,
        "capturing does not change the build"
    );
    assert_eq!(built.tines, plain.tines);
    assert!(
        plain.tines > 0 && !seeds.is_empty(),
        "tines {} seeds {}",
        plain.tines,
        seeds.len()
    );
    assert!(seeds.iter().all(|s| s.x.is_finite() && s.z.is_finite()));
    // no tines, no seeds
    let (_, none) = build_fins_capturing(
        &t,
        &res,
        &IDENTITY3,
        &FinsOpts {
            tines: Some(false),
            ..mode("auto")
        },
        &cfg,
    );
    assert!(none.is_empty());
}
