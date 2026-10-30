//! Crash reports (1D): a panic inside `catch_unwind` -- the way the build and suggest workers turn a panic into an
//! error -- still writes a report with the message, location, a symbolicated backtrace, the operation, the
//! breadcrumbs, the settings and the versions. No app crash: the panic is caught.
use std::panic::{AssertUnwindSafe, catch_unwind};

use sf_ffi::crash::{self, operation};
use sf_ffi::{crash_breadcrumb, crash_reports_start};

#[inline(never)]
fn the_failing_step() {
    panic!("probe panic: wall 7 out of range");
}

#[test]
fn a_caught_panic_writes_a_report() {
    let dir = std::env::temp_dir().join(format!("sf-crash-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    crash_reports_start(
        dir.to_string_lossy().into_owned(),
        "9.9.9 (test)".into(),
        r#"{"options":{"nozzle":0.4}}"#.into(),
    );
    // more breadcrumbs than the ring keeps: the oldest fall off
    for i in 0..(crash::MAX_CRUMBS + 5) {
        crash_breadcrumb(format!("action {i}"));
    }
    let caught = {
        let _op = operation("suggest wide");
        catch_unwind(AssertUnwindSafe(the_failing_step))
    };
    assert!(caught.is_err(), "the panic was caught, not fatal");
    assert_eq!(
        crash::current_operation(),
        "",
        "the guard restored the operation"
    );

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 2, "one .json and one .txt: {files:?}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&files[0]).unwrap()).unwrap();
    let text = std::fs::read_to_string(&files[1]).unwrap();

    assert_eq!(json["kind"], "rust-panic");
    assert_eq!(json["message"], "probe panic: wall 7 out of range");
    assert!(json["file"].as_str().unwrap().ends_with("crash.rs"));
    assert!(json["line"].as_u64().unwrap() > 0);
    assert_eq!(json["operation"], "suggest wide");
    assert_eq!(json["appVersion"], "9.9.9 (test)");
    assert_eq!(json["engineVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["upstream"].as_str().unwrap().len(), 7);
    assert!(json["time"].as_str().unwrap().ends_with('Z'));
    assert!(!json["thread"].as_str().unwrap().is_empty());
    assert_eq!(json["settings"]["options"]["nozzle"], 0.4);
    // a symbolicated backtrace: frames, and our function's name among them
    let bt = json["backtrace"].as_str().unwrap();
    assert!(bt.lines().count() > 5, "frames: {bt}");
    assert!(bt.contains("the_failing_step"), "symbolicated: {bt}");
    // the ring kept the last 50, oldest first
    let crumbs = json["breadcrumbs"].as_array().unwrap();
    assert_eq!(crumbs.len(), crash::MAX_CRUMBS);
    assert_eq!(crumbs[0], "action 5");
    assert_eq!(
        crumbs[crash::MAX_CRUMBS - 1],
        format!("action {}", crash::MAX_CRUMBS + 4)
    );
    // the text block says the same
    assert!(text.contains("Operation: suggest wide"));
    assert!(text.contains("Panic: probe panic: wall 7 out of range"));
    assert!(text.contains("Backtrace:"));

    // a second panic is a second report (the hook stays installed); no operation reads "(none)"
    let _ = catch_unwind(AssertUnwindSafe(the_failing_step));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_zip_holds_the_files() {
    let z = sf_ffi::crash_zip(
        vec!["a.txt".into(), "b.json".into()],
        vec![b"hello".to_vec(), b"{}".to_vec()],
    );
    assert_eq!(&z[..2], b"PK");
    let a = sf_io::zip::unzip(&z).unwrap();
    assert_eq!(a.len(), 2);
}
