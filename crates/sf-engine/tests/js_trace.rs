//! Port of upstream `tests/trace.test.js`: curved walls on a jagged overhang (web/prop/trace.js).
//! A rippled slope near the overhang angle breaks its red into strips, and a downhill line traced
//! on one strip ends at its edge (coffee bean tilted back: 0 curves). Grown across the slivers,
//! the strips trace as one surface and the lines cover the red themselves.

use indexmap::IndexSet;
use sf_engine::config::Config;
use sf_engine::jsnum;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_engine::prop::downhill::{DownhillOpts, downhill_lines};
use sf_engine::prop::trace::trace_groups;
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;

/// A down-facing slope along x: 3 mm strips at 40deg (red at a 45deg threshold) and 50deg (not
/// red), alternating, 42 x 24 mm, its high end 30 mm up (trace.test.js:14-26).
fn rippled() -> Topology {
    let (w, d) = (42usize, 24usize);
    let mut z = vec![30.0f64];
    for x in 1..=w {
        let deg = if ((x - 1) / 3) % 2 == 1 { 50.0 } else { 40.0 };
        z.push(z[x - 1] - jsnum::tan((deg * std::f64::consts::PI) / 180.0));
    }
    let mut tris: Vec<f64> = Vec::new();
    for x in 0..w {
        for y in 0..d {
            let (xf, yf) = (x as f64, y as f64);
            let a = [xf, yf, z[x]];
            let b = [xf, yf + 1.0, z[x]];
            let c = [xf + 1.0, yf, z[x + 1]];
            let dd = [xf + 1.0, yf + 1.0, z[x + 1]];
            // wound to face down
            for v in [a, b, c, b, dd, c] {
                tris.extend(v);
            }
        }
    }
    // Float32Array.from(tris)
    build_topology(tris.into_iter().map(|v| (v as f32) as f64).collect())
}

// trace.test.js:29
#[test]
fn trace_a_rippled_slope_traces_as_one_surface_its_lines_run_across_the_strips_and_cover_the_red() {
    let cfg = Config::default();
    let topo = rippled();
    let res = analyze(&topo, 45.0, &IDENTITY3);
    assert!(
        res.regions.len() >= 5,
        "the red should break into strips, got {} regions",
        res.regions.len()
    );
    let all: IndexSet<usize> = (0..res.regions.len()).collect();
    let tg = trace_groups(&topo, &res, None, &IDENTITY3, &res.offset, &all, &cfg);

    // the strips' own faces: no line runs past a strip (3 mm downhill)
    for &ri in &all {
        let mut tris: Vec<f64> = Vec::new();
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let (pos, off) = (&topo.pos, &res.offset);
        for &f in &res.regions[ri].faces {
            for i in 0..3 {
                let p = [
                    pos[f * 9 + i * 3] + off.x,
                    pos[f * 9 + i * 3 + 1] + off.y,
                    pos[f * 9 + i * 3 + 2] + off.z,
                ];
                tris.extend(p);
                pts.push(p);
            }
        }
        let own = downhill_lines(
            &pts,
            &Tris::new(tris),
            cfg.prop.station_step,
            cfg.prop.max_unsupported_span,
            &DownhillOpts::default(),
            &cfg,
        )
        .unwrap_or_default();
        assert!(
            own.iter().all(|l| l.len() <= 5),
            "strip {ri}: a line ran {} mm on its own faces",
            own.iter().map(|l| l.len()).max().unwrap_or(0)
        );
    }

    assert_eq!(
        tg.groups.len(),
        1,
        "the strips should join into one surface, got {}",
        tg.groups.len()
    );
    let (leader, g) = tg.groups.iter().next().unwrap();
    assert!(
        g.members.len() == res.regions.len() && tg.leader_of.values().all(|l| l == leader),
        "every strip should race with the one leader"
    );
    let lines = downhill_lines(
        &g.pts,
        &g.tris,
        cfg.prop.station_step,
        cfg.prop.max_unsupported_span,
        &DownhillOpts {
            h: Some(cfg.trace.h),
            cover: Some(&g.red),
            reach: None,
        },
        &cfg,
    );
    let lines = lines.filter(|l| l.iter().map(|x| x.len()).max().unwrap_or(0) >= 30);
    let lines = lines.unwrap_or_else(|| panic!("the lines should cross the strips"));
    let reach = cfg.prop.max_unsupported_span / 2.0;
    let bare = g
        .red
        .iter()
        .filter(|p| {
            !lines.iter().any(|l| {
                l.iter()
                    .any(|q| jsnum::hypot(&[q[0] - p[0], q[1] - p[1]]) <= reach)
            })
        })
        .count();
    assert_eq!(
        bare,
        0,
        "{bare} of {} red faces beyond every line's reach",
        g.red.len()
    );
}
