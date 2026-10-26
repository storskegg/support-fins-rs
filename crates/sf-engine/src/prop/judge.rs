//! Port of `web/prop/judge.js`. Read `docs/AGENTS.md` before changing anything here.

/// `JUDGE` (web/prop/judge.js:28). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct JudgeConfig {
    // mm past a wall's face (provisional, see above)
    pub cantilever: f64,
    /// JS key `bridgeDeg`.
    // anchors this far apart, seen from the point, bridge it
    pub bridge_deg: f64,
    // direction bins for the nearest anchor each way
    pub sectors: f64,
    // mm2 per sample
    pub sample: f64,
    // What plastic buys: a race (raster.js raceAgainst `price`) swaps to walls that
    // weigh more only when they hold at least this much more overhang per mm3 added
    // (0.01 = 10 mm2 per cm3, ~1.24 g of PLA). PROVISIONAL, Matthew's to set: the
    // cable reel's rim buys 22 mm2/cm3, the lamp shade's 34; a wall for a sliver, ~0.
    pub price: f64,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        JudgeConfig {
            cantilever: 2.0,
            bridge_deg: 135.0,
            sectors: 16.0,
            sample: 1.0,
            price: 0.01,
        }
    }
}
