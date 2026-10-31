//! The Swift <-> Rust interface of docs/FFI.md (M4a surface), UniFFI proc-macros.
//!
//! `unsafe` is allowed in this crate only: `GpuBuffer` is a raw page-aligned allocation, and UniFFI's
//! generated scaffolding is `unsafe extern "C"`. Everything else is the engine's safe code; this layer
//! only converts formats and adds no geometry logic.
#![allow(unsafe_code)]

mod bigblocks;
mod builder;
pub mod crash;
mod draw;
mod footprint;
mod gpu;
mod part;
mod plateties;
mod records;
mod report;
mod session;
mod suggest;
mod tristore;

pub use builder::{
    BuildListener, BuildResult, Builder, ExportKind, PlateFile, PlateModel, build_part,
    build_part_cancellable, export, export_file_name, export_plate, export_plates,
    export_with_drawn,
};
pub use crash::{
    crash_breadcrumb, crash_breadcrumbs, crash_demangle, crash_reports_start, crash_set_settings,
    crash_zip,
};
pub use draw::{BraceSpot, DrawKit, DrawnGhost, DrawnSweep, SurfaceSpot, arc_band};
pub use footprint::{
    Footprint, footprint_arrange, footprint_clash_cells, footprint_drag_step, footprint_of_build,
    footprint_of_build_with_drawn, footprint_of_part, footprint_spot, footprint_union, plate_under,
};
pub use gpu::GpuBuffer;
pub use part::{
    AnalysisSummary, FfiError, Island, LimitKind, LoadOptions, Part, PartInfo, island_card,
    load_parts, load_parts_with, load_step_parts, load_step_parts_with, merge_parts, reads_as_step,
    scale_from_percent_text, scale_percent_text,
};
pub use plateties::{
    PlateExport, PrinterProfileInfo, TieBuildResult, TieLink, TieModelInput, TieNoteInfo,
    TieOverrides, TiePairKey, TiePairOrigin, TiePairResult, TiePlate, export_plate_tied,
    export_plates_tied, plate_ties_of_3mf, plate_ties_settings, printer_profile, printer_profiles,
};
pub use report::{BuildReport, build_report};
pub use session::{SessionField, SessionRestore, SessionWall, js_number_text};
pub use suggest::{
    LoadAlignment, StrengthAdvice, StrengthPose, SuggestContext, SuggestListener, SuggestResult,
    SuggestTier, Suggester, Suggestion, load_alignment,
};

uniffi::setup_scaffolding!();

/// The crate version and the pinned upstream commit, e.g. `0.1.0 (upstream 23039e2)`.
#[uniffi::export]
pub fn engine_version() -> String {
    format!(
        "{} (upstream {})",
        env!("CARGO_PKG_VERSION"),
        upstream_pin()
    )
}

/// The pinned upstream commit, seven characters (the `UPSTREAM` file).
pub(crate) fn upstream_pin() -> String {
    let commit = include_str!("../../../UPSTREAM")
        .lines()
        .find_map(|l| l.trim().strip_prefix("commit="))
        .unwrap_or("unknown");
    commit[..commit.len().min(7)].to_string()
}

pub use bigblocks::BigBlocks;

/// The app's allocator (feature `app-alloc`, set by `just app-core`): see [`BigBlocks`].
#[cfg(feature = "app-alloc")]
#[global_allocator]
static ALLOC: BigBlocks = BigBlocks;
