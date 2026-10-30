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
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let now = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        HIGH.fetch_max(now, Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let now = LIVE.fetch_add(new, Relaxed) + new - l.size();
        LIVE.fetch_sub(l.size(), Relaxed);
        HIGH.fetch_max(now, Relaxed);
        unsafe { System.realloc(p, l, new) }
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
    let file = a.get(2).expect("FILE").clone();
    say("start");
    let parts = load_parts(file).expect("load");
    say("load_parts");
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
        _ => panic!("load | build"),
    }
}
