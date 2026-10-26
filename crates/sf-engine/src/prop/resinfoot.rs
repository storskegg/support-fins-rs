//! Port of `web/prop/resinfoot.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Resin feet (GitHub #309): with Vertical (resin) pins, a wall's flat base flange (and
//! a squat wall's brim) gets windows cut through it. A resin print is cured onto the
//! build plate, and a user found our solid rectangular feet "really stuck on"; Elegoo's
//! slicer cuts a grid of holes in its support base so it flexes when a scraper goes
//! under it.
//!
//! So the flange is a narrow strip under the stem, unbroken (the wall stands on it all
//! the way along), plus full-width pads RESIN_FOOT.pad long with RESIN_FOOT.window of
//! plate between them, on both sides. Each pad is its own closed box overlapping the
//! strip (overlapping solids, never flush). A flange shorter than one pad + window
//! stays solid: there is nothing to flex. Each pad's outer edges are undercut at 45deg:
//! narrow on the plate, full width on top. A resin print hangs upside down, so the lip is
//! an ordinary 45deg overhang off the plate; the tester asked for this way round (the
//! first cut bevelled up from the plate instead), so a scraper's blade slides into the
//! gap under the lip and pries the pad up rather than cutting a thin edge off the plate.

use crate::config::Config;
use crate::jsnum;
use crate::solids::ribbon;

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

/// One entry of `feet` (web/prop/resinfoot.js:27-31): one per station along the wall,
/// `{ p, sx, sy, foot, sh, zBed, baseTop }` -- the station's point, its across-the-wall unit
/// vector, the flange's and the stem's half-widths, and the flange's bottom and top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foot {
    pub p: [f64; 2],
    pub sx: f64,
    pub sy: f64,
    /// the flange's half-width
    pub foot: f64,
    /// the stem's half-width
    pub sh: f64,
    pub z_bed: f64,
    pub base_top: f64,
}

/// Port of `windowedFlange` (web/prop/resinfoot.js:33-55): emits the windowed flange into
/// `out`. Returns false (nothing emitted) when the flange is too short to window.
pub fn windowed_flange(feet: &[Foot], out: &mut Vec<[f64; 3]>, cfg: &Config) -> bool {
    let mut s = vec![0.0];
    for i in 1..feet.len() {
        s.push(
            s[i - 1]
                + jsnum::hypot(&[
                    feet[i].p[0] - feet[i - 1].p[0],
                    feet[i].p[1] - feet[i - 1].p[1],
                ]),
        );
    }
    let l = s[s.len() - 1];
    let ResinFootConfig {
        pad, window, strip, ..
    } = cfg.resin_foot;
    // `!(L >= pad + window + pad)`: true for NaN too
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let too_short = !(l >= pad + window + pad);
    if too_short {
        return false;
    }
    // the strip under the stem, the whole length
    let secs: Vec<Vec<[f64; 3]>> = feet
        .iter()
        .map(|f| sec(f, f.p, jsnum::min(f.foot, f.sh + strip)))
        .collect();
    ribbon(&secs, out);
    // pads spread evenly, one at each end
    let n = jsnum::floor((l - pad) / (pad + window)) + 1.0;
    let pitch = if n > 1.0 { (l - pad) / (n - 1.0) } else { 0.0 };
    let mut k = 0.0;
    while k < n {
        let (ap, af) = at(feet, &s, k * pitch);
        let (bp, bf) = at(feet, &s, k * pitch + pad);
        ribbon(&[pad_sec(af, ap, cfg), pad_sec(bf, bp, cfg)], out);
        k += 1.0;
    }
    true
}

/// `sec` (web/prop/resinfoot.js:58-61): the flange's cross-section at point p, the station
/// f's frame, `half` wide each side.
fn sec(f: &Foot, p: [f64; 2], half: f64) -> Vec<[f64; 3]> {
    let pt = |o: f64, z: f64| [p[0] + f.sx * o, p[1] + f.sy * o, z];
    vec![
        pt(half, f.z_bed),
        pt(half, f.base_top),
        pt(-half, f.base_top),
        pt(-half, f.z_bed),
    ]
}

/// `padSec` (web/prop/resinfoot.js:65-71): a pad's cross-section: full width on top, its outer
/// edges undercut down to no narrower than the strip on the plate.
fn pad_sec(f: &Foot, p: [f64; 2], cfg: &Config) -> Vec<[f64; 3]> {
    let pt = |o: f64, z: f64| [p[0] + f.sx * o, p[1] + f.sy * o, z];
    let bed = jsnum::max(
        jsnum::min(f.foot, f.sh + cfg.resin_foot.strip),
        f.foot - cfg.resin_foot.bevel * (f.base_top - f.z_bed),
    );
    vec![
        pt(bed, f.z_bed),
        pt(f.foot, f.base_top),
        pt(-f.foot, f.base_top),
        pt(-bed, f.z_bed),
    ]
}

/// `at` (web/prop/resinfoot.js:74-81): the point `d` along the wall (clamped to it), with the
/// nearer station's frame.
fn at<'a>(feet: &'a [Foot], s: &[f64], d: f64) -> ([f64; 2], &'a Foot) {
    let d = jsnum::max(0.0, jsnum::min(s[s.len() - 1], d));
    let mut i = 1;
    while i < s.len() - 1 && s[i] < d {
        i += 1;
    }
    let t = if s[i] > s[i - 1] {
        (d - s[i - 1]) / (s[i] - s[i - 1])
    } else {
        0.0
    };
    let a = feet[i - 1].p;
    let b = feet[i].p;
    (
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])],
        if t < 0.5 { &feet[i - 1] } else { &feet[i] },
    )
}
