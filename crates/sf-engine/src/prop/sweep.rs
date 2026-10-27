//! Port of `web/prop/sweep.js`. Read `docs/AGENTS.md` before changing anything here.

use crate::config::Config;

/// `footFor(h)` (web/prop/sweep.js:30). Fixed signature: tracks A and C call it.
pub fn foot_for(h: f64, cfg: &Config) -> f64 {
    let _ = (h, cfg);
    todo!("M2 track B")
}

/// `profileHalf(z, top)` (web/prop/sweep.js:44). Fixed signature: track C calls it.
pub fn profile_half(z: f64, top: f64, cfg: &Config) -> f64 {
    let _ = (z, top, cfg);
    todo!("M2 track B")
}

/// `stemHalf(rise)` (web/prop/sweep.js:63). Fixed signature: track B's squat calls it.
pub fn stem_half(rise: f64, cfg: &Config) -> f64 {
    let _ = (rise, cfg);
    todo!("M2 track B")
}
