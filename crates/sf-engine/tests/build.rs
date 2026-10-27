//! Own tests of `sf_engine::prop::build::{build_props, build_short_walls}` (web/prop.js): each
//! option branch on a small synthetic part. Parity (`sf-testkit/tests/parity_build.rs`) pins
//! equality with JS; these pin that each switch does what its doc says.

use indexmap::IndexSet;
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::build::{BuildOpts, PropsPass, build_props, build_short_walls, no_props};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::types::{IDENTITY3, Rot3};
use sf_testkit::block;

/// A mushroom: a 10 x 10 stem under a 40 x 40 cap, so the cap's underside is one overhang region.
fn mushroom() -> (Topology, Analysis, Rot3) {
    let mut soup = block(-5.0, 5.0, -5.0, 5.0, 0.0, 10.0);
    soup.extend(block(-20.0, 20.0, -20.0, 20.0, 10.0, 14.0));
    let topo = build_topology(soup);
    let res = analyze(&topo, 45.0, &IDENTITY3);
    (topo, res, IDENTITY3)
}

fn run(opts: &BuildOpts) -> PropsPass {
    let (topo, res, rot) = mushroom();
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    build_props(&topo, &res, &rot, opts, &ctx)
}

fn regions_of(p: &PropsPass) -> IndexSet<usize> {
    p.props.iter().map(|w| w.region).collect()
}

#[test]
fn the_default_build_supports_the_cap() {
    let (_, res, _) = mushroom();
    assert!(
        !res.regions.is_empty(),
        "the cap's underside is an overhang"
    );
    let p = run(&BuildOpts::default());
    assert!(!p.props.is_empty(), "walls under the cap");
    assert!(!p.triangles.is_empty());
    assert_eq!(p.triangles.len() % 3, 0);
    assert!(p.served >= 1);
    assert_eq!(p.tines, 0, "tines are off by default");
}

#[test]
fn raster_false_turns_the_race_off_and_tags_nothing() {
    let p = run(&BuildOpts {
        raster: Some(false),
        ..Default::default()
    });
    assert!(
        p.props.iter().all(|w| w.raster != Some(true)),
        "no raster wall without the race"
    );
    assert!(p.sag_regions.is_none(), "only a raster pass has sagRegions");
    assert!(!p.props.is_empty());
}

#[test]
fn rim_walls_false_leaves_no_rim_wall() {
    let p = run(&BuildOpts {
        rim_walls: Some(false),
        ..Default::default()
    });
    assert!(p.props.iter().all(|w| w.rim != Some(true)));
    assert!(!p.props.is_empty());
}

#[test]
fn downhill_and_curved_walls_are_races_that_keep_a_valid_pass() {
    // The races only keep a pass that holds more, so the result is never worse than the normal one.
    let base = run(&BuildOpts::default());
    for opts in [
        BuildOpts {
            downhill: Some(true),
            ..Default::default()
        },
        BuildOpts {
            curved_walls: Some(true),
            ..Default::default()
        },
    ] {
        let p = run(&opts);
        assert!(
            p.served >= base.served,
            "a race must not lose a served region"
        );
        assert_eq!(p.triangles.len() % 3, 0);
        assert!(!p.props.is_empty());
    }
    // the off switches leave the tags unset
    assert!(
        base.props
            .iter()
            .all(|w| w.downhill != Some(true) && w.curved != Some(true))
    );
}

#[test]
fn only_regions_builds_that_subset_only() {
    // (the races rebuild with their own onlyRegions, so isolate the normal pass)
    let plain = |only: IndexSet<usize>| BuildOpts {
        only_regions: Some(only),
        raster: Some(false),
        rim_walls: Some(false),
        ..Default::default()
    };
    let none = run(&plain(IndexSet::new()));
    assert!(none.props.is_empty(), "an empty subset builds nothing");
    let r = *regions_of(&run(&BuildOpts::default()))
        .first()
        .expect("a region is served");
    let one = run(&plain([r].into_iter().collect()));
    assert!(!one.props.is_empty());
    assert!(regions_of(&one).iter().all(|&x| x == r), "only region {r}");
}

#[test]
fn circular_pass_on_a_square_part_has_no_circular_wall() {
    // circularRegions finds no round bore or flange in a mushroom, so the pass is a no-op
    let p = run(&BuildOpts {
        circular_pass: true,
        ..Default::default()
    });
    assert!(p.props.iter().all(|w| w.circular != Some(true)));
}

#[test]
fn no_props_is_the_empty_pass_and_a_part_without_overhangs_builds_nothing() {
    let n = no_props();
    assert!(n.props.is_empty() && n.triangles.is_empty());
    assert_eq!((n.served, n.tines), (0, 0));
    assert!(!n.sag_risk && n.sag_regions.is_none() && n.served_regions.is_empty());

    // a plain block sits on the bed: no overhang, so buildProps builds nothing
    let topo = build_topology(block(-10.0, 10.0, -10.0, 10.0, 0.0, 10.0));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let p = build_props(&topo, &res, &IDENTITY3, &BuildOpts::default(), &ctx);
    assert!(p.props.is_empty() && p.triangles.is_empty());
}

#[test]
fn build_short_walls_builds_the_given_regions_and_no_others() {
    let (topo, res, rot) = mushroom();
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let none = build_short_walls(
        &topo,
        &res,
        &rot,
        &BuildOpts::default(),
        &IndexSet::new(),
        &ctx,
    );
    assert!(none.props.is_empty());
    let regions: IndexSet<usize> = (0..res.regions.len()).collect();
    let p = build_short_walls(&topo, &res, &rot, &BuildOpts::default(), &regions, &ctx);
    assert_eq!(p.triangles.len() % 3, 0);
    assert!(regions_of(&p).iter().all(|r| regions.contains(r)));
}
