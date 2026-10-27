//! Port of upstream `tests/short_walls.test.js`: SHORT WALLS (issue #121, a headset's ring-and-strut
//! lattice): every strut underside was 4-6 mm long, under minSpan, so each one was dropped as a stub
//! and half the red printed into air. A wall down to minSpanShort is now kept while it stays stocky
//! -- height at most maxShortAspect x its length. A tall short wall is a toothpick and is still
//! refused.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::FinsOpts;
use sf_engine::overhangs::Topology;
use sf_engine::prop::build::PropRecord;
use sf_engine::prop::clearance::min_span_for;
use sf_testkit::block;
use std::sync::Arc;

// a flat run at surface height z (the wall top sits `gap` under it), stations 1 mm apart
fn run(len: usize, z: f64) -> Vec<[f64; 3]> {
    (0..=len).map(|i| [i as f64, 0.0, z]).collect()
}

// short_walls.test.js:20
#[test]
fn short_walls_min_span_for_relaxes_to_min_span_short_only_for_a_low_wall() {
    let cfg = Config::default();
    let p = &cfg.prop;
    let need = |len, z| {
        let r = run(len, z);
        min_span_for(&r, &vec![true; r.len()], false, true, &cfg)
    };
    let r0 = run(5, 10.0);
    assert!(
        min_span_for(&r0, &vec![true; r0.len()], false, false, &cfg) == p.min_span,
        "outside the last resort minSpan holds"
    );
    assert!(
        need(5, 10.0) == p.min_span_short,
        "a 10 mm wall needs {}",
        need(5, 10.0)
    );
    let h = 30.0 - p.gap;
    assert!(
        (need(5, 30.0) - h / p.max_short_aspect).abs() < 1e-9,
        "a 30 mm wall scales with height: {}",
        need(5, 30.0)
    );
    assert!(
        need(5, 100.0) == p.min_span,
        "a 100 mm wall keeps minSpan: {}",
        need(5, 100.0)
    );
    // the TALLEST station decides: one high end makes the whole run tall
    let mut r = run(5, 10.0);
    r[5][2] = 100.0;
    let all = vec![true; r.len()];
    assert!(
        min_span_for(&r, &all, false, true, &cfg) == p.min_span,
        "the tallest station is what tips"
    );
    assert!(
        min_span_for(&r, &all, true, true, &cfg) == p.min_span_tube,
        "a small tube keeps its own floor"
    );
}

fn topo_parts(parts: &[Vec<f64>]) -> Arc<Topology> {
    topo_of(parts.iter().flatten().copied().collect())
}

// A column with a short strut sticking out along +X at height z: the strut's underside is 6 x 4 mm,
// one wall long, under minSpan, and nothing else reaches it.
fn strut(z: f64) -> Arc<Topology> {
    topo_parts(&[
        block(-4.0, 4.0, -4.0, 4.0, 0.0, z + 4.0),
        block(4.0, 10.0, -2.0, 2.0, z, z + 3.0),
    ])
}

fn walls(topo: &Arc<Topology>, last_resort: bool) -> Vec<PropRecord> {
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        last_resort: Some(last_resort),
        ..Default::default()
    };
    let (_, b) = build(topo, &I, &opts);
    // walls under the strut, not the column
    b.props
        .into_iter()
        .filter(|p| p.line.iter().any(|q| q[0] > 4.5))
        .collect()
}

// short_walls.test.js:43
#[test]
fn short_walls_a_low_short_strut_gets_a_wall() {
    let cfg = Config::default();
    let w = walls(&strut(12.0), true);
    assert!(!w.is_empty(), "a 6 mm strut 12 mm up should get a wall");
    // `short: true` is on the fin records `buildFins` makes from them: only the last resort builds it
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        ..Default::default()
    };
    let (_, b) = build(&strut(12.0), &I, &opts);
    assert!(
        b.fins
            .iter()
            .filter(|f| f.line.iter().any(|q| q[0] > 4.5))
            .all(|f| f.short),
        "only the last resort builds it"
    );
    assert!(
        walls(&strut(12.0), false).is_empty(),
        "without the last resort it is a stub"
    );
    assert!(
        w.iter()
            .all(|p| p.height <= cfg.prop.max_short_aspect * p.span + 1e-6),
        "no wall past the aspect cap"
    );
}

// short_walls.test.js:51
#[test]
fn short_walls_props_ranges_index_the_merged_triangles_compare_js_reads_them() {
    // plus a long ledge on -X that the normal pass walls, so the short wall's triangles sit AFTER
    // other walls' in the merged array
    let topo = topo_parts(&[
        block(-4.0, 4.0, -4.0, 4.0, 0.0, 16.0),
        block(4.0, 10.0, -2.0, 2.0, 12.0, 15.0),
        block(-24.0, -4.0, -6.0, 6.0, 12.0, 15.0),
    ]);
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        ..Default::default()
    };
    let (_, b) = build(&topo, &I, &opts);
    let short: Vec<_> = b.fins.iter().filter(|f| f.short).collect();
    assert!(
        !short.is_empty() && b.props.len() > short.len(),
        "a normal wall and a short wall"
    );
    for f in short {
        // the prop with the same line
        let p = b
            .props
            .iter()
            .find(|p| p.line == f.line)
            .expect("the prop of this fin");
        assert!(f.tri_ranges == p.tri_ranges, "prop and fin ranges agree");
        let xs: Vec<f64> = p.line.iter().map(|q| q[0]).collect();
        let lo = xs.iter().cloned().fold(f64::INFINITY, f64::min) - 3.0;
        let hi = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) + 3.0;
        for [a, e] in &p.tri_ranges {
            for i in *a..*e {
                assert!(
                    b.triangles[i][0] >= lo && b.triangles[i][0] <= hi,
                    "vertex {i} is off its wall: x {}",
                    b.triangles[i][0]
                );
            }
        }
    }
}

// short_walls.test.js:67
#[test]
fn short_walls_a_tall_short_strut_is_still_refused_no_toothpicks() {
    assert!(
        walls(&strut(80.0), true).is_empty(),
        "a 6 mm wall 80 mm tall must not be built"
    );
}
