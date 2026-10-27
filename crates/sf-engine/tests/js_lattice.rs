//! Port of upstream `tests/lattice.test.js`: LATTICE NETS (issue #121, a headset's ring-and-strut
//! shell): struts that meet at nodes join into one big overhang region, which went to splitRegion
//! and was shattered into slivers -- the net got almost nothing while a lone strut of the same shape
//! got a wall. prop/lattice.js cuts the net back into struts and routes each as a lone strut would,
//! along its own length; the raster race keeps it only where it holds more. Pinned on a voxel strut
//! net (one shared surface, so the struts' undersides really are one region, as on the headset).
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::prop::lattice::lattice_struts;
use sf_engine::types::Rot3;
use sf_testkit::block;
use std::sync::Arc;

const V: f64 = 2.0; // mm per voxel

fn voxel_topo(filled: impl Fn(i64, i64, i64) -> bool, nx: i64, ny: i64, nz: i64) -> Arc<Topology> {
    let at = |i: i64, j: i64, k: i64| {
        i >= 0 && j >= 0 && k >= 0 && i < nx && j < ny && k < nz && filled(i, j, k)
    };
    let p = |i: i64, j: i64, k: i64| [i as f64 * V, j as f64 * V, k as f64 * V];
    let mut t: Vec<[f64; 3]> = Vec::new();
    let mut q = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| t.extend([a, b, c, a, c, d]);
    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                if !at(i, j, k) {
                    continue;
                }
                if !at(i, j, k - 1) {
                    q(
                        p(i, j, k),
                        p(i, j + 1, k),
                        p(i + 1, j + 1, k),
                        p(i + 1, j, k),
                    );
                }
                if !at(i, j, k + 1) {
                    q(
                        p(i, j, k + 1),
                        p(i + 1, j, k + 1),
                        p(i + 1, j + 1, k + 1),
                        p(i, j + 1, k + 1),
                    );
                }
                if !at(i - 1, j, k) {
                    q(
                        p(i, j, k),
                        p(i, j, k + 1),
                        p(i, j + 1, k + 1),
                        p(i, j + 1, k),
                    );
                }
                if !at(i + 1, j, k) {
                    q(
                        p(i + 1, j, k),
                        p(i + 1, j + 1, k),
                        p(i + 1, j + 1, k + 1),
                        p(i + 1, j, k + 1),
                    );
                }
                if !at(i, j - 1, k) {
                    q(
                        p(i, j, k),
                        p(i + 1, j, k),
                        p(i + 1, j, k + 1),
                        p(i, j, k + 1),
                    );
                }
                if !at(i, j + 1, k) {
                    q(
                        p(i, j + 1, k),
                        p(i, j + 1, k + 1),
                        p(i + 1, j + 1, k + 1),
                        p(i + 1, j + 1, k),
                    );
                }
            }
        }
    }
    let flat: Vec<f64> = t.into_iter().flatten().collect();
    topo_of(f32s(&flat))
}

