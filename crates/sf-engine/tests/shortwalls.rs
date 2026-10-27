//! Own tests of `sf_engine::fins::shortwalls` (web/fins/shortwalls.js): the early exits and the
//! overlap filter. (Parity with JS is `sf-testkit/tests/parity_shortwalls.rs`.)

use sf_engine::config::Config;
use sf_engine::fins::coverage::automatic_targets;
use sf_engine::fins::shortwalls::{ShortWalls, last_resort_walls};
use sf_engine::overhangs::analyze;
use sf_engine::prop::build::BuildOpts;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::types::V4;
use sf_testkit::{load_model, rot_x};

fn opts(excluded: Vec<u8>) -> BuildOpts {
    BuildOpts {
        tines: true,
        coverage: Some(0.5),
        excluded_support_faces: Some(std::sync::Arc::new(excluded)),
        ..Default::default()
    }
}

#[test]
fn nothing_bare_means_no_short_wall_and_support_everywhere_means_none_survive() {
    let cfg = Config::default();
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    let topo = load_model("lowledge");
    let rot = rot_x(40.0);
    let res = analyze(&topo, 45.0, &rot);
    let targets = automatic_targets(&topo, &res, &rot);
    let o = opts(targets.excluded_support_faces.clone());
    // every region already served: nothing bare to wall
    let all: Vec<usize> = (0..targets.analysis.regions.len()).collect();
    let r = last_resort_walls(&topo, &targets.analysis, &rot, &o, &all, &[], &[], &ctx);
    assert_eq!(r, ShortWalls::default());
    // nothing served: walls are built where no existing support is within sideClear...
    let bare = last_resort_walls(&topo, &targets.analysis, &rot, &o, &[], &[], &[], &ctx);
    // ...and a support box covering the whole plan drops them all (bounding boxes, conservative)
    let big = |z: f64| -> Vec<V4> {
        vec![
            [-1000.0, -1000.0, z, 0.0],
            [1000.0, -1000.0, z, 0.0],
            [0.0, 1000.0, z + 1000.0, 0.0],
        ]
    };
    let blocked = last_resort_walls(
        &topo,
        &targets.analysis,
        &rot,
        &o,
        &[],
        &[],
        &big(-5.0),
        &ctx,
    );
    assert!(blocked.props.is_empty() && blocked.triangles.is_empty() && blocked.served.is_empty());
    // what survives is consistent: ranges tile the kept triangles, and the served regions are theirs
    let mut covered = vec![0u8; bare.triangles.len()];
    for p in &bare.props {
        for [a, b] in &p.tri_ranges {
            for c in &mut covered[*a..*b] {
                *c += 1;
            }
        }
    }
    assert!(covered.iter().all(|&c| c == 1));
    assert_eq!(
        bare.tines,
        bare.props.iter().map(|p| p.tines).sum::<usize>()
    );
    for r in &bare.served {
        assert!(bare.props.iter().any(|p| p.region == Some(*r)));
    }
}
