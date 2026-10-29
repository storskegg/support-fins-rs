//! `sf-cut`: a slicer's negative parts, cut out of the part (port of web/cut.js, GitHub #104), on
//! Manifold 3.5.4 built from source with the WASM build's arithmetic (see build.rs).

mod cut;
mod ffi;

pub use cut::{CutResult, cut_negatives, cut_negatives_with};
