//! Port of `web/prop/arc.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M2)

/// `ARC` (web/prop/arc.js:26). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct ArcConfig {
    /// JS key `maxWidth`.
    // mm across the strip: one wall then has every edge within 3 mm (half
    // maxUnsupportedSpan's 6 mm held rule); wider bands get rows
    pub max_width: f64,
    /// JS key `minGap`.
    // deg: the strip must leave at least this much of the circle open --
    // circular.js takes complete loops at gaps up to 30
    pub min_gap: f64,
    /// JS key `minLen`.
    // mm of line: Draw's DRAW_MIN_LEN, a short slot is still a slot
    pub min_len: f64,
    // mm between probes across the strip for its edges (a quarter of `gap`)
    pub probe: f64,
}

impl Default for ArcConfig {
    fn default() -> Self {
        ArcConfig {
            max_width: 6.0,
            min_gap: 30.0,
            min_len: 2.0,
            probe: 0.05,
        }
    }
}
