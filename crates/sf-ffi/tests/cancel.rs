//! C-04: a dropped (or cancelled) Suggester or Builder stops working at once, and stops competing with the
//! next part's build.
//!
//! The evidence is process CPU time (`getrusage`), so everything runs in ONE test (nothing else in this binary
//! may burn CPU meanwhile). Each scenario starts a long job, abandons it, lets 100 ms pass (the budget) and
//! then measures the CPU the process burns over the next window. The control runs leak the owner with
//! `mem::forget`, which is what a drop did before the fix (the worker never heard about it): they must still be
//! busy, or the harness measures nothing. `SF_CANCEL_PART=<file>` measures on that file's first object instead of the generated torus. Numbers are printed (`cargo test -p sf-ffi --test cancel -- --nocapture`).
#![allow(unsafe_code)]
mod common;

use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use sf_ffi::{
    BuildListener, BuildResult, Builder, FfiError, Part, StrengthAdvice, SuggestContext,
    SuggestListener, SuggestResult, Suggester, build_part, load_parts,
};

#[repr(C)]
#[derive(Default)]
struct Timeval {
    sec: i64,
    usec: i32,
    pad: i32,
}
#[repr(C)]
#[derive(Default)]
struct Rusage {
    utime: Timeval,
    stime: Timeval,
    rest: [i64; 14],
}
unsafe extern "C" {
    fn getrusage(who: i32, r: *mut Rusage) -> i32;
}
fn cpu_ms() -> f64 {
    let mut r = Rusage::default();
    unsafe { getrusage(0, &raw mut r) };
    (r.utime.sec + r.stime.sec) as f64 * 1e3 + (r.utime.usec + r.stime.usec) as f64 / 1e3
}
/// CPU cores the process burns over `window`.
fn cores(window: Duration) -> f64 {
    let (c0, t0) = (cpu_ms(), Instant::now());
    std::thread::sleep(window);
    (cpu_ms() - c0) / (t0.elapsed().as_secs_f64() * 1e3)
}

fn write_torus(path: &std::path::Path, u: usize, v: usize) {
    let (r0, r1) = (40.0f64, 12.0f64);
    let at = |i: usize, j: usize| {
        let (a, b) = (
            (i % u) as f64 / u as f64 * std::f64::consts::TAU,
            (j % v) as f64 / v as f64 * std::f64::consts::TAU,
        );
        [
            ((r0 + r1 * b.cos()) * a.cos()) as f32,
            ((r0 + r1 * b.cos()) * a.sin()) as f32,
            (r1 * b.sin()) as f32,
        ]
    };
    let mut pos = Vec::new();
    for i in 0..u {
        for j in 0..v {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            pos.extend([a, b, c, a, c, d]);
        }
    }
    let mut b = vec![0u8; 80];
    b.extend(((pos.len() / 3) as u32).to_le_bytes());
    for t in pos.chunks(3) {
        b.extend([0u8; 12]);
        for p in t {
            for c in p {
                b.extend(c.to_le_bytes());
            }
        }
        b.extend([0u8; 2]);
    }
    std::fs::write(path, b).unwrap();
}

