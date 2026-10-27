//! Port of `web/prop/curved.js`. Read `docs/AGENTS.md` before changing anything here.

/// `CURVED` (web/prop/curved.js:15). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedConfig {
    /// JS key `minBend`.
    // deg: a wall that turns less than this in plan is straight. Straight
    // downhill walls read 0 on real organic parts; real bends run 10-60
    pub min_bend: f64,
    // mm: the heading is read over this much of the line, not one station
    pub chord: f64,
    /// JS key `fullSlack`.
    // Full coverage keeps the curves if they hold within this share of the
    // straight build (spiral -0.8%, bust X30 -0.3% kept; bean on its side -2..-5% not)
    pub full_slack: f64,
    // the share of headings dropped at each extreme: one wandering chord
    // (where the line starts off the climb) doesn't read as a bend
    pub trim: f64,
}

impl Default for CurvedConfig {
    fn default() -> Self {
        CurvedConfig {
            min_bend: 10.0,
            chord: 3.0,
            full_slack: 0.01,
            trim: 0.1,
        }
    }
}
