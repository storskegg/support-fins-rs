//! Port of `web/prop/resinfoot.js`. Read `docs/AGENTS.md` before changing anything here.

/// `RESIN_FOOT` (web/prop/resinfoot.js:20). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct ResinFootConfig {
    // mm: a full-width pad along the wall
    pub pad: f64,
    // mm: the plate left bare between two pads
    pub window: f64,
    // mm: the strip reaches this far past the stem on each side
    pub strip: f64,
    // the pad's outer edge undercuts this many mm per mm of height (45deg)
    pub bevel: f64,
}

impl Default for ResinFootConfig {
    fn default() -> Self {
        ResinFootConfig {
            pad: 1.5,
            window: 2.0,
            strip: 0.4,
            bevel: 1.0,
        }
    }
}
