//! Port of upstream `tests/end_rows.test.js`: flush end rows are a candidate the judge races per
//! region (web/prop/endrows.js, local issue 062 step 3): the flush layout stays unless the judge
//! scores the mid-strip rows higher (a tie keeps flush). `endRows: false` builds the flush layout
//! everywhere, as before the race.
mod builds;
use builds::*;
use sf_engine::fins::build::{FinKind, FinsOpts};
use sf_testkit::{load_model, rot_x};
use std::sync::Arc;

fn build_model(name: &str, deg: f64, coverage: f64) -> sf_engine::fins::build::FinsBuilt {
    let topo = Arc::new(load_model(name));
    let opts: FinsOpts = auto_opts(Some(coverage));
    build(&topo, &rot_x(deg), &opts).1
}

// end_rows.test.js:10
#[test]
fn end_rows_a_tilted_cube_wins_its_flush_ends_in_the_race_4_walls_the_end_ones_tagged_flush() {
    let b = build_model("cube", 40.0, 0.5);
    assert!(b.props.len() == 4, "{} walls, pinned 4", b.props.len());
    assert!(
        b.props.iter().filter(|q| q.flush == Some(true)).count() == 4,
        "the walls come from the flush candidate"
    );
}

// end_rows.test.js:16
#[test]
fn end_rows_a_tie_on_the_judge_keeps_the_flush_layout_tshape_x60_sparse() {
    // the mid-strip rows hold exactly what the flush ends do there; the cheaper layout must not win
    let race = build_model("tshape", 60.0, 0.0);
    assert!(
        race.props.iter().any(|q| q.flush == Some(true)),
        "the flush layout won the tie"
    );
    // (the flush layout replaces the mid-strip rows: no row of the old layout stays beside it)
    assert!(
        race.props.len() == 3,
        "{} walls, pinned 3",
        race.props.len()
    );
}

// end_rows.test.js:24
#[test]
fn end_rows_a_portal_keeps_the_wedge_the_flush_layout_leaves_it_the_rows_would_serve_its_patch() {
    let b = build_model("portal", 40.0, 0.5);
    assert!(
        b.fins.iter().any(|f| f.kind == FinKind::Wedge),
        "the wedge is gone"
    );
}
