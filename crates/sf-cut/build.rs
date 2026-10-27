//! Builds Manifold 3.5.4 (the submodule at vendor/manifold, tag v3.5.4) and the C shim over it.
//!
//! The arithmetic must be the WASM build's, bit for bit (the upstream `manifold-3d-3.5.4` the JS
//! cutter loads), so this takes the compile flags Manifold's own CMakeLists gives a Release build
//! (`-O3`, `-ffp-contract=off` and `-fexcess-precision=standard`: no fused multiply-add, no
//! extended precision), and builds without the parallel backend (`MANIFOLD_PAR=-1`: no TBB, every
//! algorithm its serial path) and without CrossSection (Clipper2 would be a fetched dependency, and
//! cut.js uses only the 3D solids). No CMake is run, so nothing is fetched.

#![allow(unsafe_code)]

use std::path::PathBuf;

const SRCS: &[&str] = &[
    "boolean3.cpp",
    "boolean_result.cpp",
    "constructors.cpp",
    "csg_tree.cpp",
    "edge_op.cpp",
    "execution_impl.cpp",
    "face_op.cpp",
    "impl.cpp",
    "manifold.cpp",
    "minkowski.cpp",
    "polygon.cpp",
    "properties.cpp",
    "quickhull.cpp",
    "sdf.cpp",
    "smoothing.cpp",
    "sort.cpp",
    "subdivision.cpp",
    "tree2d.cpp",
];

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("vendor/manifold");
    assert!(
        root.join("src/manifold.cpp").exists(),
        "the Manifold submodule is not checked out: run `git submodule update --init`"
    );
    // The arithmetic must not depend on the builder's shell: a CXXFLAGS (here `-std=c++11` has been
    // seen) is appended after ours and would win.
    for v in [
        "CXXFLAGS",
        "CXXFLAGS_aarch64_apple_darwin",
        "CXXFLAGS_x86_64_apple_darwin",
    ] {
        // SAFETY: a build script is single-threaded at this point.
        unsafe { std::env::remove_var(v) };
    }
    let mut b = cc::Build::new();
    b.cpp(true)
        .std("c++17")
        .opt_level(3)
        .debug(false)
        .warnings(false)
        .include(root.join("include"))
        .include(root.join("src"))
        .define("MANIFOLD_PAR", "-1")
        .define("NDEBUG", None)
        .flag_if_supported("-ffp-contract=off")
        .flag_if_supported("-fexcess-precision=standard");
    for s in SRCS {
        b.file(root.join("src").join(s));
        println!(
            "cargo:rerun-if-changed={}",
            root.join("src").join(s).display()
        );
    }
    b.file("src/shim.cpp");
    println!("cargo:rerun-if-changed=src/shim.cpp");
    b.compile("sfmanifold");
}
