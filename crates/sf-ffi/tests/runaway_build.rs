//! X-07 (audit-2026-10/security.md, VERIFY-2026-10.md "Still open"): a build after "Load anyway" on an absurdly
//! large part ran away in the brim pad's outline bucketing (`fins::pad::brim_pad`): 4 to 7 GB within seconds, a
//! cancel did not stop it, and past 2^53 its counters stopped advancing. Now the pad's work has a budget
//! (`sf_engine::guards::Limits`), a build over it fails with `FfiError::LimitExceeded`, and the loops poll the
//! cancel flag.
//!
//! Every case runs in a child process (this test binary again, `x07_child`), watched for its peak RSS and killed
//! past a cap, so a regression is a failed test, never a machine out of memory or a crash dialog.
mod common;

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sf_engine::cancel::{Cancel, Cancelled};
use sf_engine::config::Config;
use sf_engine::fins::build::build_fins_cancellable;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::types::IDENTITY3;
use sf_ffi::{FfiError, LoadOptions, build_part_cancellable, load_parts_with};

const CASE: &str = "SF_X07_CASE";
const IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn repro(name: &str) -> String {
    format!("{}/tests/x07/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// What a child run printed, how long it took, its peak RSS, and whether the watcher killed it.
struct Run {
    out: String,
    wall: Duration,
    peak_mb: f64,
    killed: Option<String>,
}

fn rss_mb(pid: u32) -> Option<f64> {
    let o = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&o.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .map(|kb| kb / 1024.0)
}

/// Run `case` in a child, killing it past `cap_mb` of RSS or after `limit`.
fn in_child(case: &str, cap_mb: f64, limit: Duration) -> Run {
    let t = Instant::now();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["x07_child", "--exact", "--nocapture", "--test-threads=1"])
        .env(CASE, case)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (mut peak, mut killed) = (0.0f64, None);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if let Some(mb) = rss_mb(child.id()) {
            peak = peak.max(mb);
            if mb > cap_mb {
                killed = Some(format!("{mb:.0} MB at {:.2?}", t.elapsed()));
            }
        }
        if killed.is_none() && t.elapsed() > limit {
            killed = Some(format!(
                "still running at {:.2?} ({peak:.0} MB)",
                t.elapsed()
            ));
        }
        if killed.is_some() {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let wall = t.elapsed();
    let mut out = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    let out = out
        .lines()
        .filter_map(|l| l.strip_prefix("X07RESULT "))
        .collect::<Vec<_>>()
        .join("\n");
    Run {
        out,
        wall,
        peak_mb: peak,
        killed,
    }
}

/// The child: one case, its outcome on stdout, then exit (a stuck build thread must not keep it alive).
#[test]
fn x07_child() {
    let Ok(case) = std::env::var(CASE) else {
        return; // run as an ordinary test: nothing to do
    };
    let out = match case.split(':').collect::<Vec<_>>()[..] {
        ["build", file, json] => build_case(file, json),
        ["cancel", file] => cancel_case(file),
        _ => panic!("unknown case {case}"),
    };
    println!("\nX07RESULT {out}");
    std::process::exit(0);
}

/// Load with the limits lifted (the user said "Load anyway"), then build the way the app does: `app` sends the
/// app's real settings (the page's build), `empty` sends `{}` / `{}` (the CLI's build, as the verification ran it).
fn build_case(file: &str, json: &str) -> String {
    let parts = load_parts_with(repro(file), LoadOptions { allow_large: true }).expect("loads");
    let (opts, tunables) = match json {
        "app" => (common::options(), common::tunables()),
        _ => ("{}".to_string(), "{}".to_string()),
    };
    let t = Instant::now();
    let r = build_part_cancellable(&parts[0], &IDENTITY, &opts, &tunables, &Cancel::default());
    let ms = t.elapsed().as_secs_f64() * 1e3;
    match r {
        Ok(_) => format!("ok {ms:.0} ms"),
        Err(FfiError::LimitExceeded {
            kind,
            estimate,
            limit,
            message,
        }) => format!("limit {kind:?} {estimate:e} {limit} {ms:.0} ms: {message}"),
        Err(e) => format!("error {e:?} {ms:.0} ms"),
    }
}

/// The engine's build of `file` with the budgets lifted (so it does run away: in `fins::fill` for the 7.6 km
/// coupon), its cancel raised 100 ms in: how long after the raise it returned.
fn cancel_case(file: &str) -> String {
    let bytes = std::fs::read(repro(file)).unwrap();
    let topo = Arc::new(build_topology(sf_io::stl::read_stl(&bytes).unwrap()));
    let res = analyze(&topo, 45.0, &IDENTITY3);
    let cfg = Config {
        limits: sf_engine::guards::Limits::lifted(),
        ..Default::default()
    };
    let opts = sf_testkit::site_opts("pla", "auto", false);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    let raised = Arc::new(std::sync::Mutex::new(None::<Instant>));
    let r2 = raised.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        *r2.lock().unwrap() = Some(Instant::now());
        f2.store(true, Ordering::SeqCst);
    });
    let r = build_fins_cancellable(&topo, &res, &IDENTITY3, &opts, &cfg, &Cancel::new(flag));
    let back = Instant::now();
    let after = raised
        .lock()
        .unwrap()
        .map(|t| back.duration_since(t).as_secs_f64() * 1e3);
    match (r, after) {
        (Err(Cancelled), Some(ms)) => format!("cancelled {ms:.1} ms after the raise"),
        (Err(Cancelled), None) => "cancelled before the raise".into(),
        (Ok(_), _) => "built (the cancel came too late to matter)".into(),
    }
}

#[test]
fn a_part_loaded_anyway_fails_its_build_fast_with_a_limit_error() {
    for file in ["coupon_one_1e30.stl", "coupon_scaled_1e5.stl"] {
        for json in ["app", "empty"] {
            let run = in_child(
                &format!("build:{file}:{json}"),
                1024.0,
                Duration::from_secs(10),
            );
            eprintln!(
                "{file} {json}: {:.2?}, peak {:.0} MB, {}",
                run.wall, run.peak_mb, run.out
            );
            assert!(
                run.killed.is_none(),
                "{file} {json}: killed, {}",
                run.killed.unwrap()
            );
            assert!(
                run.out.starts_with("limit Dimension") || run.out.starts_with("ok"),
                "{file} {json}: {}",
                run.out
            );
            assert!(
                run.wall < Duration::from_secs(3),
                "{file} {json}: {:.2?}",
                run.wall
            );
            assert!(run.peak_mb < 512.0, "{file} {json}: {:.0} MB", run.peak_mb);
        }
    }
}

#[test]
fn a_cancel_raised_in_a_runaway_build_returns_within_100_ms() {
    let run = in_child(
        "cancel:coupon_scaled_1e5.stl",
        4096.0,
        Duration::from_secs(10),
    );
    eprintln!(
        "cancel: {:.2?}, peak {:.0} MB, {}",
        run.wall, run.peak_mb, run.out
    );
    assert!(run.killed.is_none(), "killed, {}", run.killed.unwrap());
    let ms: f64 = run
        .out
        .strip_prefix("cancelled ")
        .and_then(|s| s.split(' ').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("{}", run.out));
    assert!(ms < 100.0, "returned {ms} ms after the raise");
}
