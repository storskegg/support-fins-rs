//! Port of `web/prop/downhill.js`. Read `docs/AGENTS.md` before changing anything here.

/// `DOWNHILL` (web/prop/downhill.js:21). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct DownhillConfig {
    // mm: the region footprint sampled for its top edge
    pub grid: f64,
    // mm: central-difference step for the slope
    pub h: f64,
    /// JS key `minSlope`.
    // |grad z| under this is level: no downhill to follow
    pub min_slope: f64,
    // mm: the end seeds sit this far in from the top edge's ends
    pub inset: f64,
    // mm: a line that comes this close to an earlier one stops (they merge)
    pub join: f64,
    /// JS key `maxSteps`.
    pub max_steps: f64,
    /// JS key `maxLines`.
    // Curved walls' coverage seeds stop at this many lines under one surface
    pub max_lines: f64,
    /// JS key `maxGrow`.
    // a region swaps to downhill walls only if they weigh at most this x its walls...
    pub max_grow: f64,
    // ...and hold this share more than they do (raster needs RACE_REL, 10%: the
    // #25 fixture's curved end is +9.8%, and the weight cap guards the rest)
    pub race: f64,
}

impl Default for DownhillConfig {
    fn default() -> Self {
        DownhillConfig {
            grid: 1.0,
            h: 0.5,
            min_slope: 0.05,
            inset: 1.0,
            join: 3.0,
            max_steps: 600.0,
            max_lines: 60.0,
            max_grow: 1.5,
            race: 0.03,
        }
    }
}
