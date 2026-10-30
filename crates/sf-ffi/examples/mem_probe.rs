//! Peak-memory probe for the load and the build (docs/audit-2026-10 C-05, C-07; `just` has no recipe, run it by hand):
//!
//!   mem_probe load FILE [OBJ]      what opening a file costs: the app's steps (`load_parts`, every object's `info`
//!                                  for the picker, then the picked object's `mesh`), RSS after each
//!   mem_probe build FILE OBJ N [TILT]   the RSS one held `BuildResult` costs: N builds kept, RSS after each
//!
//! Prints sizes and counts only. Peak RSS is `getrusage`'s `ru_maxrss` (bytes on macOS).
#![allow(unsafe_code)]
use sf_ffi::{build_part, load_parts};
use std::alloc::{GlobalAlloc, Layout, System};

/// BIG=1 probes with the app's allocator (`sf_ffi::BigBlocks`) under the counter, otherwise the system's; set in
/// `main` before the first big block, so every block is freed the way it was allocated
static BIG_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
macro_rules! inner {
    ($m:ident($($a:expr),*)) => {
        if BIG_ON.load(std::sync::atomic::Ordering::Relaxed) { sf_ffi::BigBlocks.$m($($a),*) } else { System.$m($($a),*) }
    };
}
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// Counts the bytes allocated and not yet freed, so what a held `BuildResult` owns is exact, not the allocator's
/// retained pages. (The page-aligned `GpuBuffer`s are counted too: they go through the global allocator.)
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static HIGH: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let now = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        HIGH.fetch_max(now, Relaxed);
        unsafe { inner!(alloc(l)) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let now = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        HIGH.fetch_max(now, Relaxed);
        unsafe { inner!(alloc_zeroed(l)) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { inner!(dealloc(p, l)) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let now = LIVE.fetch_add(new, Relaxed) + new - l.size();
        LIVE.fetch_sub(l.size(), Relaxed);
        HIGH.fetch_max(now, Relaxed);
        unsafe { inner!(realloc(p, l, new)) }
    }
}
#[global_allocator]
static A: Counting = Counting;

#[repr(C)]
#[derive(Default)]
struct Rusage {
    times: [i64; 4],
    maxrss: i64,
    rest: [i64; 13],
}
unsafe extern "C" {
    fn getrusage(who: i32, r: *mut Rusage) -> i32;
}
fn peak_mb() -> f64 {
    let mut r = Rusage::default();
    unsafe { getrusage(0, &mut r) };
    r.maxrss as f64 / 1048576.0
}
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}
fn say(step: &str) {
    println!(
        "{step:<30} rss {:>7.0} MB  peak {:>7.0} MB  live {:>7.1} MB  live high {:>7.1} MB",
        rss_mb(),
        peak_mb(),
        LIVE.load(Relaxed) as f64 / 1048576.0,
        HIGH.load(Relaxed) as f64 / 1048576.0
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    BIG_ON.store(std::env::var("BIG").is_ok(), Relaxed);
    let file = a.get(2).expect("FILE").clone();
    say("start");
    // LIVE=1: the live heap every 40 ms during the load, to see which stage holds the peak
    let sampler = std::env::var("LIVE").ok().map(|_| {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || {
            let mut line = Vec::new();
            while !s2.load(Relaxed) {
                line.push(LIVE.load(Relaxed) / 1048576);
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            println!("live MB every 40 ms: {line:?}");
        });
        (stop, h)
    });
    let parts = load_parts(file).expect("load");
    if let Some((stop, h)) = sampler {
        stop.store(true, Relaxed);
        h.join().unwrap();
    }
    say("load_parts");
    if let Ok(secs) = std::env::var("PAUSE") {
        std::thread::sleep(std::time::Duration::from_secs(secs.parse().unwrap()));
    }
    let faces: u64 = parts.iter().map(|p| p.info().triangles).sum();
    println!("{} objects, {faces} faces", parts.len());
    say("info of every object (picker)");
    match a[1].as_str() {
        "load" => {
            let i: usize = a.get(3).map_or(0, |s| s.parse().unwrap());
            let mesh = parts[i].mesh();
            say("mesh of the picked object");
            drop(mesh);
        }
        "build" => {
            let i: usize = a[3].parse().unwrap();
            let n: usize = a[4].parse().unwrap();
            // a tilt about x (degrees) gives a flat part an underside to support
            let t = a
                .get(5)
                .map_or(0.0, |s| s.parse::<f64>().unwrap())
                .to_radians();
            let rot = [1.0, 0.0, 0.0, 0.0, t.cos(), t.sin(), 0.0, -t.sin(), t.cos()];
            // OPTS: the options JSON (default "{}"), e.g. {"coverage":1} for a part with many fins
            let opts = std::env::var("OPTS").unwrap_or_else(|_| "{}".into());
            let mut held = Vec::new();
            for k in 0..n {
                held.push(build_part(&parts[i], &rot, &opts, "{}").expect("build"));
                say(&format!("build {} held", k + 1));
            }
            println!("fin triangles {}", held[0].triangles().len() / 9);
        }
        // digest FILE OBJ TILT OUTDIR: what the removal readouts, the exports and the footprint make of one build, so a
        // change to how a BuildResult stores its fins can be compared byte for byte (sha256 of the files and the JSON)
        "digest" => {
            let i: usize = a[3].parse().unwrap();
            let t = a[4].parse::<f64>().unwrap().to_radians();
            let dir = std::path::PathBuf::from(&a[5]);
            std::fs::create_dir_all(&dir).unwrap();
            let rot = [1.0, 0.0, 0.0, 0.0, t.cos(), t.sin(), 0.0, -t.sin(), t.cos()];
            let opts = std::env::var("OPTS").unwrap_or_else(|_| "{}".into());
            let b = std::sync::Arc::new(build_part(&parts[i], &rot, &opts, "{}").expect("build"));
            let ids: Vec<u64> = vec![0, 1, 2, 7];
            std::fs::write(
                dir.join("after0.json"),
                b.after_removal_json(vec![]).unwrap(),
            )
            .unwrap();
            std::fs::write(
                dir.join("after3.json"),
                b.after_removal_json(ids.clone()).unwrap(),
            )
            .unwrap();
            std::fs::write(dir.join("records.json"), b.records_json()).unwrap();
            for ext in ["stl", "3mf", "obj"] {
                let path = dir
                    .join(format!("out.{ext}"))
                    .to_string_lossy()
                    .into_owned();
                match sf_ffi::export(path, vec![parts[i].clone()], vec![b.clone()], "{}".into()) {
                    Ok(()) => {}
                    Err(e) => println!("export {ext}: {e}"),
                }
            }
            let path = dir.join("removed.stl").to_string_lossy().into_owned();
            let _ = sf_ffi::export(
                path,
                vec![parts[i].clone()],
                vec![b.clone()],
                "{\"removed\":[0,1,2,7]}".into(),
            );
            if let Some(f) = sf_ffi::footprint_of_build(b.clone(), vec![]).unwrap() {
                println!("footprint area {} bounds {:?}", f.area(), f.bounds());
            }
            if let Some(f) = sf_ffi::footprint_of_build(b.clone(), ids).unwrap() {
                println!(
                    "footprint(removed) area {} bounds {:?}",
                    f.area(),
                    f.bounds()
                );
            }
        }
        _ => panic!("load | build | digest"),
    }
}
