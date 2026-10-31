//! Embeds every printer profile in `profiles/printers/*.json` (issue #50): adding a printer is adding
//! a file there, with no code change. Writes `$OUT_DIR/printer_profiles.rs`, a list of
//! `(file name, include_str!(...))` in file-name order.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("profiles/printers");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("profiles/printers")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut out = String::from("&[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let name = f.file_name().unwrap().to_string_lossy();
        writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            f.display().to_string()
        )
        .unwrap();
    }
    out.push(']');
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("printer_profiles.rs");
    std::fs::write(dest, out).unwrap();
}
