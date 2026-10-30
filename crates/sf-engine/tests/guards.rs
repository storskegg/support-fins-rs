//! Engine guards (fix round 2D, audit findings S-05 and S-06): values that pass the loader -- a "load anyway"
//! part metres across, one stray vertex at 1e16 -- must cost bounded time and memory, never hang, never abort.
//! (S-03, the sort comparators, is tested beside the helper: `guards::tests`.)

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sf_engine::config::Config;
use sf_engine::footprint::{
    CELL_BUDGET, area, dilate, find_spot, placed, rasterize, shared, union,
};
use sf_engine::orient::suggest_orientations_cancellable;
use sf_engine::overhangs::build_topology;
use sf_testkit::block;

/// A flat box's two top triangles, x0..x1 by y0..y1.
fn flat(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<f64> {
    vec![
        x0, y0, 1.0, x1, y0, 1.0, x1, y1, 1.0, x0, y0, 1.0, x1, y1, 1.0, x0, y1, 1.0,
    ]
}

/// Run `f` on a thread and fail (rather than hang the suite) when it takes over `limit`.
fn within<T: Send + 'static>(
    limit: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> (T, Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let t = Instant::now();
        let v = f();
        let _ = tx.send((v, t.elapsed()));
    });
    rx.recv_timeout(limit)
        .unwrap_or_else(|_| panic!("still running after {limit:?}"))
}

/// A 20 mm cube plus one down-facing stray triangle at (1e16, 1e16): the audit's S-06 input.
fn cube_with_stray(at: f64) -> Vec<f64> {
    let mut soup = block(0.0, 20.0, 0.0, 20.0, 0.0, 20.0);
    // the audit's triangle, 10 mm up, as the loader's f32 holds it (at 1e16 the 5 mm legs vanish)
    soup.extend(
        [at, at, 10.0, at, at + 5.0, 10.0, at + 5.0, at, 10.0].map(|v: f64| v as f32 as f64),
    );
    soup
}

// S-06: before the fix `rank_poses` was "STILL RUNNING after 60 s" (`v += 0.25` stops changing `v`).
#[test]
fn a_stray_vertex_at_1e16_ranks_within_two_seconds() {
    let topo = Arc::new(build_topology(cube_with_stray(1e16)));
    let (ranked, took) = within(Duration::from_secs(20), move || {
        suggest_orientations_cancellable(&topo, 3, 45.0, &Config::default(), &|| false)
    });
    assert!(ranked.is_some());
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

#[test]
fn a_cancel_stops_a_ranking_that_has_a_stray_vertex() {
    let topo = Arc::new(build_topology(cube_with_stray(1e16)));
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        flag.store(true, Ordering::Relaxed);
    });
    let (_, took) = within(Duration::from_secs(20), move || {
        suggest_orientations_cancellable(&topo, 3, 45.0, &Config::default(), &|| {
            stop.load(Ordering::Relaxed)
        })
    });
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

// S-05: a 20 m part was a 400 MB grid and 6.8 s.
#[test]
fn a_20_m_part_has_a_coarse_footprint_in_bounded_time_and_memory() {
    let (g, took) = within(Duration::from_secs(20), || {
        rasterize(&flat(0.0, 0.0, 20000.0, 20000.0), None).unwrap()
    });
    assert!(took < Duration::from_millis(500), "took {took:?}");
    assert!(g.bits.len() as i64 <= CELL_BUDGET, "{} cells", g.bits.len());
    assert!(g.cell > 1);
    // conservative: every mm of the part is covered, and not much more
    let a = area(Some(&g));
    assert!(a >= 20000 * 20000 && a < 21000 * 21000, "area {a}");
}

#[test]
fn a_1e7_mm_part_does_not_abort() {
    let g = rasterize(&flat(0.0, 0.0, 1e7, 1e7), None).unwrap();
    assert!(g.bits.len() as i64 <= CELL_BUDGET);
    let g = rasterize(&flat(-1e9, -1e9, 1e9, 1e9), None).unwrap();
    assert!(g.bits.len() as i64 <= CELL_BUDGET);
    // one long thin one: the budget is cells, not a square side
    let g = rasterize(&flat(0.0, 0.0, 1e9, 3.0), None).unwrap();
    assert!(g.bits.len() as i64 <= CELL_BUDGET);
}

#[test]
fn a_3e38_vertex_does_not_panic() {
    // (the audit: `capacity overflow`)
    let mut t = flat(0.0, 0.0, 10.0, 10.0);
    t.extend([0.0, 0.0, 0.0, 3e38, 0.0, 0.0, 0.0, 3e38, 0.0]);
    let g = rasterize(&t, None).unwrap();
    assert!(g.bits.len() as i64 <= CELL_BUDGET);
    // a non-finite vertex covers nothing
    let mut t = flat(0.0, 0.0, 10.0, 10.0);
    t.extend([f64::NAN, 0.0, 0.0, 1.0, 1.0, 0.0, f64::INFINITY, 0.0, 0.0]);
    let g = rasterize(&t, None).unwrap();
    assert_eq!(area(Some(&g)), 11 * 11);
    assert!(rasterize(&[f64::NAN; 9], None).is_none());
}

#[test]
fn the_grid_operations_on_a_coarse_grid_stay_conservative_and_bounded() {
    let big = rasterize(&flat(0.0, 0.0, 20000.0, 20000.0), None).unwrap();
    let small = rasterize(&flat(0.0, 0.0, 10.0, 10.0), None).unwrap();
    // a small model parked on the big one's corner clashes with it; one clear of it does not
    assert!(!shared(Some(&placed(&small, [5.0, 5.0])), Some(&big)).is_empty());
    assert!(shared(Some(&placed(&small, [-100.0, -100.0])), Some(&big)).is_empty());
    let u = union(&[Some(&big), Some(&small)]).unwrap();
    assert!(u.bits.len() as i64 <= CELL_BUDGET);
    let d = dilate(&big, 5);
    assert!(d.bits.len() as i64 <= CELL_BUDGET);
    assert!(area(Some(&d)) > area(Some(&big)));
    // a dilate by a billion cells cannot be a billion-cell grid
    let d = dilate(&small, 1_000_000_000);
    assert!(d.bits.len() as i64 <= CELL_BUDGET);
    // a part bigger than the bed has no spot on it
    let bed = sf_engine::footprint::bed_grid(250.0, 250.0);
    assert_eq!(find_spot(&bed, &big, 2, 5), None);
}
