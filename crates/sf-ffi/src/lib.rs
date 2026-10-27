//! The Swift <-> Rust interface of docs/FFI.md (M4a surface), UniFFI proc-macros.
//!
//! `unsafe` is allowed in this crate only: `GpuBuffer` is a raw page-aligned allocation, and UniFFI's
//! generated scaffolding is `unsafe extern "C"`. Everything else is the engine's safe code; this layer
//! only converts formats and adds no geometry logic.
#![allow(unsafe_code)]

mod gpu;
mod part;

pub use gpu::GpuBuffer;
pub use part::{AnalysisSummary, FfiError, Part, PartInfo, load_parts};

uniffi::setup_scaffolding!();

/// The crate version and the pinned upstream commit, e.g. `0.1.0 (upstream 299ef49)`.
#[uniffi::export]
pub fn engine_version() -> String {
    let commit = include_str!("../../../UPSTREAM")
        .lines()
        .find_map(|l| l.trim().strip_prefix("commit="))
        .unwrap_or("unknown");
    format!(
        "{} (upstream {})",
        env!("CARGO_PKG_VERSION"),
        &commit[..commit.len().min(7)]
    )
}
