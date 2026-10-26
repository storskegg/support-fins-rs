//! Port of `web/fins/rows.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M3)

/// `ROWS` (web/fins/rows.js:25). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct RowsConfig {
    // mm between rows (across the heading); the greedy pick spaces them
    pub step: f64,
    /// JS key `zStep`.
    // mm between a row's height hints
    pub z_step: f64,
    /// JS key `zBand`.
    // mm: a row spans the red within this of its height
    pub z_band: f64,
    /// JS key `minPiece`.
    // mm: a failed piece is bisected while its halves are at least this
    pub min_piece: f64,
    /// JS key `minKeep`.
    // mm: a row piece shorter than this is left to the patch lines (at 6 a small
    // figure's short rows went: the guardian lion upright held 86 -> 54%)
    pub min_keep: f64,
    /// JS key `maxRows`.
    // rows searched, heaviest red first
    pub max_rows: f64,
    /// JS key `maxBuilds`.
    // drawnWall calls while searching (a count, not a clock)...
    pub max_builds: f64,
    /// JS key `fullFaces`.
    // ...at up to this many part faces; a denser part gets proportionally fewer
    pub full_faces: f64,
    /// JS key `minBuilds`.
    // (but never under this many)
    pub min_builds: f64,
}

impl Default for RowsConfig {
    fn default() -> Self {
        RowsConfig {
            step: 1.0,
            z_step: 3.0,
            z_band: 6.0,
            min_piece: 3.0,
            min_keep: 3.0,
            max_rows: 160.0,
            max_builds: 900.0,
            full_faces: 250000.0,
            min_builds: 150.0,
        }
    }
}
