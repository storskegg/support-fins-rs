//! Times what the app calls for each pose during a ring drag: `analyze` and `face_flags` (which runs its own
//! `analyze`) on a file. `cargo run --release -p sf-ffi --example analyze_time -- file.stl [runs]`.
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a file");
    let runs: usize = args.next().map_or(7, |r| r.parse().unwrap());
    let t = Instant::now();
    let parts = sf_ffi::load_parts(path).unwrap();
    println!("load: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let part = &parts[0];
    println!("triangles: {}", part.info().triangles);
    let mut analyze = vec![];
    let mut flags = vec![];
    let mut both = vec![];
    for i in 0..runs {
        let a = 0.2 * i as f64;
        let (c, s) = (a.cos(), a.sin());
        let rot = vec![c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0];
        let t = Instant::now();
        part.analyze(45.0, rot.clone()).unwrap();
        analyze.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        part.face_flags(45.0, rot.clone()).unwrap();
        flags.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        part.face_flags(45.0, rot.clone()).unwrap();
        part.analyze(45.0, rot).unwrap();
        both.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    println!("analyze median {:.1} ms", med(&mut analyze));
    println!("face_flags median {:.1} ms", med(&mut flags));
    println!(
        "face_flags + analyze (the app's pair) median {:.1} ms",
        med(&mut both)
    );
}
