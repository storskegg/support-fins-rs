//! Port of `web/prop/pins.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Resin pins (GitHub #309): the 'vertical' tine direction. A resin print is peeled
//! off the vat floor every layer, so a support has to be ATTACHED to the part --
//! a resin slicer's contact tip sinks into the model -- and a wall stopping `gap`
//! under a flat span (the FDM wall) holds nothing. Instead of the horizontal comb
//! (tines.js), each station gets a thin square pin standing on the wall top, up
//! across the gap and PROP.pinDepth into the part above it. Like a resin slicer's
//! support, it narrows across the gap to a PROP.pinTipW contact tip, so it snaps at
//! the part's surface rather than partway down or by tearing a chip out.
//!
//! Every pin is checked, never assumed: its four top corners must be inside the
//! part (so it bites the underside, not its own edge), and a pin that would come
//! out the other side of a thin section sinks shallower or is skipped.
//!
//! Untested on a resin printer; the numbers are resin slicers' tip sizes.

use crate::inside::inside_part;
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::ctx::{BuildCtx, TineSeed};
use crate::prop::tines::TineOpts;
use crate::solids::stack_extrude;
use crate::types::{Rot3, V4, Xyz};

/// `PROBE` (web/prop/pins.js:20): how finely a pin corner looks up for the part's surface.
const PROBE: f64 = 0.05;
/// `LOOK` (web/prop/pins.js:21): and how far: a corner with no part this close above is off the
/// part.
const LOOK: f64 = 2.0;

/// Port of `emitPins` (web/prop/pins.js:28-92). Same arguments as `emit_tines` (tines.rs) --
/// `line` is the wall's settled top contour (each station's z is the part surface above it).
/// Returns how many pins landed. `opts.body` is unused, as in JS.
pub fn emit_pins(
    line: &[[f64; 3]],
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    out: &mut Vec<V4>,
    opts: TineOpts,
    ctx: &BuildCtx,
) -> usize {
    let prop = &ctx.cfg.prop;
    let TineOpts {
        step: step_arg,
        min_top,
        tine_h,
        mut grip,
        ..
    } = opts;
    let step_arg = step_arg.unwrap_or(prop.tine_step);
    let min_top = min_top.unwrap_or(prop.base_h + 0.2);
    if line.len() < 2 {
        return 0;
    }
    let mut s = vec![0.0];
    for i in 1..line.len() {
        s.push(
            s[i - 1] + jsnum::hypot(&[line[i][0] - line[i - 1][0], line[i][1] - line[i - 1][1]]),
        );
    }
    let total = s[s.len() - 1];
    // `!(S > 0)`: also true for NaN
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let none = !(total > 0.0);
    if none {
        return 0;
    }
    // the comb's spacing rules: the user's step, tightened to the grip floor on a short wall
    let step = if prop.tines_per_wall > 0.0 {
        total / prop.tines_per_wall
    } else {
        jsnum::min(step_arg, total / prop.min_grip_tines)
    };
    let half = prop.pin_w / 2.0;
    let inside = |x: f64, y: f64, z: f64| inside_part(topo, rot, offset, x, y, z);

    // the lowest z at or above `z` where (x, y) is inside the part, or null
    let entry = |x: f64, y: f64, z: f64| -> Option<f64> {
        let mut h = 0.0;
        while h <= LOOK {
            if inside(x, y, z + h) {
                return Some(z + h);
            }
            h += PROBE;
        }
        None
    };

    let mut count = 0;
    let mut place = |d: f64| {
        let mut k = 0;
        while k + 2 < s.len() && s[k + 1] < d {
            k += 1;
        }
        let f = (d - s[k]) / jsnum::max(1e-9, s[k + 1] - s[k]);
        let x = line[k][0] + (line[k + 1][0] - line[k][0]) * f;
        let y = line[k][1] + (line[k + 1][1] - line[k][1]) * f;
        let z = line[k][2] + (line[k + 1][2] - line[k][2]) * f; // surface z
        let wall_top = z - prop.gap;
        if wall_top < min_top {
            return;
        }
        // the pin's square, squared to the run like the wall it stands on
        let rx = line[k + 1][0] - line[k][0];
        let ry = line[k + 1][1] - line[k][1];
        // `Math.hypot(rx, ry) || 1`
        let hy = jsnum::hypot(&[rx, ry]);
        let rm = if hy != 0.0 && !hy.is_nan() { hy } else { 1.0 };
        let (ux, uy) = (rx / rm, ry / rm);
        let (vx, vy) = (-uy, ux);
        let corners: Vec<[f64; 2]> = [[-half, -half], [half, -half], [half, half], [-half, half]]
            .iter()
            .map(|&[a, b]| [x + ux * a + vx * b, y + uy * a + vy * b])
            .collect();
        // where each corner meets the part: the pin's top clears the HIGHEST, so all four bite
        let mut meet = f64::NEG_INFINITY;
        let mut low = f64::INFINITY;
        for &[cx, cy] in &corners {
            let Some(e) = entry(cx, cy, wall_top) else {
                return; // a corner off the part's edge: no pin
            };
            meet = jsnum::max(meet, e);
            low = jsnum::min(low, e);
        }
        // sink pinDepth in, shallower where the part is thinner than that; a section too
        // thin for even a quarter of it gets no pin (it would print through)
        let mut top: Option<f64> = None;
        let mut dep = prop.pin_depth;
        while dep >= prop.pin_depth / 4.0 - 1e-9 {
            if corners
                .iter()
                .all(|&[cx, cy]| inside(cx, cy, meet + dep) && inside(cx, cy, meet + dep / 2.0))
            {
                top = Some(meet + dep);
                break;
            }
            dep /= 2.0;
        }
        let Some(top) = top else {
            return;
        };
        // from inside the wall (tineOverlap down, so they union), full width to the wall
        // top, narrowing across the gap to the tip a probe step under the LOWEST corner's
        // meet (the surface is within PROBE below it) -- so on a sloped or curved underside
        // the part touches the pin only where it's tip-wide -- then tip-wide to `top`
        let bot = wall_top - prop.tine_overlap;
        let tip_z = low - PROBE;
        let neck = jsnum::min(wall_top, tip_z - 0.05); // low >= wallTop: neck stays over bot
        let sq = |h: f64| vec![[-h, -h], [h, -h], [h, h], [-h, h]];
        let tip = prop.pin_tip_w / 2.0;
        stack_extrude(
            &[sq(half), sq(half), sq(tip), sq(tip)],
            &[bot, neck, tip_z, top],
            |a, b, c| [x + ux * a + vx * b, y + uy * a + vy * b, c],
            out,
        );
        if let Some(g) = grip.as_deref_mut()
            && bot < *g
        {
            *g = bot;
        }
        ctx.capture(TineSeed {
            x,
            y,
            z: z - tine_h.unwrap_or(prop.tine_h) / 2.0,
            bite_x: 0.0,
            bite_y: 0.0,
        });
        count += 1;
    };
    let mut d = step / 2.0;
    while d < total {
        place(d);
        d += step;
    }
    count
}
