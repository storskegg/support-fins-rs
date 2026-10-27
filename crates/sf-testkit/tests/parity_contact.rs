//! Replay of `parity/corpus/contact/*` (recorded by `parity/oracle/contact.js` from web/prop/contact.js).
//! Exact. The in-place editors are replayed on a copy of the recorded input and compared with the
//! (mutated) line JS returned. A captured call records only the arguments it passed, so the JS
//! defaults (`band = Infinity`, `step = 0.25`) fill the rest.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::contact::*;
use sf_testkit::parity::{Resolved, resolve_args};

/// Optional JS argument `i` (before the trailing `$config`, when the function has one).
fn opt(a: &[Resolved], i: usize, has_cfg: bool, default: f64) -> f64 {
    let js = if has_cfg { a.len() - 1 } else { a.len() };
    if js > i && !matches!(a[i].plain(), Value::Undefined) {
        a[i].plain().num()
    } else {
        default
    }
}

fn straightness_replay(file: &str) {
    let cases = load_cases("contact", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let line = pts3(&c.args[0]);
        check(file, c, &Value::Num(straightness(&line)), &Tol::Exact);
    }
}

fn contact_line_replay(file: &str) {
    let cases = load_cases("contact", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let pts = pts3(a[0].plain());
        let actual = match contact_line(
            &pts,
            &tris_of(&a[1]),
            a[2].plain().num() as usize,
            &cfg_of(&a),
        ) {
            Some(l) => pts_value(&l),
            None => Value::Null,
        };
        check(file, c, &actual, &Tol::Exact);
    }
}

fn editor_replay(file: &str, run: impl Fn(&mut [[f64; 3]], &[Resolved])) {
    let cases = load_cases("contact", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let mut line = pts3(a[0].plain());
        run(&mut line, &a);
        // the editors change the line in place: the same array comes back, with its properties
        check(
            file,
            c,
            &with_props(pts_value(&line), props_of(a[0].plain())),
            &Tol::Exact,
        );
    }
}

fn lower_sag_run(line: &mut [[f64; 3]], a: &[Resolved]) {
    lower_sag(line, &tris_of(&a[1]), opt(a, 2, false, f64::INFINITY));
}
fn contour_top_run(line: &mut [[f64; 3]], a: &[Resolved]) {
    contour_top(
        line,
        &tris_of(&a[1]),
        opt(a, 2, true, f64::INFINITY),
        &cfg_of(a),
    );
}
fn settle_top_run(line: &mut [[f64; 3]], a: &[Resolved]) {
    let (step, band) = (opt(a, 2, true, 0.25), opt(a, 3, true, f64::INFINITY));
    settle_top(line, &tris_of(&a[1]), step, band, &cfg_of(a));
}

#[test]
fn straightness_direct() {
    straightness_replay("straightness");
}
#[test]
fn straightness_captured() {
    straightness_replay("straightness.captured");
}
#[test]
fn contact_line_direct() {
    contact_line_replay("contactLine");
}
#[test]
fn contact_line_captured() {
    contact_line_replay("contactLine.captured");
}
#[test]
fn lower_sag_direct() {
    editor_replay("lowerSag", lower_sag_run);
}
#[test]
fn lower_sag_captured() {
    editor_replay("lowerSag.captured", lower_sag_run);
}
#[test]
fn contour_top_direct() {
    editor_replay("contourTop", contour_top_run);
}
#[test]
fn contour_top_captured() {
    editor_replay("contourTop.captured", contour_top_run);
}
#[test]
fn settle_top_direct() {
    editor_replay("settleTop", settle_top_run);
}
#[test]
fn settle_top_captured() {
    editor_replay("settleTop.captured", settle_top_run);
}
