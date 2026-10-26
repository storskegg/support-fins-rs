//! Port of `web/fins/config.js`. Read `docs/AGENTS.md` before changing anything here.

/// `FIN` (web/fins/config.js:9). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct FinConfig {
    /// JS key `roundFeet`.
    pub round_feet: bool,
    /// JS key `tineH`.
    // --- from docs/FIN-SPEC.md, stated on camera. Do not "tune" these. ---
    // = slicer layer height: a tine must be ONE layer so it prints
    // as a single continuous bead (see prop/config.js tineH / FIN-SPEC)
    pub tine_h: f64,
    /// JS key `padH`.
    // --- ours, derived or measured ---
    // bed pad thickness
    pub pad_h: f64,
    /// JS key `padMargin`.
    // how far the pad's open-bed grip spreads past the part's
    // contact. A part tilted onto an EDGE grips only the OUTBOARD
    // side (inboard the part rises over the pad, which conforms
    // or drops), so the whole hold is one narrow strip. This was
    // 8mm to anchor a near-zero-contact tilted cube on the pad
    // alone, but that made the pad a fat blob on every part;
    // Matthew chose a slimmer, cleaner oval and a slicer brim for
    // the worst tilts instead (the readout says so). Outboard
    // cells sit on open bed at full height, so the margin only
    // adds bed grip -- it can never weld to the part.
    pub pad_margin: f64,
    /// JS key `padSegs`.
    // segments on the smooth-oval pad (open-bed footprint)
    pub pad_segs: f64,
    /// JS key `padMinArea`.
    // mm^2 of bed contact above which no pad is needed
    pub pad_min_area: f64,
    /// JS key `coverExtraSparse`.
    // WIDE-FACE ROW COVERAGE. Density is the maker's call via opts.coverage
    // (0 sparse .. 1 dense), which maps to the wedge row pitch (coverPitch).
    // mm row pitch at coverage 0 (below the neutral default)
    pub cover_extra_sparse: f64,
    /// JS key `coverSparse`.
    // mm row pitch at coverage 0.5 -- the neutral default
    pub cover_sparse: f64,
    /// JS key `coverDense`.
    // mm row pitch at coverage 1
    pub cover_dense: f64,
    /// JS key `coverDefault`.
    // density used when the UI has not set one yet (neutral)
    pub cover_default: f64,
}

impl Default for FinConfig {
    fn default() -> Self {
        FinConfig {
            round_feet: false,
            tine_h: 0.2,
            pad_h: 0.5,
            pad_margin: 4.0,
            pad_segs: 48.0,
            pad_min_area: 60.0,
            cover_extra_sparse: 88.0,
            cover_sparse: 55.0,
            cover_dense: 22.0,
            cover_default: 0.5,
        }
    }
}
