//! Port of upstream `tests/small_tubes.test.js`: SMALL TUBES (MorbidJ-hub, Sept 2026): a small
//! horizontal tube (a peg, a pin) got no support at all. A convex band now takes the tube route down
//! to tubeSmallMinArea, and its one wall may be as short as minSpanTube. Concave pockets stay out.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::build_fins;
use sf_engine::overhangs::{Analysis, Topology, analyze};
use std::sync::Arc;

fn quad(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Vec<[f64; 3]> {
    vec![a, b, c, a, c, d]
}
fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    [
        quad(v[0], v[3], v[2], v[1]),
        quad(v[4], v[5], v[6], v[7]),
        quad(v[0], v[1], v[5], v[4]),
        quad(v[2], v[3], v[7], v[6]),
        quad(v[1], v[2], v[6], v[5]),
        quad(v[0], v[4], v[7], v[3]),
    ]
    .concat()
}
/// closed cylinder along +Y from y0 to y1, axis at (cx, cz), radius r, 48 sides
fn cylinder_y(cx: f64, cz: f64, r: f64, y0: f64, y1: f64) -> Vec<[f64; 3]> {
    let n = 48usize;
    let ring = |y: f64| -> Vec<[f64; 3]> {
        (0..n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                [cx + r * a.cos(), y, cz + r * a.sin()]
            })
            .collect()
    };
    let (a, b) = (ring(y0), ring(y1));
    let mut t = vec![];
    for i in 0..n {
        let j = (i + 1) % n;
        t.extend([a[i], b[j], a[j], a[i], b[i], b[j]]); // side, outward
        t.extend([[cx, y0, cz], a[i], a[j]]); // cap at y0, facing -Y
        t.extend([[cx, y1, cz], b[j], b[i]]); // cap at y1, facing +Y
    }
    t
}
fn volume(t: &[[f64; 3]]) -> f64 {
    t.chunks(3)
        .map(|c| {
            let (a, b, c) = (c[0], c[1], c[2]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}
fn topo_from(tris: &[[f64; 3]]) -> Arc<Topology> {
    topo_of(f32s(&tris.iter().flatten().copied().collect::<Vec<_>>()))
}

// raster: false -- these tests pin the tube-vs-patch race inside the normal pass
fn build_with(topo: &Arc<Topology>, cfg: &Config) -> sf_engine::fins::build::FinsBuilt {
    let o = sf_engine::fins::build::FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        coverage: Some(0.5),
        raster: Some(false),
        ..Default::default()
    };
    build_fins(topo, &analyze(topo, 45.0, &I), &I, &o, cfg)
}

// A 30x20x24 block with a 5 mm peg sticking 8 mm out of its +Y face, 14 mm up: the chimney in
// miniature. The peg overlaps the block by 1 mm, and its underside band is ~25 mm2.
fn peg_part() -> Arc<Topology> {
    let mut t = block(-15.0, 15.0, -10.0, 10.0, 0.0, 24.0);
    t.extend(cylinder_y(0.0, 14.0, 2.5, 9.0, 18.0));
    topo_from(&t)
}

// small_tubes.test.js:51
#[test]
fn small_tube_the_test_peg_is_a_real_outward_facing_solid() {
    let v = volume(&cylinder_y(0.0, 14.0, 2.5, 9.0, 18.0));
    assert!(
        v > 0.95 * std::f64::consts::PI * 2.5 * 2.5 * 9.0,
        "peg volume {v}: wound inside out?"
    );
}

// :56
#[test]
fn small_tube_a_peg_sticking_out_sideways_gets_a_wall_under_it() {
    let topo = peg_part();
    let cfg = Config::default();
    let b = build_with(&topo, &cfg);
    let res: Analysis = analyze(&topo, 45.0, &I);
    let peg = res.regions.iter().position(|r| {
        r.faces.iter().any(|&f| {
            let y = topo.pos[f * 9 + 1] + topo.pos[f * 9 + 4] + topo.pos[f * 9 + 7];
            y / 3.0 > 10.5
        })
    });
    let peg = peg.expect("the peg underside should be an overhang region");
    assert!(
        res.regions[peg].area < cfg.prop.tube_min_area,
        "the peg must be a SMALL tube for this test"
    );
    assert!(
        b.served_regions.contains(&peg),
        "peg region left unserved (served: {:?})",
        b.served_regions
    );
    // and the wall under it really is under it: some support vertex beyond the block's face, below the peg
    let under = b
        .triangles
        .iter()
        .filter(|p| p[1] > 10.8 && p[1] < 17.5 && p[2] < 11.0)
        .count();
    assert!(under > 0, "no support geometry stands under the peg");
}

// Overhang area held by `sup` (support triangles), by the sweep's coverage rule.
fn held(topo: &Topology, res: &Analysis, sup: &[[f64; 3]], span: f64) -> f64 {
    let (mut tot, mut got) = (0.0, 0.0);
    let off = [res.offset.x, res.offset.y, res.offset.z];
    for g in &res.regions {
        for &f in &g.faces {
            let c: Vec<f64> = (0..3)
                .map(|k| {
                    (topo.pos[f * 9 + k] + topo.pos[f * 9 + 3 + k] + topo.pos[f * 9 + 6 + k]) / 3.0
                        + off[k]
                })
                .collect();
            tot += topo.area[f];
            if sup.iter().any(|v| {
                v[2] > c[2] - 3.0 && v[2] < c[2] + 0.5 && (v[0] - c[0]).hypot(v[1] - c[1]) <= span
            }) {
                got += topo.area[f];
            }
        }
    }
    got / tot
}

// :95
#[test]
fn small_tube_a_failed_tube_hands_its_region_back_to_the_patch_path() {
    // A longer, fatter peg: its underside is a small convex band that takes the tube route. When that
    // one wall fails, the region must go back to the patch path, as if the tube had never claimed it.
    let mut t = block(-25.0, 25.0, -10.0, 10.0, 0.0, 60.0);
    t.extend(cylinder_y(0.0, 30.0, 8.0, 9.0, 23.0));
    let topo = topo_from(&t);
    let res = analyze(&topo, 45.0, &I);
    let default = Config::default();
    let mut off = default.clone();
    off.prop.tube_small_min_area = f64::INFINITY; // small tubes off: the old patch path
    let patch_only = build_with(&topo, &off);
    assert!(
        build_with(&topo, &default).props.len() > patch_only.props.len(),
        "the peg should take the tube route"
    );
    let mut failing = default.clone();
    failing.prop.min_span_tube = 1000.0; // the tube's one wall now always fails
    let failed = build_with(&topo, &failing);
    let span = default.prop.max_unsupported_span;
    let tri = |b: &sf_engine::fins::build::FinsBuilt| xyz(&b.triangles);
    assert!(
        held(&topo, &res, &tri(&failed), span) >= held(&topo, &res, &tri(&patch_only), span) - 1e-9,
        "a failed small tube must hold no less than the patch path"
    );
    assert!(
        format!("{:?}", failed.skipped) == format!("{:?}", patch_only.skipped),
        "the patch path must run as before: {:?} vs {:?}",
        failed.skipped,
        patch_only.skipped
    );
}
