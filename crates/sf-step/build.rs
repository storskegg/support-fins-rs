//! Builds the C++ shim (cpp/step_shim.cpp) against a static OpenCascade 7.6.1 and links it.
//!
//! OpenCascade is found in `$SF_OCCT_DIR`, else `~/.cache/support-fins/occt-7.6.1` (what `just occt`
//! builds, tools/build-occt.sh). Without it the crate still builds with `--cfg no_occt`: `read_step`
//! then says STEP import is not available in this build, and nothing else changes. Without the `occt` feature it is
//! `no_occt` too, quietly: that build (the app's engine) never meant to link OpenCascade.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rustc-check-cfg=cfg(no_occt)");
    println!("cargo:rerun-if-env-changed=SF_OCCT_DIR");
    println!("cargo:rerun-if-changed=cpp/step_shim.cpp");
    println!("cargo:rerun-if-changed=cpp/step_shim.h");
    println!("cargo:rerun-if-changed=build.rs");

    // without the `occt` feature (the app's engine, sf-ffi) nothing links OpenCascade: the StepReader XPC service does
    if std::env::var_os("CARGO_FEATURE_OCCT").is_none() {
        println!("cargo:rustc-cfg=no_occt");
        return;
    }

    let dir = std::env::var_os("SF_OCCT_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache/support-fins/occt-7.6.1"))
        });
    let found = dir.filter(|d| d.join("include/opencascade/Standard_Version.hxx").is_file());
    let Some(dir) = found else {
        println!(
            "cargo:warning=sf-step: OpenCascade 7.6.1 not found (run `just occt`, or set SF_OCCT_DIR): STEP import is disabled in this build"
        );
        println!("cargo:rustc-cfg=no_occt");
        return;
    };
    println!("cargo:rerun-if-changed={}", dir.display());

    // The maths library the web's kernel uses (tools/occt/wasm_math.h): every file that includes OpenCascade
    // headers is compiled with it, and libwasmmath.a (built beside OpenCascade) provides wasm_sin & co.
    let math_h = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/occt/wasm_math.h");
    assert!(
        dir.join("lib/libwasmmath.a").is_file(),
        "{}/lib/libwasmmath.a is missing: rebuild OpenCascade with `just occt`",
        dir.display()
    );
    println!("cargo:rerun-if-changed={}", math_h.display());

    cc::Build::new()
        .cpp(true)
        .flag("-include")
        .flag(math_h.to_str().expect("utf-8 path"))
        .std("c++17")
        .file("cpp/step_shim.cpp")
        .include(dir.join("include/opencascade"))
        .flag_if_supported("-ffp-contract=off")
        .warnings(false)
        .compile("sf_step_shim");

    let lib = dir.join("lib");
    println!("cargo:rustc-link-search=native={}", lib.display());
    let mut libs: Vec<String> = std::fs::read_dir(&lib)
        .expect("OpenCascade lib dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_prefix("lib")
                .and_then(|n| n.strip_suffix(".a"))
                .filter(|n| n.starts_with("TK"))
                .map(str::to_string)
        })
        .collect();
    libs.sort();
    libs.push("wasmmath".into());
    for l in libs {
        println!("cargo:rustc-link-lib=static={l}");
    }
}
