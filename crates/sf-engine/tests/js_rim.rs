//! Port of upstream `tests/rim.test.js`: rim walls (prop/rim.js, local issue 062): a level overhang's
//! free outline gets a wall under it when the judge says it holds more.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::{Topology, analyze};
use sf_testkit::{block, load_model, rot_x};
use std::sync::Arc;

fn build(
    topo: &Arc<Topology>,
    rot: &sf_engine::types::Rot3,
    tweak: impl Fn(&mut FinsOpts),
) -> FinsBuilt {
    let mut o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        coverage: Some(0.5),
        ..Default::default()
    };
    tweak(&mut o);
    build_fins(topo, &analyze(topo, 45.0, rot), rot, &o, &Config::default())
}

// post y -10..0, ledge 28 wide (x) x 14 deep (y), underside z 25: main's rows stop 4 mm short of y 14
fn ledge() -> Arc<Topology> {
    let mut v = block(0.0, 28.0, -10.0, 0.0, 0.0, 27.0);
    v.extend(block(0.0, 28.0, 0.0, 14.0, 25.0, 27.0));
    topo_of(f32s(&v))
}

fn len(line: &[[f64; 3]]) -> f64 {
    line.windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum()
}

// rim.test.js:17
#[test]
fn rim_a_ledge_off_one_edge_gets_a_wall_under_its_free_edge_in_pull_out_pieces() {
    let b = build(&ledge(), &I, |_| {});
    let rims: Vec<_> = b.props.iter().filter(|q| q.rim_wall).collect();
    assert!(
        rims.iter().any(|q| q.line.iter().all(|p| p[1] > 10.0)),
        "no rim wall along the free y edge"
    );
    let piece_len = Config::default().rim.piece_len;
    for q in &rims {
        assert!(
            len(&q.line) <= piece_len + 1.0,
            "a rim piece {:.1} mm long",
            len(&q.line)
        );
    }
    // none along the attached edge (the post's side goes down from it)
    assert!(
        !rims.iter().any(|q| q.line.iter().all(|p| p[1] < 3.0)),
        "a rim wall along the attached edge"
    );
}

// :26
#[test]
fn rim_rim_walls_false_and_raster_false_build_no_rim() {
    let a = build(&ledge(), &I, |o| o.rim_walls = Some(false));
    assert!(
        !a.props.iter().any(|q| q.rim_wall),
        "{{ rimWalls: false }} built a rim"
    );
    let b = build(&ledge(), &I, |o| o.raster = Some(false));
    assert!(
        !b.props.iter().any(|q| q.rim_wall),
        "{{ raster: false }} built a rim"
    );
}

// :32
#[test]
fn rim_a_tilted_face_gets_no_rim_tube_x25_keeps_its_4_walls() {
    let topo = Arc::new(load_model("tube"));
    let b = build(&topo, &rot_x(25.0), |_| {});
    assert!(
        !b.props.iter().any(|q| q.rim_wall) && b.props.len() == 4,
        "{} walls",
        b.props.len()
    );
}

// :37
#[test]
fn rim_a_deck_laid_on_two_unwelded_posts_is_attached_there_no_rim_no_swap() {
    let mut v = block(-20.0, 20.0, 40.0, 44.0, 0.0, 17.0);
    v.extend(block(-20.0, 20.0, -44.0, -40.0, 0.0, 17.0));
    v.extend(block(-20.0, 20.0, -40.0, 40.0, 15.0, 17.0));
    let b = build(&topo_of(f32s(&v)), &I, |_| {});
    assert!(
        !b.props.iter().any(|q| q.rim_wall),
        "the deck took a rim: the posts read as free edges"
    );
}

// :43
#[test]
fn rim_a_round_flange_on_a_hub_its_ring_of_rim_pieces_leaves_a_gap_at_every_join_the_seam_too() {
    // a 40 mm disc 2 mm thick, underside z 20, on a 10 mm hub: the whole outline is free
    let (n, r) = (64usize, 20.0f64);
    let ring = |z: f64| -> Vec<[f64; 3]> {
        (0..n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                [r * a.cos(), r * a.sin(), z]
            })
            .collect()
    };
    let (lo, hi) = (ring(20.0), ring(22.0));
    let mut t: Vec<[f64; 3]> = vec![];
    for i in 0..n {
        let j = (i + 1) % n;
        t.extend([
            lo[i],
            hi[j],
            lo[j],
            lo[i],
            hi[i],
            hi[j],
            [0.0, 0.0, 20.0],
            lo[j],
            lo[i],
            [0.0, 0.0, 22.0],
            hi[i],
            hi[j],
        ]);
    }
    let mut flat: Vec<f64> = t.iter().flatten().copied().collect();
    flat.extend(block(-5.0, 5.0, -5.0, 5.0, 0.0, 20.5));
    let b = build(&topo_of(f32s(&flat)), &I, |_| {});
    let rims: Vec<_> = b.props.iter().filter(|q| q.rim_wall).collect();
    assert!(
        rims.len() >= 3,
        "{} rim pieces round a free 126 mm outline",
        rims.len()
    );
    let ends: Vec<[f64; 3]> = rims
        .iter()
        .flat_map(|q| [q.line[0], *q.line.last().unwrap()])
        .collect();
    let gap = Config::default().rim.piece_gap;
    for a in 0..ends.len() {
        for c in a + 1..ends.len() {
            if (a >> 1) == (c >> 1) {
                continue; // a piece's own two ends
            }
            let d = (ends[a][0] - ends[c][0]).hypot(ends[a][1] - ends[c][1]);
            assert!(d > 0.9 * gap, "two rim pieces {d:.2} mm apart");
        }
    }
}
