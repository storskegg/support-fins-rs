//! Port of upstream `tests/kiss.test.js`: kissEnds (web/kiss.js). A tine's end stops at the FIRST
//! solid along its heading. It used to probe once at 2x the reach and bisect back from there: a
//! part thinner than that put the probe in air past it (the full reach, run through the part), and
//! a second piece beyond a thin near one pulled the end out through the near one.
//!
//! The fourth case builds a wedge with `buildPerpFins` (fins/wedges.js, track A).

use sf_engine::config::Config;
use sf_engine::fins::wedges::{PerpConfig, WedgeOpts, build_perp_fins};
use sf_engine::inside::inside_part;
use sf_engine::kiss::{KissEnds, KissOpts, kiss_ends};
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::planes::find_wall_patches;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_testkit::{assert_close, block, load_model, rot_x, tine_boxes};

const O: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

fn topo_of(boxes: &[[f64; 6]]) -> Topology {
    let mut pos = Vec::new();
    for b in boxes {
        pos.extend(block(b[0], b[1], b[2], b[3], b[4], b[5]));
    }
    build_topology(pos)
}

// a 0.6-layer tine at the origin biting +Y: reach 1.5, so the old far probe sat at 3.0
// (`contactOffset` is left out in the JS: its default is -0.01, PROP.tineContactOffset)
fn ends(topo: &Topology) -> KissEnds {
    kiss_ends(
        topo,
        &IDENTITY3,
        &O,
        0.0,
        0.0,
        0.0,
        1.0,
        4.6,
        5.2,
        &KissOpts {
            half: 0.25,
            back: 0.3,
            reach: 1.5,
            contact_offset: -0.01,
        },
    )
}

fn all(e: &KissEnds) -> [f64; 4] {
    [e.bot[0], e.bot[1], e.top[0], e.top[1]]
}

// kiss.test.js:23
#[test]
fn kiss_ends_a_part_thinner_than_the_far_probe_still_stops_the_tine_at_its_face() {
    // solid from y 0.5 to 1.3: both the old 3.0 and 1.5 probes land in air past it
    for d in all(&ends(&topo_of(&[[-5.0, 5.0, 0.5, 1.3, 0.0, 10.0]]))) {
        assert_close(d, 0.51, 1e-3, "end");
    }
}

// kiss.test.js:28
#[test]
fn kiss_ends_a_thin_near_piece_stops_the_tine_not_a_block_beyond_it() {
    // a 0.2 slab at y 0.5, then a block from y 2: the old far probe found the block
    let t = topo_of(&[
        [-5.0, 5.0, 0.5, 0.7, 0.0, 10.0],
        [-5.0, 5.0, 2.0, 10.0, 0.0, 10.0],
    ]);
    for d in all(&ends(&t)) {
        assert_close(d, 0.51, 1e-3, "end");
    }
}

// kiss.test.js:33
#[test]
fn kiss_ends_no_solid_within_reach_keeps_the_full_reach_solid_behind_the_seed_pulls_back() {
    for d in all(&ends(&topo_of(&[[-5.0, 5.0, 4.0, 10.0, 0.0, 10.0]]))) {
        assert_close(d, 1.5, 1e-9, "bare");
    }
    for d in all(&ends(&topo_of(&[[-5.0, 5.0, -1.0, 10.0, 0.0, 10.0]]))) {
        assert_close(d, -0.25, 1e-9, "behind");
    }
}

// kiss.test.js:40 -- a wedge on a thin part: its tines' top corners stay outside the part at the
// tine's mid-layer (the tine ends are kissed to the part, even at 0.6 layers).
#[test]
fn kiss_ends_a_wedge_on_a_thin_part_keeps_its_tines_out_of_it_at_0_6_layers() {
    let topo = load_model("wedge");
    let rot = rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let perp = PerpConfig::default();
    let p = find_wall_patches(&topo, &rot, &res.offset, None, None)
        .into_iter()
        .find(|q| q.n.z < -0.05 && q.area >= perp.min_area && (q.u1 - q.u0) >= perp.min_width)
        .expect("a wedge patch");
    let on = build_perp_fins(
        &p,
        &topo,
        &rot,
        &res.offset,
        &WedgeOpts {
            tines: Some(true),
            layer_height: Some(0.6),
            ..Default::default()
        },
        &ctx,
    );
    let off = build_perp_fins(
        &p,
        &topo,
        &rot,
        &res.offset,
        &WedgeOpts {
            tines: Some(false),
            ..Default::default()
        },
        &ctx,
    );
    let xyz = |v: &[[f64; 4]]| -> Vec<[f64; 3]> { v.iter().map(|q| [q[0], q[1], q[2]]).collect() };
    let mut n = 0;
    for (i, w) in on.wedges.iter().enumerate() {
        let pre = off.wedges[i].tri_range[1] - off.wedges[i].tri_range[0];
        let tris = xyz(&on.triangles[w.tri_range[0] + pre..w.tri_range[1]]);
        for t in tine_boxes(&tris).tines {
            n += 1;
            let mid = (t.lo[2] + t.hi[2]) / 2.0;
            let cx = (t.lo[0] + t.hi[0]) / 2.0;
            let cy = (t.lo[1] + t.hi[1]) / 2.0;
            for v in t.verts.iter().filter(|q| q[2] > t.hi[2] - 1e-6) {
                // 0.02 back toward the tine's centre: the end overlaps the part by 0.01 on purpose
                let dx = cx - v[0];
                let dy = cy - v[1];
                let d = {
                    let h = dx.hypot(dy);
                    if h == 0.0 { 1.0 } else { h }
                };
                assert!(
                    !inside_part(
                        &topo,
                        &rot,
                        &res.offset,
                        v[0] + (dx / d) * 0.02,
                        v[1] + (dy / d) * 0.02,
                        mid
                    ),
                    "a tine top corner at x {:.2} is inside the part at its mid-layer",
                    v[0]
                );
            }
        }
    }
    assert!(n >= 20, "wedge X45 at 0.6 built {n} wedge tines");
}