fn part() -> Arc<Part> {
    // SF_CANCEL_PART=<file>: measure on that file's first object instead (e.g. the private 3MF)
    if let Ok(f) = std::env::var("SF_CANCEL_PART") {
        return load_parts(f).unwrap().remove(0);
    }
    let dir = std::env::temp_dir().join(format!("sf-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("torus.stl");
    // under RANK_CHEAP_FACES, so a ranking builds fins for its finalists (pass 2)
    write_torus(&f, 520, 300); // 312k faces: a wide ranking runs well over a second alone
    let p = load_parts(f.to_string_lossy().into_owned())
        .unwrap()
        .remove(0);
    let _ = std::fs::remove_dir_all(&dir);
    p
}
fn ctx() -> SuggestContext {
    SuggestContext {
        current_bore: 0,
        density: 1.24,
    }
}

enum Ev {
    Progress,
    Done,
    Other,
}
struct SL(Mutex<mpsc::Sender<Ev>>);
impl SuggestListener for SL {
    fn suggested(&self, _: u64, _: SuggestResult) {
        let _ = self.0.lock().unwrap().send(Ev::Done);
    }
    fn progress(&self, _: u64, _: f64) {
        let _ = self.0.lock().unwrap().send(Ev::Progress);
    }
    fn strength(&self, _: u64, _: StrengthAdvice) {
        let _ = self.0.lock().unwrap().send(Ev::Done);
    }
    fn failed(&self, _: u64, _: FfiError) {
        let _ = self.0.lock().unwrap().send(Ev::Other);
    }
}
fn sl() -> (Box<SL>, mpsc::Receiver<Ev>) {
    let (tx, rx) = mpsc::channel();
    (Box::new(SL(Mutex::new(tx))), rx)
}
struct BL(Mutex<mpsc::Sender<Ev>>);
impl BuildListener for BL {
    fn done(&self, _: u64, _: Arc<BuildResult>) {
        let _ = self.0.lock().unwrap().send(Ev::Done);
    }
    fn failed(&self, _: u64, _: FfiError) {
        let _ = self.0.lock().unwrap().send(Ev::Other);
    }
}

/// How long the process takes to answer one build of the part.
fn build_ms(part: &Part) -> f64 {
    let t = Instant::now();
    build_part(
        part,
        &[1., 0., 0., 0., 1., 0., 0., 0., 1.],
        &common::options(),
        &common::tunables(),
    )
    .unwrap();
    t.elapsed().as_secs_f64() * 1e3
}

const RUN_BEFORE_ABANDON: Duration = Duration::from_millis(300);
const BUDGET: Duration = Duration::from_millis(100);
const WINDOW: Duration = Duration::from_millis(300);

fn start_wide(part: &Arc<Part>) -> (Arc<Suggester>, mpsc::Receiver<Ev>) {
    let s = Suggester::new(part.clone());
    let (l, rx) = sl();
    s.suggest_wide(45.0, common::options(), common::tunables(), ctx(), 10.0, l)
        .unwrap();
    std::thread::sleep(RUN_BEFORE_ABANDON);
    (s, rx)
}

#[test]
fn abandoning_a_search_or_a_build_stops_the_cpu_within_100_ms() {
    let part = part();
    let alone = (0..3).map(|_| build_ms(&part)).fold(f64::MAX, f64::min);
    eprintln!("a build alone: {alone:.0} ms");

    // control: the old drop (the owner is leaked, the worker never hears) keeps the pool busy
    let (s, rx) = start_wide(&part);
    std::mem::forget(Arc::clone(&s));
    let leaked = cores(WINDOW);
    eprintln!("wide ranking, owner leaked (old drop): {leaked:.2} cores over the next {WINDOW:?}");
    // (a loaded machine squeezes the control's share of the CPU, so the bar is "clearly running", well above the
    // 0.15 an abandoned job must fall under)
    assert!(leaked > 0.25, "the control ranking is not busy ({leaked})");
    let slowed = build_ms(&part);
    eprintln!(
        "a build while the leaked ranking runs (old drop): {slowed:.0} ms (alone {alone:.0} ms)"
    );
    // let it finish (it was never going to stop), so it does not pollute the next scenario
    s.cancel();
    drop(s);
    while rx.recv_timeout(Duration::from_secs(5)).is_ok() {}

    // dropping the Suggester mid-ranking
    let (s, rx) = start_wide(&part);
    let t = Instant::now();
    drop(s);
    let mut quiet_at = None;
    for _ in 0..40 {
        if cores(Duration::from_millis(25)) < 0.1 {
            quiet_at = Some(t.elapsed());
            break;
        }
    }
    eprintln!("quiet after {quiet_at:?}");
    assert!(
        quiet_at.is_some_and(|q| q < Duration::from_millis(250)),
        "the pool was still busy {quiet_at:?} after the drop"
    );
    std::thread::sleep(BUDGET);
    let after = cores(WINDOW);
    eprintln!(
        "wide ranking, Suggester dropped: {after:.2} cores from {:?} after the drop",
        t.elapsed().checked_sub(WINDOW).unwrap()
    );
    assert!(after < 0.15, "still {after} cores after the drop");
    while let Ok(e) = rx.recv_timeout(Duration::from_millis(300)) {
        assert!(
            !matches!(e, Ev::Done | Ev::Other),
            "a dropped ranking was delivered"
        );
    }

    // cancel() mid-ranking, the Suggester lives on and answers the next request
    let (s, rx) = start_wide(&part);
    s.cancel();
    std::thread::sleep(BUDGET);
    let after = cores(WINDOW);
    eprintln!("wide ranking, cancel(): {after:.2} cores");
    assert!(after < 0.15, "still {after} cores after cancel()");
    drop(rx);
    let (l, rx) = sl();
    s.suggest(45.0, common::options(), common::tunables(), ctx(), l)
        .unwrap();
    loop {
        match rx.recv_timeout(Duration::from_secs(120)).unwrap() {
            Ev::Done => break,
            Ev::Progress => {}
            Ev::Other => panic!("failed"),
        }
    }

    // a concurrent build is not slowed by the dropped ranking
    let (s, _rx) = start_wide(&part);
    drop(s);
    let with = build_ms(&part);
    eprintln!("a build right after the drop: {with:.0} ms (alone {alone:.0} ms)");
    assert!(
        with < alone * 1.6 + 50.0,
        "{with} ms against {alone} ms alone"
    );

    // a Strength search is stale-checked too: cancel() before its first analyze finishes delivers nothing
    let s = Suggester::new(part.clone());
    let (l, rx) = sl();
    s.strength(vec![0., 0., 1.], vec![0., 0., 1.], 45.0, l)
        .unwrap();
    s.cancel();
    drop(s);
    std::thread::sleep(Duration::from_millis(500));
    while let Ok(e) = rx.try_recv() {
        assert!(
            !matches!(e, Ev::Done),
            "a cancelled strength search was delivered"
        );
    }

    // control: a leaked Builder finishes its build (and any queued) for nobody
    let b = Builder::new(part.clone());
    let (tx, _brx) = mpsc::channel();
    for _ in 0..8 {
        b.build(
            vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
            common::options(),
            common::tunables(),
            Box::new(BL(Mutex::new(tx.clone()))),
        )
        .unwrap();
    }
    let leaked = cores(WINDOW);
    eprintln!("build, Builder leaked (old drop): {leaked:.2} cores");
    // the old drop would have left that build running; stop it here and wait for quiet, so the scenario
    // below measures only itself
    b.cancel();
    drop(b);
    for _ in 0..200 {
        if cores(Duration::from_millis(50)) < 0.05 {
            break;
        }
    }

    // dropping the Builder mid-build
    let b = Builder::new(part);
    let (tx, brx) = mpsc::channel();
    b.build(
        vec![1., 0., 0., 0., 1., 0., 0., 0., 1.],
        common::options(),
        common::tunables(),
        Box::new(BL(Mutex::new(tx))),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(50));
    drop(b);
    std::thread::sleep(BUDGET);
    let after = cores(WINDOW);
    eprintln!("build, Builder dropped: {after:.2} cores");
    // (a build unwinds at its next poll; in a loaded `cargo test` that tail can still show as a sliver of CPU at
    // 100 ms, so the bar is a quarter core: far under the 1-3 cores of the control)
    assert!(after < 0.3, "still {after} cores after the Builder drop");
    assert!(brx.try_recv().is_err(), "a dropped Builder delivered");
}
