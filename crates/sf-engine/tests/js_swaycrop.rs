//! Port of upstream `tests/swaycrop.test.js`: Sway's cross ribs run on a crop of the part
//! (`swaycrop.rs`) instead of the whole thing. The crop may only drop triangles the rib can't see,
//! so the rib it builds, and the Sway brace around it, must be identical to the one built from the
//! whole part.

use sf_engine::config::Config;
use sf_engine::overhangs::{Topology, analyze};
use sf_engine::prop::brace::brace_cross_rib;
use sf_engine::sway::*;
use sf_engine::swaycrop::crop_for_rib;
use sf_engine::tris::Tris;
use sf_testkit::{block_topo, load_model, tilted_block_topo};

const ID: [f64; 9] = sf_engine::types::IDENTITY3;

/// `rng(seed)` (tests/swaycrop.test.js:13-15).
fn rng(mut seed: u64) -> impl FnMut() -> f64 {
    move || {
        seed = (seed * 1664525 + 1013904223) % (1u64 << 32);
        seed as f64 / 4294967296.0
    }
}

fn scenes() -> Vec<(&'static str, Topology)> {
    vec![
        ("lbracket", load_model("lbracket")),
        ("staircase", load_model("staircase")),
        ("tshape", load_model("tshape")),
        ("post", block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 150.0)),
        (
            "tilted",
            tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 100.0, 25.0),
        ),
    ]
}

// swaycrop.test.js:25 (one JS test per scene)
#[test]
fn cross_ribs_from_the_crop_equal_those_from_the_whole_part() {
    let cfg = Config::default();
    for (name, topo) in scenes() {
        let tris = Tris::new(topo.pos[..topo.n_faces * 9].to_vec());
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for t in tris.v.chunks(3) {
            for k in 0..2 {
                lo[k] = lo[k].min(t[k]);
                hi[k] = hi[k].max(t[k]);
            }
        }
        let mut rand = rng(name.len() as u64 * 31 + 5);
        let (mut built, mut blocked) = (0, 0);
        for n in 0..60 {
            let o = [
                lo[0] - 25.0 + rand() * (hi[0] - lo[0] + 50.0),
                lo[1] - 25.0 + rand() * (hi[1] - lo[1] + 50.0),
            ];
            let ang = rand() * std::f64::consts::PI * 2.0;
            let d = [ang.cos(), ang.sin()];
            let lean = [(rand() - 0.5) * 0.8, (rand() - 0.5) * 0.8];
            let height = 15.0 + rand() * 60.0;
            let d0 = 4.0 + rand() * 12.0;
            let th = 1.0 + rand() * 2.0;
            let run = |part: &Tris| {
                let mut out = vec![];
                let c = brace_cross_rib(
                    &mut out,
                    part,
                    o,
                    d,
                    0.0,
                    height,
                    d0,
                    lean,
                    |_| true,
                    0.5,
                    th,
                    &cfg,
                );
                (c, out)
            };
            // a few mm wider than the radius sway.js passes, so this tests the crop, not that radius
            let r = 1.5f64.max(d0) + 2.0 * 0.1 + 1.0 + th + 1.0 + 5.0;
            let want = run(&tris);
            let got = run(&crop_for_rib(&tris.v, o, r, lean, 0.0, height + 0.1));
            assert!(got == want, "{name} case {n}: crop changed the cross rib");
            if want.0.is_some() {
                built += 1;
            } else {
                blocked += 1;
            }
        }
        assert!(
            built > 5 && blocked > 0,
            "{name}: want both built ({built}) and blocked ({blocked}) ribs"
        );
    }
}

// swaycrop.test.js:52
#[test]
fn a_sway_click_on_a_post_still_gets_both_cross_arms_from_the_crop() {
    // end to end through sway.js's crop call
    let cfg = Config::default();
    let topo = block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 150.0);
    let res = analyze(&topo, 45.0, &ID);
    let f = (0..topo.n_faces)
        .find(|&i| face_is_upright(&topo, &ID, i, &cfg))
        .unwrap();
    let c: Vec<f64> = (0..3)
        .map(|k| (topo.pos[f * 9 + k] + topo.pos[f * 9 + 3 + k] + topo.pos[f * 9 + 6 + k]) / 3.0)
        .collect();
    let opts = SwayOpts {
        tines: Some(true),
        layer_height: Some(0.2),
        full: true,
        cross_above: Some(25.0),
        cross_rise: Some(0.5),
        ..Default::default()
    };
    let r = sway_at_face(
        &topo,
        &res,
        &ID,
        f,
        [
            c[0] + res.offset.x,
            c[1] + res.offset.y,
            c[2] + res.offset.z,
        ],
        &opts,
        &Avoid::default(),
        &cfg,
    );
    let rib = r.as_ref().expect("post brace");
    assert!(
        rib.cross == 2,
        "post brace should take both cross arms ({})",
        rib.cross
    );
}
