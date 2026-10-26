//! Port of `web/prop/rim.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M2)

/// `RIM` (web/prop/rim.js:32). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct RimConfig {
    /// JS key `pieceLen`.
    // mm: the longest rim piece
    pub piece_len: f64,
    /// JS key `pieceTurn`.
    // deg: the most a piece may turn (a U can't be pulled out)
    pub piece_turn: f64,
    /// JS key `pieceGap`.
    // mm between pieces
    pub piece_gap: f64,
    /// JS key `levelDeg`.
    // deg off horizontal: a rim stands under a LEVEL region (most of its area) (a flange, a shelf -- the
    // "flat + curved edges" class). A sloped patch's rows already run downhill to
    // its edges (the flush end rows, #143/#269); a rim round a tilted face just
    // rings it with walls (tube X25: 8 for 4).
    pub level_deg: f64,
}

impl Default for RimConfig {
    fn default() -> Self {
        RimConfig {
            piece_len: 40.0,
            piece_turn: 90.0,
            piece_gap: 1.5,
            level_deg: 10.0,
        }
    }
}
