//! Port of `web/fins/wedges.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M3)

/// `PERP` (web/fins/wedges.js:50). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct PerpConfig {
    /// JS key `roundFeet`.
    pub round_feet: bool,
    // wedge thickness (thin across the face)
    pub th: f64,
    // breakaway clearance under the contact (matches PROP/FIN)
    pub gap: f64,
    /// JS key `footHalf`.
    // foot flange half-width past the wedge, each side
    pub foot_half: f64,
    /// JS key `footH`.
    pub foot_h: f64,
    // mm between wedges across the face (a ROW, not a wall of plastic)
    pub pitch: f64,
    /// JS key `tStep`.
    // sampling step up the face
    pub t_step: f64,
    /// JS key `minH`.
    // skip a wedge shorter than this
    pub min_h: f64,
    // keep wedges off the very edges of the patch
    pub inset: f64,
    /// JS key `maxRow`.
    // hard cap on wedges per patch, so a wide face never sprays
    pub max_row: f64,
    /// JS key `minArea`.
    // A wedge is for a BROAD leaning face props can't reach. Below this face area
    // (or u-extent) it is a small/curved patch better left to props or draw-mode --
    // wedging it just sprays spikes (the hook's curved arm).
    pub min_area: f64,
    /// JS key `minWidth`.
    pub min_width: f64,
}

impl Default for PerpConfig {
    fn default() -> Self {
        PerpConfig {
            round_feet: false,
            th: 1.2,
            gap: 0.2,
            foot_half: 3.0,
            foot_h: 0.6,
            pitch: 24.0,
            t_step: 1.5,
            min_h: 2.0,
            inset: 2.0,
            max_row: 14.0,
            min_area: 500.0,
            min_width: 22.0,
        }
    }
}
