//! `cargo build --release -p sf-ffi --example limits_probe`, then per file
//! `/usr/bin/time -l target/release/examples/limits_probe <file> [--allow-large]`.
//! Loads a file the way the app does (on a 512 KiB thread) and says what came back and how long
//! it took; the shell's `time -l` gives the peak RSS. For the hostile inputs of audit X-02..X-07.
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: limits_probe <file> [--allow-large]");
    let allow_large = args.next().as_deref() == Some("--allow-large");
    let t = Instant::now();
    let r = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || sf_ffi::load_parts_with(path, sf_ffi::LoadOptions { allow_large }))
        .unwrap()
        .join();
    let took = t.elapsed();
    match r {
        Err(_) => println!("PANIC after {took:.2?}"),
        Ok(Ok(parts)) => println!(
            "ok: {} part(s), {} triangles, {took:.2?}",
            parts.len(),
            parts.iter().map(|p| p.info().triangles).sum::<u64>()
        ),
        Ok(Err(sf_ffi::FfiError::LimitExceeded {
            kind,
            estimate,
            limit,
            message,
        })) => {
            println!("LIMIT {kind:?} estimate {estimate:e} limit {limit:e} ({message}), {took:.2?}")
        }
        Ok(Err(e)) => println!("error: {e}, {took:.2?}"),
    }
}
