//! Port of `web/nozzle.js`. Read `docs/AGENTS.md` before changing anything here.

/// The module `let`s `nozzle` and `wallLines` (web/nozzle.js:42-43).
#[derive(Clone, Debug, PartialEq)]
pub struct NozzleState {
    pub nozzle: f64,
    pub wall_lines: f64,
}

impl Default for NozzleState {
    fn default() -> Self {
        NozzleState {
            nozzle: 0.4,
            wall_lines: 2.0,
        }
    }
}
