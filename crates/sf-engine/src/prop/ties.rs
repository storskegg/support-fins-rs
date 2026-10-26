//! Port of `web/prop/ties.js`. Read `docs/AGENTS.md` before changing anything here.

use crate::jsnum;

/// `TIE` (web/prop/ties.js:25). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct TieConfig {
    // rise per run: 55deg, steeper than the 45deg a printer holds
    pub slope: f64,
    // mm: a strut's depth, measured straight up
    pub thick: f64,
    /// JS key `minSpan`.
    // mm: closer than this the two walls' feet already meet
    pub min_span: f64,
    /// JS key `maxSpan`.
    // mm: farther than this a strut is a wall of its own
    pub max_span: f64,
    /// JS key `minHeight`.
    // mm: a wall shorter than this stands without help
    pub min_height: f64,
    // ties stop at this share of the lower wall's height
    pub rise: f64,
    /// JS key `perWall`.
    // ties a wall takes part in, nearest neighbours first
    pub per_wall: f64,
    // mm: part clearance sampled this finely along a strut
    pub step: f64,
    // mm: a strut's end runs this far past each wall's centreline -- inside
    // the wall (th/2 = 0.5) square on, never ending flush on its far face
    pub into: f64,
}

impl Default for TieConfig {
    fn default() -> Self {
        TieConfig {
            slope: jsnum::tan((55.0 * std::f64::consts::PI) / 180.0),
            thick: 1.2,
            min_span: 2.0,
            max_span: 12.0,
            min_height: 8.0,
            rise: 0.7,
            per_wall: 2.0,
            step: 0.25,
            into: 0.35,
        }
    }
}
