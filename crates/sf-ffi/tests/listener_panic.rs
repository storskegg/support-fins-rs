//! C-06: a listener callback that panics (UniFFI panics when the Swift side reports an unexpected error) must not
//! end the `sf-build` / `sf-suggest` worker thread. Before the fix the thread died, every later `build()` was
//! sent into a closed channel, and no build ever landed again. The panic is caught, the crash-dump hook reports
//! it, and the next job on the same worker is delivered.
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use sf_ffi::{
    BuildListener, BuildResult, Builder, FfiError, Part, StrengthAdvice, SuggestContext,
    SuggestListener, SuggestResult, Suggester, crash_reports_start, load_parts,
};
use sf_testkit::models_dir;

const ID: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

fn load(name: &str) -> Arc<Part> {
    load_parts(
        models_dir()
            .join(format!("{name}.stl"))
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap()
    .remove(0)
}

/// The crash-report folder is process-wide: one folder for every test of this file.
fn reports_dir() -> &'static std::path::Path {
    static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("sf-listener-panic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crash_reports_start(
            dir.to_string_lossy().into_owned(),
            "test".into(),
            "{}".into(),
        );
        dir
    })
}

/// The first `n` callbacks panic; the rest deliver.
struct BuildProbe {
    panics_left: AtomicUsize,
    tx: Mutex<mpsc::Sender<&'static str>>,
}
impl BuildListener for BuildProbe {
    fn done(&self, _: u64, _: Arc<BuildResult>) {
        if self.panics_left.load(Ordering::SeqCst) > 0 {
            self.panics_left.fetch_sub(1, Ordering::SeqCst);
            self.tx.lock().unwrap().send("panicking").unwrap();
            panic!("listener probe: done");
        }
        self.tx.lock().unwrap().send("done").unwrap();
    }
    fn failed(&self, _: u64, _: FfiError) {
        if self.panics_left.load(Ordering::SeqCst) > 0 {
            self.panics_left.fetch_sub(1, Ordering::SeqCst);
            self.tx.lock().unwrap().send("panicking").unwrap();
            panic!("listener probe: failed");
        }
        self.tx.lock().unwrap().send("failed").unwrap();
    }
}
fn probe(n: usize) -> (Box<BuildProbe>, mpsc::Receiver<&'static str>) {
    let (tx, rx) = mpsc::channel();
    (
        Box::new(BuildProbe {
            panics_left: AtomicUsize::new(n),
            tx: Mutex::new(tx),
        }),
        rx,
    )
}

/// Queue a build whose first callback panics and wait until it has (the worker then has to survive it).
fn build_whose_callback_panics(b: &Builder, options: String) {
    let (l, rx) = probe(1);
    b.build(ID.to_vec(), options, common::tunables(), l)
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(60)), Ok("panicking"));
}

fn build_that_lands(b: &Builder) {
    let (l, rx) = probe(0);
    b.build(ID.to_vec(), common::options(), common::tunables(), l)
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(60)), Ok("done"));
}

#[test]
fn a_build_lands_after_a_done_callback_panicked() {
    reports_dir();
    let b = Builder::new(load("lbracket"));
    build_whose_callback_panics(&b, common::options());
    build_that_lands(&b);
}

#[test]
fn a_build_lands_after_a_failed_callback_panicked() {
    reports_dir();
    let b = Builder::new(load("lbracket"));
    // options that fail: `failed` is called, and panics
    build_whose_callback_panics(&b, "not json".into());
    build_that_lands(&b);
}

#[test]
fn builds_keep_landing_after_many_panicking_callbacks() {
    reports_dir();
    let b = Builder::new(load("lbracket"));
    for _ in 0..5 {
        build_whose_callback_panics(&b, common::options());
    }
    build_that_lands(&b);
}

#[test]
fn the_callback_panic_is_reported_to_the_crash_dump() {
    let dir = reports_dir();
    let b = Builder::new(load("lbracket"));
    build_whose_callback_panics(&b, common::options());
    build_that_lands(&b);
    let mut found = false;
    for _ in 0..100 {
        for e in std::fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            if path.extension().is_some_and(|x| x == "json") {
                let j: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
                found |=
                    j["message"] == "listener probe: done" && j["operation"] == "listener callback";
            }
        }
        if found {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        found,
        "a crash report for the callback panic, naming the callback"
    );
}

struct SuggestProbe {
    /// `progress` always panics; `suggested` panics while this is positive
    panic_progress: bool,
    panics_left: AtomicUsize,
    tx: Mutex<mpsc::Sender<&'static str>>,
}
impl SuggestListener for SuggestProbe {
    fn suggested(&self, _: u64, _: SuggestResult) {
        if self.panics_left.load(Ordering::SeqCst) > 0 {
            self.panics_left.fetch_sub(1, Ordering::SeqCst);
            self.tx.lock().unwrap().send("panicking").unwrap();
            panic!("listener probe: suggested");
        }
        self.tx.lock().unwrap().send("suggested").unwrap();
    }
    fn progress(&self, _: u64, _: f64) {
        if self.panic_progress {
            panic!("listener probe: progress");
        }
    }
    fn strength(&self, _: u64, _: StrengthAdvice) {
        self.tx.lock().unwrap().send("strength").unwrap();
    }
    fn failed(&self, _: u64, _: FfiError) {
        self.tx.lock().unwrap().send("failed").unwrap();
    }
}
fn suggest_probe(
    panic_progress: bool,
    panics: usize,
) -> (Box<SuggestProbe>, mpsc::Receiver<&'static str>) {
    let (tx, rx) = mpsc::channel();
    (
        Box::new(SuggestProbe {
            panic_progress,
            panics_left: AtomicUsize::new(panics),
            tx: Mutex::new(tx),
        }),
        rx,
    )
}
fn ctx() -> SuggestContext {
    SuggestContext {
        current_bore: 0,
        density: 1.24,
    }
}

#[test]
fn a_ranking_lands_after_a_suggested_callback_panicked() {
    reports_dir();
    let s = Suggester::new(load("sphere"));
    let (l, rx) = suggest_probe(false, 1);
    s.suggest(45.0, common::options(), common::tunables(), ctx(), l)
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(60)), Ok("panicking"));
    let (l, rx) = suggest_probe(false, 0);
    s.suggest(45.0, common::options(), common::tunables(), ctx(), l)
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(60)), Ok("suggested"));
}

#[test]
fn a_ranking_still_lands_when_every_progress_callback_panics() {
    reports_dir();
    let s = Suggester::new(load("sphere"));
    let (l, rx) = suggest_probe(true, 0);
    s.suggest(45.0, common::options(), common::tunables(), ctx(), l)
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(60)), Ok("suggested"));
    // and the worker still takes the next one
    let (l, rx) = suggest_probe(true, 0);
    s.suggest(45.0, common::options(), common::tunables(), ctx(), l)
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(60)), Ok("suggested"));
}
