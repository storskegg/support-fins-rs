//! Port of `web/prop/curved.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Curved walls (issue #25): the "Curved walls" checkbox. Downhill walls (downhill.js)
//! follow a curved overhang down to the plate, but they race the straight walls on
//! held area with a weight cap (DOWNHILL.maxGrow / race), and on a curved part the
//! arcs hold about what the straight walls do at more plastic -- so they lose and
//! the curve never shows (spiral ramp: arcs 1.5x the weight). With the box ticked,
//! downhill walls are tried under every region, and a region whose downhill walls
//! bend always swaps to them, at any weight and whatever they hold: the user has
//! chosen the curve. The normal walls that hold what the curve misses, or grip
//! lower, stay beside it (raster.js keepers), and any overhang still bare shows red.
//! The curves trace each region grown across the slivers around it (trace.js), so a
//! jagged overhang's islands don't cut them short.

use crate::config::Config;
use crate::jsnum;

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

/// Port of `lineBend` (web/prop/curved.js:30-52): how far a wall's top line turns in plan
/// (deg): the spread of its headings, each read over `chord` mm of line (stations needn't be
/// evenly spaced), unwrapped along the line, with `trim` of them dropped at each end of the
/// range. 0 for a line under two chords long: too short to tell a bend from a wobble. Only
/// x and y of each station are read.
pub fn line_bend(line: &[[f64; 3]], cfg: &Config) -> f64 {
    let (chord, trim) = (cfg.curved.chord, cfg.curved.trim);
    let mut heads: Vec<f64> = Vec::new();
    let mut j = 0;
    let mut prev: Option<f64> = None;
    for i in 0..line.len() {
        if j < i {
            j = i;
        }
        while j < line.len()
            && jsnum::hypot(&[line[j][0] - line[i][0], line[j][1] - line[i][1]]) < chord
        {
            j += 1;
        }
        if j == line.len() {
            break;
        }
        let mut h = jsnum::atan2(line[j][1] - line[i][1], line[j][0] - line[i][0]);
        if let Some(p) = prev {
            // unwrap
            h = p + jsnum::atan2(jsnum::sin(h - p), jsnum::cos(h - p));
        }
        heads.push(h);
        prev = Some(h);
    }
    let mut run = 0.0;
    for i in 1..line.len() {
        run += jsnum::hypot(&[line[i][0] - line[i - 1][0], line[i][1] - line[i - 1][1]]);
    }
    if run < 2.0 * chord || heads.is_empty() {
        return 0.0;
    }
    // `(a, b) => a - b`: a NaN difference counts as equal
    heads.sort_by(|a, b| {
        let d = a - b;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let cut = jsnum::floor(trim * heads.len() as f64);
    // an index outside the array is `undefined` in JS, so the sum is NaN
    let at = |i: f64| {
        if i >= 0.0 && i < heads.len() as f64 {
            heads[i as usize]
        } else {
            f64::NAN
        }
    };
    ((at(heads.len() as f64 - 1.0 - cut) - at(cut)) * 180.0) / std::f64::consts::PI
}

/// Port of `bends` (web/prop/curved.js:55): does any of a region's walls bend past
/// `CURVED.minBend`? raceAgainst's `lenient`. `lines` are the walls' `q.line ?? []`.
pub fn bends<'a>(lines: impl IntoIterator<Item = &'a [[f64; 3]]>, cfg: &Config) -> bool {
    lines
        .into_iter()
        .any(|line| line_bend(line, cfg) > cfg.curved.min_bend)
}
