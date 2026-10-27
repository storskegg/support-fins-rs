//! Port of `web/nozzle.js`. Read `docs/AGENTS.md` before changing anything here.

//!
//! Nozzle size: every support width was tuned for a 0.4 mm nozzle -- a 1.0 mm wall
//! (two 0.5 mm lines), a 0.6 mm contact tip, 0.5 mm tines (one line). On a 0.6 or
//! 0.8 nozzle the slicer drops a tine that thin or lays a wall as one weak line, so
//! the widths scale by nozzle / 0.4. Heights don't: tine height, the gaps and the
//! pad follow the layer height, which has its own field.
//!
//! Two rules. The one-line features (tines, contact tip) scale both ways: a finer
//! nozzle prints a finer tine and leaves a smaller mark. The STRUCTURAL widths (the
//! wall, the wedge, the sway brace rib) only grow: a 0.2 nozzle doesn't make a
//! free-standing wall any stiffer, so they never go below their 0.4 numbers.
//!
//! Wall lines (2 / 4 / 6 / 8) thicken the structural widths again, for a tall part
//! or a big printer: 4 lines is twice the wall. The tip and tines stay one line, so
//! the contact breaks off the same. A thicker wall keeps its foot's margin past its
//! face and necks into the tip no flatter than 55deg. Based on Per Hjort's
//! nozzle profiles (GitHub #210).
//!
//! The page sets these once per change; the Worker gets them in opts.tunables.nozzle
//! and .wallLines (fins.js applyTunables), like the gap. 0.4 at 2 lines leaves every
//! number as it was.

use crate::config::Config;
use crate::jsnum;

/// `NOZZLE_REF` (web/nozzle.js:31).
pub const NOZZLE_REF: f64 = 0.4;
/// `NOZZLE_MAX` (web/nozzle.js:32).
pub const NOZZLE_MAX: f64 = 2.0;
/// `TH_MAX` (web/nozzle.js:37). The widest wall: under half the row spacing
/// (`PROP.maxUnsupportedSpan`, 12 mm), so a row's walls and their feet keep air between
/// them. A 2.5 nozzle's two-line wall (6.25) or a 2.0 at four lines (10) would fuse the
/// rows into one block.
pub const TH_MAX: f64 = 6.0;
/// `WALL_LINES` (web/nozzle.js:38).
pub const WALL_LINES: [f64; 4] = [2.0, 4.0, 6.0, 8.0];

/// `BASE` (web/nozzle.js:41-48): the 0.4 numbers, captured before anything scales them.
/// JS captures them at import, which is `Config::default()` here.
struct Base {
    th: f64,
    tip: f64,
    tine_w: f64,
    tip_h: f64,
    foot_min: f64,
    foot_max: f64,
    squat_brim_w: f64,
    edge_inset: f64,
    perp_th: f64,
    sway_tine_w: f64,
    sway_th_min: f64,
    sway_th_max: f64,
    sway_th_per_mm: f64,
    wall_half: f64,
}

impl Base {
    fn capture() -> Base {
        let c = Config::default();
        Base {
            th: c.prop.th,
            tip: c.prop.tip,
            tine_w: c.prop.tine_w,
            tip_h: c.prop.tip_h,
            foot_min: c.prop.foot_min,
            foot_max: c.prop.foot_max,
            squat_brim_w: c.prop.squat_brim_w,
            edge_inset: c.prop.edge_inset,
            perp_th: c.perp.th,
            sway_tine_w: c.sway.tine_w,
            sway_th_min: c.sway.th_min,
            sway_th_max: c.sway.th_max,
            sway_th_per_mm: c.sway.th_per_mm,
            wall_half: c.sway.wall_half,
        }
    }
}

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

/// Port of `applyNozzle` (web/nozzle.js:53-57): scale the support widths for this nozzle
/// diameter (mm). Junk is ignored.
pub fn apply_nozzle(cfg: &mut Config, d: f64) {
    if !d.is_finite() || d < 0.1 || d > NOZZLE_MAX {
        return;
    }
    cfg.nozzle.nozzle = d;
    rescale(cfg);
}

