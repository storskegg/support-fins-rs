//! Port of upstream `tests/downhill.test.js`: downhill walls (issue #25): walls that run DOWN an
//! overhang and follow it to the plate (web/prop/downhill.js), raced per region against the other
//! placements (prop/raster.js raceAgainst) and kept only when the whole build is no worse for them
//! (fins/downhill.js judgeDownhill). (`globalThis.__TINECAP` is `build_fins_capturing`'s second
//! value; it lives in sf-io's tests for `read_stl`.)
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins_capturing};
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::prop::downhill::{DownhillOpts, downhill_lines};
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, Rot3};
use sf_io::stl::read_stl;
use sf_testkit::upstream_root;

fn example(name: &str) -> Arc<Topology> {
    let bytes = std::fs::read(upstream_root().join(format!("tests/fixtures/{name}.stl"))).unwrap();
    Arc::new(build_topology(read_stl(&bytes).unwrap()))
}

/// Overhang area within reach of a wall top: prototype/examples/probe.js's rule.
fn held_frac(topo: &Arc<Topology>, rot: &Rot3, b: &FinsBuilt) -> f64 {
    let res = analyze(topo, 45.0, rot);
    let (o, pos) = (res.offset, &topo.pos);
    let tops: Vec<[f64; 3]> = b.fins.iter().flat_map(|q| q.line.iter().copied()).collect();
    let r = Config::default().prop.max_unsupported_span / 2.0;
    let (mut area, mut held) = (0.0, 0.0);
    for g in &res.regions {
        for &f in &g.faces {
            let mut c = [0.0; 3];
            for i in 0..3 {
                let j = f * 9 + i * 3;
                let (x, y, z) = (pos[j], pos[j + 1], pos[j + 2]);
                c[0] += (rot[0] * x + rot[3] * y + rot[6] * z + o.x) / 3.0;
                c[1] += (rot[1] * x + rot[4] * y + rot[7] * z + o.y) / 3.0;
                c[2] += (rot[2] * x + rot[5] * y + rot[8] * z + o.z) / 3.0;
            }
            area += topo.area[f];
            if tops.iter().any(|p| {
                let dz = c[2] - p[2];
                (-0.05..=1.5).contains(&dz) && (c[0] - p[0]).hypot(c[1] - p[1]) <= r
            }) {
                held += topo.area[f];
            }
        }
    }
    held / area
}

fn build(topo: &Arc<Topology>, f: impl FnOnce(&mut FinsOpts)) -> (FinsBuilt, f64) {
    let mut o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        ..Default::default()
    };
    f(&mut o);
    let (b, caps) = build_fins_capturing(
        topo,
        &analyze(topo, 45.0, &IDENTITY3),
        &IDENTITY3,
        &o,
        &Config::default(),
    );
    (
        b,
        caps.iter()
            .fold(f64::INFINITY, |l, t| if t.z < l { t.z } else { l }),
    )
}

// downhill.test.js:46
#[test]
fn downhill_the_25_part_gets_its_curved_end_held_90_97_never_less_grip() {
    // A long strip of overhang that slopes down to the plate across its width, its end bending
    // round a notch: the rows left the bend bare (90% held on main).
    let topo = example("issue25_curve");
    let (off, off_low) = build(&topo, |o| o.downhill = Some(false));
    let (on, on_low) = build(&topo, |_| {});
    let (h_off, h_on) = (
        held_frac(&topo, &IDENTITY3, &off),
        held_frac(&topo, &IDENTITY3, &on),
    );
    assert!(
        h_off < 0.92,
        "downhill:false should be main's 90%, got {:.1}%",
        100.0 * h_off
    );
    assert!(
        on.props.iter().any(|q| q.downhill == Some(true)),
        "no region swapped to downhill walls"
    );
    assert!(h_on >= 0.97, "downhill held only {:.1}%", 100.0 * h_on);
    assert!(
        on_low <= off_low + 0.1,
        "lowest tine rose {off_low} -> {on_low}"
    );
}

// downhill.test.js:58
#[test]
fn downhill_a_plain_slope_gets_straight_lines_down_it_one_at_each_end_of_its_top_edge() {
    // A 40 x 30 mm plane, 45deg-ish, sloping down along +x: z = 30 - 0.9 x.
    let z = |x: f64| 30.0 - 0.9 * x;
    let p = |x: f64, y: f64| [x, y, z(x)];
    let tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| [a, b, c].concat();
    let tris = [
        tri(p(0.0, 0.0), p(40.0, 0.0), p(40.0, 30.0)),
        tri(p(0.0, 0.0), p(40.0, 30.0), p(0.0, 30.0)),
    ]
    .concat();
    let pts = [p(0.0, 0.0), p(40.0, 0.0), p(40.0, 30.0), p(0.0, 30.0)];
    let cfg = Config::default();
    let lines = downhill_lines(
        &pts,
        &Tris::new(tris),
        cfg.prop.station_step,
        cfg.prop.max_unsupported_span,
        &DownhillOpts::default(),
        &cfg,
    );
    let lines = lines
        .filter(|l| l.len() >= 3)
        .expect("expected >= 3 lines across 30 mm");
    let mut ys: Vec<f64> = lines.iter().map(|l| l[0][1]).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        ys[0] < 2.0 && *ys.last().unwrap() > 28.0,
        "end lines at y {:.1} and {:.1}, not the edge's ends",
        ys[0],
        ys.last().unwrap()
    );
    for i in 1..ys.len() {
        assert!(
            ys[i] - ys[i - 1] <= cfg.prop.max_unsupported_span + 1e-6,
            "lines wider apart than the span cap"
        );
    }
    for l in &lines {
        for k in 1..l.len() {
            assert!(l[k][2] < l[k - 1][2], "a line climbed");
            assert!(
                (l[k][1] - l[0][1]).abs() < 1e-6,
                "a line on a plane bent sideways"
            );
        }
    }
}

// downhill.test.js:78
#[test]
fn downhill_a_level_ceiling_has_no_downhill_to_follow() {
    let tris = vec![
        0.0, 0.0, 10.0, 20.0, 0.0, 10.0, 20.0, 20.0, 10.0, 0.0, 0.0, 10.0, 20.0, 20.0, 10.0, 0.0,
        20.0, 10.0,
    ];
    let pts = [
        [0.0, 0.0, 10.0],
        [20.0, 0.0, 10.0],
        [20.0, 20.0, 10.0],
        [0.0, 20.0, 10.0],
    ];
    let cfg = Config::default();
    let r = downhill_lines(
        &pts,
        &Tris::new(tris),
        cfg.prop.station_step,
        cfg.prop.max_unsupported_span,
        &DownhillOpts::default(),
        &cfg,
    );
    assert!(r.is_none(), "a flat ceiling got downhill lines");
}

// downhill.test.js:84
#[test]
fn downhill_downhill_false_places_no_downhill_wall() {
    // the judge's fallback build
    let topo = example("issue25_curve");
    let (b, _) = build(&topo, |o| o.downhill = Some(false));
    assert!(
        !b.props.iter().any(|q| q.downhill == Some(true)),
        "downhill:false placed a downhill wall"
    );
}
