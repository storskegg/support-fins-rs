//! The lazily built grids (the inside-test bucket grid, the surface z-grid, the judge's part anchors)
//! are filled on first use, and on a big mesh the fill itself runs on Rayon (`csr::build_csr`). When
//! the first use is on a Rayon worker, that worker waits for its own sub-jobs by stealing other
//! queued jobs, which may need the very same grid: they block on a `OnceLock` the worker's own stack
//! holds, and the pool is dead (a hung `suggest_orientations` on a 772k-triangle part).
//!
//! This test fans many tasks out over a mesh above `build_csr`'s parallel threshold, every task
//! calling all three getters on the same, freshly made caches, again and again. A watchdog turns a
//! deadlock into a failure instead of a hung run. It lives in its own test binary so a dead pool
//! cannot take another test with it.

use rayon::prelude::*;
use sf_engine::inside::inside_part;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::judge::{JudgeCache, part_anchors};
use sf_engine::prop::surface::surface_z_at;
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, Xyz};
use std::sync::mpsc;
use std::time::Duration;

/// A UV sphere of radius 20 centred at the origin, `lon * (2 * lat - 2)` triangles, as a flat
/// triangle array (9 floats a face).
fn sphere(lat: usize, lon: usize) -> Vec<f64> {
    let p = |i: usize, j: usize| -> [f64; 3] {
        let th = std::f64::consts::PI * i as f64 / lat as f64;
        let ph = 2.0 * std::f64::consts::PI * (j % lon) as f64 / lon as f64;
        [
            20.0 * th.sin() * ph.cos(),
            20.0 * th.sin() * ph.sin(),
            20.0 * th.cos(),
        ]
    };
    let mut v = Vec::new();
    let mut tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        for q in [a, b, c] {
            v.extend_from_slice(&q);
        }
    };
    for i in 0..lat {
        for j in 0..lon {
            let (a, b, c, d) = (p(i, j), p(i, j + 1), p(i + 1, j + 1), p(i + 1, j));
            if i > 0 {
                tri(a, d, b);
            }
            if i + 1 < lat {
                tri(b, d, c);
            }
        }
    }
    v
}

/// One round: `outer` jobs (suggest's finalists), each fanning `inner` probes out over the job's own
/// surface and judge caches and the shared topology. Few outer jobs leave workers idle, and an idle
/// worker is what steals half of a grid fill from the worker running it.
fn round(mesh: &[f64], outer: usize, inner: usize, round: usize) -> f64 {
    let off = Xyz {
        x: 0.0,
        y: 0.0,
        z: 20.0,
    };
    let topo = build_topology(mesh.to_vec());
    let result = analyze(&topo, 45.0, &IDENTITY3);
    let seated: Vec<f64> = mesh
        .iter()
        .enumerate()
        .map(|(i, &x)| if i % 3 == 2 { x + 20.0 } else { x })
        .collect();
    (0..outer)
        .into_par_iter()
        .map(|o| {
            let tris = Tris::new(seated.clone());
            let cache = JudgeCache::default();
            (0..inner)
                .into_par_iter()
                .map(|t| {
                    let x = (t as f64 / inner as f64 - 0.5) * 30.0;
                    match (t + o + round) % 3 {
                        0 => surface_z_at(&tris, x, 1.0).unwrap_or(0.0),
                        1 => f64::from(inside_part(&topo, &IDENTITY3, &off, x, 1.0, 25.0)),
                        _ => part_anchors(&topo, &result, &IDENTITY3, &cache).len() as f64,
                    }
                })
                .sum::<f64>()
        })
        .sum()
}

#[test]
fn tasks_first_using_the_lazy_grids_on_workers_do_not_deadlock() {
    const ROUNDS: usize = 20;
    let mesh = sphere(150, 200);
    assert!(mesh.len() / 9 >= 50_000, "above the parallel threshold");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut sum = 0.0f64;
        for r in 0..ROUNDS {
            for (outer, inner) in [(1, 64), (2, 64), (4, 64), (8, 32), (256, 3)] {
                sum += round(&mesh, outer, inner, r);
            }
        }
        let _ = tx.send(sum);
    });
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(sum) => assert!(sum.is_finite()),
        Err(e) => panic!("deadlock: the lazy grids were not built within 60 s ({e})"),
    }
}
