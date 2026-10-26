//! Port of `web/prop/brace.js`. Read `docs/AGENTS.md` before changing anything here.

use crate::prop::config::PropConfig;

/// `BRACE` (web/prop/brace.js:35). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct BraceConfig {
    // braced when height > this x length (Auto's own short-wall limit)
    pub aspect: f64,
    // rib height at the stem, as a share of the wall's height there
    pub rise: f64,
    /// JS key `onPlate`.
    // mm: a station whose floor is this close to the bed stands on the plate
    pub on_plate: f64,
    /// JS key `minReach`.
    // mm: a rib shorter than this is not tried (blocked ribs retry at half reach)
    pub min_reach: f64,
    // mm: part clearance sampled this finely under a rib
    pub step: f64,
    /// JS key `tallH`.
    // mm: a wall taller than this gets face ribs at any length (kinds.tall; tall/ coupon)
    pub tall_h: f64,
    /// JS key `faceStep`.
    // mm: one pair of face ribs per this much wall length
    pub face_step: f64,
}

impl Default for BraceConfig {
    fn default() -> Self {
        BraceConfig {
            aspect: PropConfig::default().max_short_aspect,
            rise: 0.5,
            on_plate: 0.5,
            min_reach: 1.5,
            step: 0.25,
            tall_h: 100.0,
            face_step: 30.0,
        }
    }
}