// A 64 mm square net of 4 mm struts at a 20 mm pitch, 24 mm up on four corner posts.
const N: i64 = 32;
fn bar(u: i64) -> bool {
    u % 10 < 2
}
fn net() -> Arc<Topology> {
    voxel_topo(
        |i, j, k| {
            (k >= 12 && k < 14 && (bar(i) || bar(j)))
                || (k < 12 && bar(i) && bar(j) && (i < 2 || i >= 30) && (j < 2 || j >= 30))
        },
        N,
        N,
        14,
    )
}
fn rot_x_s(d: f64) -> Rot3 {
    let r = (d * std::f64::consts::PI) / 180.0;
    let (c, s) = (r.cos(), r.sin());
    [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
}

fn struts(topo: &Arc<Topology>, rot: &Rot3) -> Option<Vec<Vec<usize>>> {
    let res: Analysis = analyze(topo, 45.0, rot);
    let o = res.offset;
    let seated = |f: usize| -> [[f64; 3]; 3] {
        let mut out = [[0.0; 3]; 3];
        for (k, v) in out.iter_mut().enumerate() {
            let p = &topo.pos;
            let (x, y, z) = (p[f * 9 + k * 3], p[f * 9 + k * 3 + 1], p[f * 9 + k * 3 + 2]);
            *v = [
                rot[0] * x + rot[3] * y + rot[6] * z + o.x,
                rot[1] * x + rot[4] * y + rot[7] * z + o.y,
                rot[2] * x + rot[5] * y + rot[8] * z + o.z,
            ];
        }
        out
    };
    lattice_struts(topo, &res.regions[0].faces, &seated, &Config::default()).map(|s| s.groups)
}

// lattice.test.js:48
#[test]
fn lattice_a_strut_net_is_cut_into_struts_a_plate_and_a_lone_bar_are_not_nets() {
    let g = struts(&net(), &rot_x_s(20.0));
    assert!(
        g.as_ref().is_some_and(|g| g.len() >= 16),
        "the net should give ~20 struts: {:?}",
        g.map(|g| g.len())
    );
    let plate = topo_of(
        [
            block(0.0, 60.0, 0.0, 60.0, 20.0, 24.0),
            block(0.0, 4.0, 0.0, 4.0, 0.0, 20.0),
        ]
        .concat(),
    );
    assert!(
        struts(&plate, &rot_x_s(0.0)).is_none(),
        "a broad plate is not a net"
    );
    let bar1 = topo_of(
        [
            block(0.0, 60.0, 0.0, 4.0, 20.0, 24.0),
            block(0.0, 4.0, 0.0, 4.0, 0.0, 20.0),
        ]
        .concat(),
    );
    assert!(
        struts(&bar1, &rot_x_s(0.0)).is_none(),
        "a lone bar is not a net"
    );
}

// The net tilted 20 degrees about X: its struts along X now slope ACROSS their 4 mm width -- the
// headset's struts running around the dome.
// lattice.test.js:59
#[test]
fn lattice_a_tilted_net_gets_a_wall_along_each_strut_the_normal_pass_gives_almost_none() {
    let cfg = Config::default();
    let topo = net();
    let rot = rot_x_s(20.0);
    let res = analyze(&topo, 45.0, &rot);
    let go = |raster: bool| {
        build_fins(
            &topo,
            &res,
            &rot,
            &FinsOpts {
                mode: Some("auto".into()),
                bed_pad: Some(true),
                raster: Some(raster),
                ..Default::default()
            },
            &cfg,
        )
    };
    let (before, after) = (go(false), go(true));
    // the normal pass gets at most a couple (flush rows at the struts' free edges,
    // PROP.edgeInset); the raster pass is what gives every strut its wall
    assert!(
        before.props.len() <= 2,
        "normal pass changed: {} walls",
        before.props.len()
    );
    assert!(
        after.props.len() >= 16,
        "the struts should get walls: {}",
        after.props.len()
    );
    // every wall runs ALONG a strut (X or Y), never across one
    let dir = |q: &sf_engine::prop::build::PropRecord| {
        let (a, b) = (q.line[0], q.line[q.line.len() - 1]);
        if (b[0] - a[0]).abs() > (b[1] - a[1]).abs() {
            'x'
        } else {
            'y'
        }
    };
    for q in &after.props {
        let (a, b) = (q.line[0], q.line[q.line.len() - 1]);
        let ang = ((b[1] - a[1]).abs().atan2((b[0] - a[0]).abs()) * 180.0) / std::f64::consts::PI;
        assert!(
            ang < 10.0 || ang > 80.0,
            "a wall runs across a strut at {ang:.0}deg"
        );
    }
    // identical struts, the same support: both directions get their walls
    let nx = after.props.iter().filter(|q| dir(q) == 'x').count();
    let ny = after.props.len() - nx;
    assert!(
        nx >= 8 && ny >= 8,
        "struts along X got {nx} walls, along Y {ny}"
    );
    // no stubs from the cut: a plate wall keeps minSpan, one on the part minSpanPart (#116: one strut
    // wall here stands on the net, 6.98 mm)
    let least = |q: &sf_engine::prop::build::PropRecord| {
        if q.part_attached {
            cfg.prop.min_span_part
        } else {
            cfg.prop.min_span
        }
    };
    assert!(
        after.props.iter().all(|q| q.span >= least(q)),
        "strut walls keep minSpan / minSpanPart"
    );
}
