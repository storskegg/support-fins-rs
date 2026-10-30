//! C-05 / C-07 / C-11: what loading a file and holding a build costs in memory.
//!
//! - C-05: opening a file with several objects reads their soups only. An object is welded when something needs
//!   its topology (its mesh, an analysis, a build), so the objects the picker never picks are never welded.
//! - C-07: a `BuildResult` keeps its fin triangles once as the GPU buffers and once as the engine's `f64`
//!   records (which the removal readouts and the export need), no `f32` copy of the soup and no second clone.
//! - C-11: a finished build leaves no stamp arrays on the Rayon workers.
//!
//! One test, so the counting allocator sees no other test's allocations.
#![allow(unsafe_code)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use sf_ffi::{build_part, load_parts};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, Relaxed);
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn live() -> usize {
    LIVE.load(Relaxed)
}

/// An OBJ of a tilted torus (so it has an underside to support) named `name`, offset along x by `dx`.
fn torus_obj(name: &str, dx: f64, out: &mut String, base: &mut usize) {
    let (u, v) = (120usize, 60usize);
    let tilt = 55f64.to_radians();
    let at = |i: usize, j: usize| {
        let a = (i % u) as f64 / u as f64 * std::f64::consts::TAU;
        let b = (j % v) as f64 / v as f64 * std::f64::consts::TAU;
        let r = 40.0 + 12.0 * b.cos();
        let (x, y, z) = (r * a.cos(), r * a.sin(), 12.0 * b.sin());
        (
            x + dx,
            y * tilt.cos() - z * tilt.sin(),
            y * tilt.sin() + z * tilt.cos(),
        )
    };
    out.push_str(&format!("o {name}\n"));
    for i in 0..u {
        for j in 0..v {
            for (p, q) in [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)] {
                let c = at(p, q);
                out.push_str(&format!("v {:.5} {:.5} {:.5}\n", c.0, c.1, c.2));
            }
        }
    }
    for k in 0..u * v {
        let b = *base + k * 4;
        out.push_str(&format!(
            "f {} {} {}\nf {} {} {}\n",
            b + 1,
            b + 2,
            b + 3,
            b + 1,
            b + 3,
            b + 4
        ));
    }
    *base += u * v * 4;
}

#[test]
fn loading_welds_lazily_and_a_build_keeps_one_copy_of_its_fins() {
    let dir = std::env::temp_dir().join(format!("sf-ffi-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two-tori.obj");
    let (mut obj, mut base) = (String::new(), 0);
    torus_obj("first", 0.0, &mut obj, &mut base);
    torus_obj("second", 200.0, &mut obj, &mut base);
    std::fs::write(&path, obj).unwrap();

    // C-05: the picker's questions (name, triangle count, size) weld nothing
    let parts = load_parts(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(parts.len(), 2);
    for p in &parts {
        let info = p.info();
        assert_eq!(info.triangles, 14_400);
        assert!(info.size.iter().all(|s| *s > 0.0));
        assert!(!p.is_welded(), "info() welded {}", info.name);
    }
    // the picked object is welded by its mesh; the other stays a soup
    let mesh = parts[1].mesh();
    assert_eq!(mesh.count(), 14_400 * 3);
    assert!(parts[1].is_welded() && !parts[0].is_welded());
    assert!(
        parts[1].info().weld_ms > 0.0,
        "the weld's time is reported once it ran"
    );
    drop(mesh);
    // a rescale or a merge is welded before it comes back, whichever way it is reached
    let scaled = parts[0].scaled(1.5).unwrap();
    assert!(scaled.is_welded() && !parts[0].is_welded());
    let merged = sf_ffi::merge_parts(parts.clone(), 2).unwrap();
    assert!(merged.is_welded() && !parts[0].is_welded());
    assert_eq!(merged.info().triangles, 28_800);

    // C-07: a held build is the GPU soup (72 B per triangle), the engine's f64 vertices deduplicated (about 30 B) and the records;
    // it was 224 B per triangle before C-07 (an f32 copy of the soup, a clone of the build, 96 B of f64 vertices)
    let rot = {
        let t = 55f64.to_radians();
        [1.0, 0.0, 0.0, 0.0, t.cos(), t.sin(), 0.0, -t.sin(), t.cos()]
    };
    let opts = r#"{"mode":"full","coverage":1}"#;
    let part = &parts[0];
    part.topology(); // weld first, so the build's own cost is what is measured
    let _warm = build_part(part, &rot, opts, "{}").unwrap();
    let before = live();
    let result = build_part(part, &rot, opts, "{}").unwrap();
    let held = live() - before;
    let tris = (result.fins().count() + result.pad().count()) as usize / 3;
    assert!(tris > 5_000, "a fins-heavy build: {tris} triangles");
    let per_tri = held as f64 / tris as f64;
    eprintln!("a held build: {held} B for {tris} triangles = {per_tri:.0} B per triangle");
    assert!(per_tri < 150.0, "{per_tri:.0} B per fin triangle is held");
    assert_eq!(
        result.triangles().len(),
        tris * 9,
        "the export soup is read back out of the buffers"
    );

    // C-11: no stamp arrays are left on the workers
    assert_eq!(sf_engine::inside::scratch_bytes(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
