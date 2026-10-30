//! Crash reports (1D): a panic inside `catch_unwind` -- the way the build and suggest workers turn a panic into an
//! error -- still writes a report with the message, location, a symbolicated backtrace, the operation, the
//! breadcrumbs, the settings and the versions. No app crash: the panic is caught.
use std::panic::{AssertUnwindSafe, catch_unwind};

use sf_ffi::crash::{self, operation};
use sf_ffi::{crash_breadcrumb, crash_reports_start};

/// The report folder is process-wide: the tests that panic take turns, so one counts only its own reports.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn take_turn() -> std::sync::MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(|e| e.into_inner())
}

#[inline(never)]
fn the_failing_step() {
    panic!("probe panic: wall 7 out of range");
}

#[test]
fn a_caught_panic_writes_a_report() {
    let _turn = take_turn();
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
    )
    .unwrap();
    assert_eq!(&z[..2], b"PK");
    let a = sf_io::zip::unzip(&z).unwrap();
    assert_eq!(a.len(), 2);
}

/// X-11: the body of every input-taking export runs under `guarded`: a panic comes back as an error Swift can
/// show, not as an abort, and the operation is set for the crash report while it runs.
#[test]
fn guarded_turns_a_panic_into_an_internal_error_and_names_the_operation() {
    let _turn = take_turn();
    use sf_ffi::FfiError;
    let seen = std::cell::Cell::new("");
    let r: Result<u32, FfiError> = crash::guarded("scale", || {
        seen.set(crash::current_operation());
        panic!("probe panic: index 9 out of range");
    });
    assert_eq!(seen.get(), "scale");
    assert_eq!(crash::current_operation(), "", "the operation is restored");
    match r {
        Err(FfiError::Engine { message }) => {
            assert_eq!(message, "internal error: probe panic: index 9 out of range");
        }
        other => panic!("{other:?}"),
    }
    // a formatted (String) payload, and a value body
    let r = crash::guarded_value("scale", || -> u8 { panic!("{} of {}", 3, 2) });
    assert!(matches!(r, Err(FfiError::Engine { message }) if message == "internal error: 3 of 2"));
    // an ordinary result passes through untouched, Ok or Err
    assert_eq!(crash::guarded("x", || Ok(5)).unwrap(), 5);
    assert!(matches!(
        crash::guarded("x", || Err::<u8, _>(FfiError::Cancelled)),
        Err(FfiError::Cancelled)
    ));
}

#[test]
fn a_listener_call_that_panics_is_caught_and_reports_false() {
    let _turn = take_turn();
    assert!(crash::listener_call(|| {}));
    assert!(!crash::listener_call(|| panic!("probe panic: callback")));
    assert_eq!(crash::current_operation(), "");
}

/// Native crash reports (1E) name Rust frames with `crash_demangle`: v0 (the release build's mangling) and legacy,
/// with the Mach-O `_` or without, and nothing for a C++ or Swift name.
#[test]
fn crash_demangle_reads_rust_symbols_only() {
    use sf_ffi::crash_demangle;
    // legacy: the hash goes
    assert_eq!(
        crash_demangle("_ZN7sf_core5build17h0123456789abcdefE".into()).as_deref(),
        Some("sf_core::build")
    );
    assert_eq!(
        crash_demangle("__ZN7sf_core5build17h0123456789abcdefE".into()).as_deref(),
        Some("sf_core::build")
    );
    // v0, from this very binary's own mangling of a generic-free function
    let v0 = "_RNvCs1234_7sf_core5build";
    assert_eq!(crash_demangle(v0.into()).as_deref(), Some("sf_core::build"));
    assert_eq!(
        crash_demangle(format!("_{v0}")).as_deref(),
        Some("sf_core::build")
    );
    // not Rust
    assert_eq!(crash_demangle("_ZN3foo3barEv".into()), None, "C++");
    assert_eq!(crash_demangle("$s4main3fooyyF".into()), None, "Swift");
    assert_eq!(crash_demangle("abort".into()), None);
}