/// Port of `wallLinesFit` (web/nozzle.js:60): does an `n`-line wall fit under `TH_MAX`
/// at nozzle `d`? The page offers only these. JS defaults `d` to the current nozzle
/// (`cfg.nozzle.nozzle`); callers pass it explicitly.
pub fn wall_lines_fit(n: f64, d: f64) -> bool {
    Base::capture().th * jsnum::max(1.0, d / NOZZLE_REF) * n / 2.0 <= TH_MAX + 1e-9
}

/// Port of `applyWallLines` (web/nozzle.js:63-67): lines across a wall: 2 (the tuned
/// wall), 4, 6 or 8. Anything else is ignored.
pub fn apply_wall_lines(cfg: &mut Config, n: f64) {
    if !WALL_LINES.contains(&n) {
        return;
    }
    cfg.nozzle.wall_lines = n;
    rescale(cfg);
}

/// Port of `rescale` (web/nozzle.js:69-98).
pub fn rescale(cfg: &mut Config) {
    let base = Base::capture();
    let k = cfg.nozzle.nozzle / NOZZLE_REF;
    let grow = jsnum::max(1.0, k);
    let thick = grow * cfg.nozzle.wall_lines / 2.0;
    cfg.prop.tip = base.tip * k;
    cfg.prop.tine_w = base.tine_w * k;
    cfg.sway.tine_w = base.sway_tine_w * k;
    cfg.prop.th = jsnum::min(base.th * thick, TH_MAX);
    cfg.perp.th = jsnum::min(base.perp_th * thick, TH_MAX);
    // A brace rib is never thinner than the wall; its height rule is the 0.4 one, grown.
    cfg.sway.th_min = jsnum::max(base.sway_th_min * grow, cfg.prop.th);
    cfg.sway.th_max = jsnum::max(base.sway_th_max * grow, cfg.prop.th);
    cfg.sway.th_per_mm = base.sway_th_per_mm * grow; // same height to reach thMax
    // The braces' keep-clear half-width assumes the widest wall's half (PERP's):
    // a wider wedge needs the braces that much further off.
    cfg.sway.wall_half = base.wall_half + (cfg.perp.th - base.perp_th) / 2.0;
    // The feet are half-widths from the wall's centre: a wider wall would swallow them,
    // so they move out with its face.
    let wider = (cfg.prop.th - base.th) / 2.0;
    cfg.prop.foot_min = base.foot_min + wider;
    cfg.prop.foot_max = base.foot_max + wider;
    // ...but a brim stays under half the row spacing, or neighbouring rows' brims fuse.
    cfg.prop.squat_brim_w = jsnum::min(
        base.squat_brim_w + wider,
        (cfg.prop.max_unsupported_span - 1.0) / 2.0,
    );
    // The neck into the tip: a 4 mm wall necking to 0.6 over the 1.5 mm taper would
    // overhang. A neck that would lean flatter than 55deg gets the height to stand at 55.
    let neck = (cfg.prop.th - cfg.prop.tip) / 2.0;
    cfg.prop.tip_h = jsnum::max(
        base.tip_h,
        neck * jsnum::tan(cfg.prop.neck_deg * std::f64::consts::PI / 180.0),
    );
    // A tine looks for the part from the wall's centre, past the contact tip: a tip wider
    // than the 0.4 nozzle's (0.6) would swallow the 0.5 reach, and on a curved underside
    // the probe stops finding the part (a sphere at 45deg: 30 tines at 0.4, 6 at 2.0).
    // The extra half-tip is added to the reach (prop/tines.js); 0.4 and finer add nothing.
    cfg.prop.tine_reach_tip = jsnum::max(0.0, (cfg.prop.tip - base.tip) / 2.0);
    // A free edge's outermost row stands in from the edge by the 0.4 tip's half and a
    // bit: at 2.0 the 3 mm tip and its 2.5 mm tines hung 1.2 mm past a cube's edge,
    // tabs out of the side instead of under the part. It steps in as the tip widens.
    cfg.prop.edge_inset = base.edge_inset + cfg.prop.tine_reach_tip;
}
