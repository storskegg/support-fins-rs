//! Own tests of `sf_engine::fins::tall` (web/fins/tall.js): the plan grid's tag semantics and
//! `brace_tall_walls`'s selection and bookkeeping on hand-made builds. (Parity with JS is
//! `sf-testkit/tests/parity_tall.rs`; the JS `tall.test.js` builds a part through `buildFins` and is
//! ported with it.)

use sf_engine::config::Config;
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt};
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::fins::tall::{PlanGrid, brace_tall_walls};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::types::{IDENTITY3, V4};
use sf_testkit::block;

fn tri(z: f64) -> Vec<V4> {
    vec![[0.0, 0.0, z, 0.0], [3.0, 0.0, z, 0.0], [0.0, 3.0, z, 0.0]]
}

#[test]
fn a_piece_counts_unless_it_is_the_ribs_own_wall() {
    let cfg = Config::default();
    let mut g = PlanGrid::new();
    g.add(&tri(0.0), Some(1), |_, _, _| true);
    let rib = tri(5.0);
    assert!(
        g.hits(&rib, None, &cfg),
        "undefined own: every piece counts"
    );
    assert!(g.hits(&rib, Some(2), &cfg), "another fin's piece counts");
    assert!(
        !g.hits(&rib, Some(1), &cfg),
        "its own wall's piece does not"
    );
    // the pad's untagged (null) pieces differ from every number and from undefined
    let mut g = PlanGrid::new();
    g.add(&tri(0.0), None, |_, _, _| true);
    assert!(g.hits(&rib, Some(1), &cfg));
    assert!(g.hits(&rib, None, &cfg));
}

#[test]
fn only_pieces_lower_than_the_ribs_top_within_side_clear_in_plan_clash() {
    let cfg = Config::default();
    let mut g = PlanGrid::new();
    g.add(&tri(20.0), Some(1), |_, _, _| true); // a tine high on a wall
    assert!(
        !g.hits(&tri(5.0), Some(2), &cfg),
        "a piece above the rib's top is no clash"
    );
    assert!(g.hits(&tri(20.0), Some(2), &cfg));
    // far away in plan
    let far: Vec<V4> = tri(5.0)
        .iter()
        .map(|v| [v[0] + 50.0, v[1], v[2], 0.0])
        .collect();
    assert!(!g.hits(&far, Some(2), &cfg));
    // `keep` leaves out what it rejects
    let mut g = PlanGrid::new();
    g.add(&tri(20.0), Some(1), |_, _, _| false);
    assert!(!g.hits(&tri(20.0), Some(2), &cfg));
}

fn empty() -> FinsBuilt {
    FinsBuilt::empty(Seating {
        kind: SeatingKind::Face,
        span: 0.0,
        bed_area: 0.0,
    })
}

fn wall(height: f64, line: Vec<[f64; 3]>) -> FinRecord {
    FinRecord {
        height,
        length: 1.0,
        tines: 0,
        rows: 0,
        stilt: 0.0,
        lean: 0.0,
        bearing: 0.0,
        site: None,
        id: 0,
        kind: FinKind::Prop,
        tri_ranges: vec![],
        line,
        span: 1.0,
        short: false,
        fill: false,
        joined: false,
        braces: None,
    }
}

#[test]
fn nothing_tall_means_zero_ribs_and_an_untouched_build() {
    let cfg = Config::default();
    let topo = build_topology(block(-10.0, 10.0, -10.0, 10.0, 0.0, 10.0));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let mut b = empty();
    b.fins
        .push(wall(50.0, vec![[0.0, 0.0, 50.0], [10.0, 0.0, 50.0]])); // under tallH (100)
    b.fins.push(FinRecord {
        kind: FinKind::Wedge,
        ..wall(500.0, vec![[0.0, 0.0, 500.0], [10.0, 0.0, 500.0]])
    }); // not a prop
    b.fins.push(FinRecord {
        braces: Some(3),
        ..wall(500.0, vec![[0.0, 0.0, 500.0], [10.0, 0.0, 500.0]])
    }); // already braced by the fill pass
    b.fins.push(wall(500.0, vec![[0.0, 0.0, 500.0]])); // a one-point line
    let before = b.clone();
    let ribs = brace_tall_walls(&topo, &res, &IDENTITY3, &mut b, &cfg);
    assert!(ribs.is_empty());
    assert_eq!(b.tall_ribs, Some(0));
    b.tall_ribs = None;
    assert_eq!(b, before);
}

#[test]
fn a_tall_wall_on_the_plate_gets_ribs_that_extend_its_ranges() {
    let cfg = Config::default();
    // a free-standing slab, so there is nothing for the ribs to clash with
    let topo = build_topology(block(80.0, 90.0, 80.0, 90.0, 0.0, 10.0));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let mut b = empty();
    // a wall 120 mm up, 40 mm long, its line the gap under a 120 mm contact
    let line: Vec<[f64; 3]> = (0..9).map(|i| [i as f64 * 5.0, 0.0, 119.8]).collect();
    let mut f = wall(120.0, line);
    f.tri_ranges = vec![[0, 0]];
    b.fins.push(f);
    let ribs = brace_tall_walls(&topo, &res, &IDENTITY3, &mut b, &cfg);
    assert_eq!(ribs.len(), 1);
    let (k, n) = ribs[0];
    assert_eq!((k, b.tall_ribs), (0, Some(n)));
    assert!(n >= 1 && !b.triangles.is_empty());
    assert_eq!(b.fins[0].tri_ranges.len(), 2);
    assert_eq!(b.fins[0].tri_ranges[1], [0, b.triangles.len()]);
}
