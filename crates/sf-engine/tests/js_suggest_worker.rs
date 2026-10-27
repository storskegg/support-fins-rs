//! Port of upstream `tests/suggest_worker.test.js`: Suggest orientation runs in a Worker
//! (web/suggestworker.js), with the page's tunables applied first. Its ranking has to be the one the
//! page used to compute itself, with the default settings and with a changed Support gap -- on a
//! torus, whose finalists all take walls, so the gap really changes the answer (checked below, or
//! the test would prove nothing about the tunables reaching the Worker).
//!
//! In Rust the worker is a direct `suggest_orientations` call on a `Config` the tunables were applied
//! to. The "page" side here is a `Config` that was last left at the OTHER gap and then had the
//! tunables applied, the way the page's module state is (applied, built, put back): a gap that stuck
//! would rank differently.
use std::sync::Arc;

use sf_engine::config::{Config, Tunables, apply_tunables};
use sf_engine::orient::{Suggestions, suggest_orientations};
use sf_testkit::load_model;

fn tunables(gap: f64) -> Tunables {
    Tunables {
        prop_gap: Some(gap),
        ..Default::default()
    }
}

fn ranked(gap: Option<f64>) -> (Suggestions, Suggestions) {
    let topo = Arc::new(load_model("torus"));
    // the worker: a fresh state, with the tunables applied when the request carries them
    let mut worker = Config::default();
    if let Some(g) = gap {
        apply_tunables(&mut worker, Some(&tunables(g)));
    }
    let got = suggest_orientations(&topo, 3, 45.0, &worker);
    // the page: its state was last left at the other gap
    let mut page = Config::default();
    let before = page.prop.gap;
    apply_tunables(
        &mut page,
        Some(&tunables(if gap == Some(0.35) { 0.2 } else { 0.35 })),
    );
    apply_tunables(&mut page, Some(&tunables(gap.unwrap_or(before))));
    let want = suggest_orientations(&topo, 3, 45.0, &page);
    (got, want)
}

// suggest_worker.test.js:23
#[test]
fn suggest_worker_ranks_exactly_as_the_page_did_torus_gap_default() {
    let (got, want) = ranked(None);
    assert!(
        format!("{got:?}") == format!("{want:?}"),
        "worker ranking differs:\n{got:?}\nvs\n{want:?}"
    );
}

// suggest_worker.test.js:23
#[test]
fn suggest_worker_ranks_exactly_as_the_page_did_torus_gap_0_35() {
    let (got, want) = ranked(Some(0.35));
    assert!(
        format!("{got:?}") == format!("{want:?}"),
        "worker ranking differs:\n{got:?}\nvs\n{want:?}"
    );
}

// suggest_worker.test.js:39
#[test]
fn the_changed_gap_changes_the_torus_ranking_so_the_tests_above_test_the_tunables() {
    let default = format!("{:?}", ranked(None).0);
    let wide = format!("{:?}", ranked(Some(0.35)).0);
    assert!(
        default != wide,
        "gap 0.35 ranked the torus exactly like 0.2"
    );
}
