//! `cargo build --release -p sf-ffi --example load_time && /usr/bin/time -l target/release/examples/load_time <file>`
//! Times `load_parts` (and the first part's `mesh` and `face_flags`) on a big file.
use std::time::Instant;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: load_time <file.stl|.3mf|.obj>");
    let t = Instant::now();
    let parts = sf_ffi::load_parts(path).unwrap_or_else(|e| panic!("{e}"));
    println!(
        "load_parts: {:.2} s, {} parts, {} triangles",
        t.elapsed().as_secs_f64(),
        parts.len(),
        parts.iter().map(|p| p.info().triangles).sum::<u64>()
    );
    let p = &parts[0];
    let t = Instant::now();
    let mesh = p.mesh();
    println!(
        "part 1 mesh: {:.0} ms, {} MiB",
        t.elapsed().as_secs_f64() * 1e3,
        mesh.len_bytes() >> 20
    );
    let t = Instant::now();
    let rot = vec![1., 0., 0., 0., 1., 0., 0., 0., 1.];
    let flags = p.face_flags(45.0, rot).unwrap();
    println!(
        "part 1 face_flags: {:.0} ms, {} faces",
        t.elapsed().as_secs_f64() * 1e3,
        flags.count()
    );
}
