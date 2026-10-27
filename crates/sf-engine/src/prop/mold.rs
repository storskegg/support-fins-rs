//! Port of `web/prop/mold.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! The floor a PART-ATTACHED wall stands on: `floor_line` reads it under each station, and the
//! bottom a lifted wall gets from it (see sweepBetween).

use crate::config::Config;
use crate::jsnum;
use crate::prop::surface::surface_zs_at;
use crate::tris::Tris;

/// One station of a floor contour: JS `[x, y, z]`, or `[x, y, z, zNeg, zPos]` when the
/// bottom is lifted by `footGap` (the floors under its two sides).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloorPt {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// `[zNeg, zPos]` for the `-w` and `+w` sides (the across-wall `sx` sign); `None` when welded.
    pub sides: Option<[f64; 2]>,
}

/// `{ top, floor }` of `moldLine`, for sweepBetween.
#[derive(Clone, Debug, PartialEq)]
pub struct MoldLine {
    pub top: Vec<[f64; 3]>,
    pub floor: Vec<FloorPt>,
}

/// Port of `floorLine` (web/prop/mold.js:30-81): the floor contour a PART-ATTACHED support
/// stands on: for each station of `top_line`, the HIGHEST part surface strictly below the
/// overhang, or 0 (the plate) where nothing intervenes. The exact mirror of `contourTop`:
/// contourTop looks UP across the tip and takes the LOWEST hit; floorLine looks DOWN across the
/// bottom and takes the HIGHEST hit below the overhang, so the support lands on the part instead
/// of driving to z=0.
///
/// `margin` (default 1.0) keeps the overhang's OWN face from being read as its floor: only
/// surfaces at least `margin` below the contact line count. `mold` (default false) also
/// reads along the wall.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(g > 0)` is true for NaN, as in JS
pub fn floor_line(
    top_line: &[[f64; 3]],
    tris: &Tris,
    margin: f64,
    mold: bool,
    cfg: &Config,
) -> Vec<FloorPt> {
    // Across the bottom's REAL width: the welded tip, or -- for a bottom lifted by
    // footGap (sweepBetween) -- the full th plus footGap past each side. Read across
    // the tip only, a th-wide bottom dug into a floor sloping across the wall; read
    // across th only, it cleared the slope by 0.2 straight down but ~0.05 sideways
    // on a steep one (lbracket X30Y60), close enough to fuse the first layer.
    // With `mold`, it also reads along the wall (below), so the bottom sits on the
    // floor offset by footGap in every direction -- the part, dilated.
    let g = cfg.prop.foot_gap;
    let w = cfg.prop.th / 2.0;
    let t = cfg.prop.tip / 2.0;
    let offs: Vec<f64> = if g > 0.0 {
        vec![-w - g, -w, -t, 0.0, t, w, w + g]
    } else {
        vec![-t, 0.0, t]
    };
    let len = top_line.len();
    let mut bot = Vec::new();
    for i in 0..len {
        let a = top_line[jsnum::max(0.0, i as f64 - 1.0) as usize];
        let b = top_line[jsnum::min((len - 1) as f64, (i + 1) as f64) as usize];
        let rx = b[0] - a[0];
        let ry = b[1] - a[1];
        let h = jsnum::hypot(&[rx, ry]);
        let rn = if h == 0.0 || h.is_nan() { 1.0 } else { h };
        let (sx, sy) = (ry / rn, -rx / rn); // across the wall
        let ceil = top_line[i][2] - margin;
        // molding (moldLine), also footGap ahead and behind along the wall: the
        // bottom keeps its gap from a floor rising along the wall too, not just
        // straight down. Only for the bottom's SHAPE -- the plain reading is what
        // buildPartAttached decides where walls go by (fed this one, it placed a
        // different wall in tube X30Y60, grazing the bore)
        let along: Vec<f64> = if mold && g > 0.0 {
            vec![-g, 0.0, g]
        } else {
            vec![0.0]
        };
        let (ax, ay) = (rx / rn, ry / rn);
        let mut f0 = Vec::new(); // straight down
        let mut fm = Vec::new(); // and molded (with along)
        for &o in &offs {
            let (mut z0, mut zm) = (0.0f64, 0.0f64); // plate fallback
            for &d in &along {
                let x = top_line[i][0] + sx * o + ax * d;
                let y = top_line[i][1] + sy * o + ay * d;
                for zz in surface_zs_at(tris, x, y) {
                    if zz >= ceil {
                        continue; // highest surface below the overhang
                    }
                    if zz > zm {
                        zm = zz;
                    }
                    if d == 0.0 && zz > z0 {
                        z0 = zz;
                    }
                }
            }
            f0.push(z0);
            fm.push(zm);
        }
        // [2] is the plain floor every decision (and sweepBetween's headroom) uses;
        // the molded reading only places the bottom's two sides
        let z = jsnum::max_of(&f0);
        if !(g > 0.0) {
            bot.push(FloorPt {
                x: top_line[i][0],
                y: top_line[i][1],
                z,
                sides: None,
            });
            continue;
        }
        bot.push(FloorPt {
            x: top_line[i][0],
            y: top_line[i][1],
            z,
            sides: Some(side_floors(&fm, &offs, jsnum::max_of(&fm), w)),
        });
    }
    bot
}

/// `MAX_DROP` (web/prop/mold.js:90).
const MAX_DROP: f64 = 2.75;
/// `MAX_HANG` (web/prop/mold.js:90).
const MAX_HANG: f64 = 1.0;

/// Port of `sideFloors` (web/prop/mold.js:91-130): the floor under each side of a lifted
/// bottom, so it can TILT with the part instead of hanging level off the highest point.
/// Each side takes the floor under it and footGap past it; if the part bulges up between
/// them, both rise until the straight bottom edge clears it -- but never past the highest floor:
/// then the high side pins there and only the low one tilts. The tilt reaches at most MAX_DROP
/// across th (~70deg), leaving that corner at most MAX_HANG (+ footGap) over its floor; a side
/// whose floor is further down stays level, so a wall on the brink of a ledge doesn't reach
/// down its face into the air. Returns `[zNeg, zPos]`.
fn side_floors(f: &[f64], offs: &[f64], z: f64, w: f64) -> [f64; 2] {
    let (mut z_n, mut z_p) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (k, &o) in offs.iter().enumerate() {
        if o <= -w {
            z_n = jsnum::max(z_n, f[k]);
        }
        if o >= w {
            z_p = jsnum::max(z_p, f[k]);
        }
    }
    // The tilt stops at MAX_DROP. A side whose floor is only a little past that (a
    // steep slope: bore_bracket's bore) keeps the capped tilt, its corner just over
    // the slope; one whose floor is further -- open to the plate past a ledge edge, or
    // a step well down -- stays LEVEL at z: tilted, it reached into the air, a corner
    // the slicer starts mid-air (Auto built them on the lighthouse X90, gree,
    // artichoke, the #121 M4; local issue 032).
    z_n = if z_n < z - MAX_DROP - MAX_HANG {
        z
    } else {
        jsnum::max(z_n, z - MAX_DROP)
    };
    z_p = if z_p < z - MAX_DROP - MAX_HANG {
        z
    } else {
        jsnum::max(z_p, z - MAX_DROP)
    };
    let mut lift = 0.0f64;
    for (k, &o) in offs.iter().enumerate() {
        if o <= -w || o >= w {
            continue;
        }
        lift = jsnum::max(lift, f[k] - (z_n + (z_p - z_n) * (o + w) / (2.0 * w)));
    }
    // Never ABOVE the level bottom (z, the highest floor): that height is what
    // clearBetween judged, and a side lifted past it rose into the part above in a
    // tight corner (ushape X30Y60 came within 0.004 mm).
    z_n += lift;
    z_p += lift;
    if z_n <= z && z_p <= z {
        return [z_n, z_p];
    }
    // So pin the high side at z and tilt only the low one, as far down as the
    // bulge lets it: an interior sample f at t across (0 at the low side, 1 at the
    // pinned one) needs low * (1 - t) + z * t >= f.
    let pin_p = z_p > z_n;
    let mut low = if pin_p { z_n - lift } else { z_p - lift };
    for (k, &o) in offs.iter().enumerate() {
        if o <= -w || o >= w {
            continue;
        }
        let t = if pin_p {
            (o + w) / (2.0 * w)
        } else {
            (w - o) / (2.0 * w)
        }; // 1 at the pinned side
        low = jsnum::max(low, (f[k] - z * t) / (1.0 - t));
    }
    low = jsnum::min(low, z);
    if pin_p { [low, z] } else { [z, low] }
}

/// `MOLD_STEP` (web/prop/mold.js:149).
const MOLD_STEP: f64 = 0.25;
/// `MOLD_MIN` (web/prop/mold.js:149).
const MOLD_MIN: f64 = 0.1;

/// Port of `moldLine` (web/prop/mold.js:150-171): MOLD a lifted wall's bottom to the part: the
/// stations `floor_line` reads, plus more wherever the floor changes fast between two of them,
/// so the bottom traces the part instead of cutting straight across it. Halves each such gap
/// until the floor (level, and under each side) steps at most MOLD_STEP, down to MOLD_MIN
/// apart (a sheer drop stops there). The new stations sit on the straight line between their
/// neighbours, top included, so only the bottom changes. Welded (footGap 0), the floor is
/// returned as `floor_line` reads it.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(footGap > 0)` is true for NaN, as in JS
pub fn mold_line(top: &[[f64; 3]], tris: &Tris, cfg: &Config) -> MoldLine {
    let mut top = top.to_vec();
    if !(cfg.prop.foot_gap > 0.0) {
        let floor = floor_line(&top, tris, 1.0, false, cfg);
        return MoldLine { top, floor };
    }
    let mut floor = floor_line(&top, tris, 1.0, true, cfg);
    // `[2, 3, 4].map(k => Math.abs(a[k] - b[k]))`: z, then the two side floors
    let jump = |a: &FloorPt, b: &FloorPt| {
        let (sa, sb) = (
            a.sides.unwrap_or([f64::NAN; 2]),
            b.sides.unwrap_or([f64::NAN; 2]),
        );
        jsnum::max_of(&[
            jsnum::abs(a.z - b.z),
            jsnum::abs(sa[0] - sb[0]),
            jsnum::abs(sa[1] - sb[1]),
        ])
    };
    for _pass in 0..8 {
        let mut next: Vec<[f64; 3]> = Vec::new();
        let mut added = 0;
        for i in 0..top.len() {
            next.push(top[i]);
            if i + 1 == top.len() {
                break;
            }
            let (p, q) = (top[i], top[i + 1]);
            if jump(&floor[i], &floor[i + 1]) <= MOLD_STEP {
                continue;
            }
            if jsnum::hypot(&[q[0] - p[0], q[1] - p[1]]) < 2.0 * MOLD_MIN {
                continue;
            }
            next.push([
                (p[0] + q[0]) / 2.0,
                (p[1] + q[1]) / 2.0,
                (p[2] + q[2]) / 2.0,
            ]);
            added += 1;
        }
        if added == 0 {
            break;
        }
        top = next;
        floor = floor_line(&top, tris, 1.0, true, cfg);
    }
    MoldLine { top, floor }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(z: f64) -> Tris {
        Tris::new(vec![
            -20.0, -20.0, z, 20.0, -20.0, z, 20.0, 20.0, z, //
            -20.0, -20.0, z, 20.0, 20.0, z, -20.0, 20.0, z,
        ])
    }

    fn welded() -> Config {
        let mut c = Config::default();
        c.prop.foot_gap = 0.0;
        c
    }

    #[test]
    fn the_floor_is_the_highest_surface_below_the_overhang_or_the_plate() {
        let cfg = welded();
        let top = [[-3.0, 0.0, 8.0], [3.0, 0.0, 8.0]];
        // a floor at 2, with the overhang 6 above: stands on it
        let f = floor_line(&top, &plate(2.0), 1.0, false, &cfg);
        assert_eq!(f.iter().map(|p| p.z).collect::<Vec<_>>(), vec![2.0, 2.0]);
        assert!(f.iter().all(|p| p.sides.is_none())); // welded: no side floors
        // a surface within `margin` of the line is the overhang's own face, not a floor
        let f = floor_line(&top, &plate(7.5), 1.0, false, &cfg);
        assert_eq!(f[0].z, 0.0);
        let f = floor_line(&top, &plate(7.5), 0.25, false, &cfg);
        assert_eq!(f[0].z, 7.5);
        // nothing there: the plate
        let off = [[100.0, 0.0, 8.0]];
        assert_eq!(floor_line(&off, &plate(2.0), 1.0, false, &cfg)[0].z, 0.0);
        assert!(floor_line(&[], &plate(2.0), 1.0, false, &cfg).is_empty());
    }

    #[test]
    fn a_lifted_bottom_gets_a_floor_under_each_side() {
        let cfg = Config::default(); // footGap 0.2
        let top = [[-3.0, 0.0, 8.0], [0.0, 0.0, 8.0], [3.0, 0.0, 8.0]];
        let f = floor_line(&top, &plate(2.0), 1.0, true, &cfg);
        for p in &f {
            assert_eq!((p.z, p.sides), (2.0, Some([2.0, 2.0])));
        }
    }

    #[test]
    fn a_floor_far_below_one_side_stays_level() {
        // z = 2 floor under the +y half only; the -y side is open to the plate, 2 + > MAX_DROP + MAX_HANG
        // would be needed to reach: side floors fall back to level at the highest floor
        let t = Tris::new(vec![
            -20.0, 0.0, 6.0, 20.0, 0.0, 6.0, 20.0, 20.0, 6.0, -20.0, 0.0, 6.0, 20.0, 20.0, 6.0,
            -20.0, 20.0, 6.0,
        ]);
        let cfg = Config::default();
        let top = [[-3.0, 0.0, 12.0], [3.0, 0.0, 12.0]];
        let f = floor_line(&top, &t, 1.0, true, &cfg);
        let [zn, zp] = f[0].sides.unwrap();
        assert!(zn <= f[0].z && zp <= f[0].z);
        assert!((f[0].z - 6.0).abs() < 1e-12);
    }

    #[test]
    fn mold_line_welded_is_the_plain_floor_and_lifted_adds_stations_at_a_step() {
        let top = [[-5.0, 0.0, 9.0], [5.0, 0.0, 9.0]];
        let flat = plate(2.0);
        let m = mold_line(&top, &flat, &welded());
        assert_eq!(m.top, top.to_vec());
        assert_eq!(m.floor, floor_line(&top, &flat, 1.0, false, &welded()));
        // lifted over a flat floor: the floor never steps, no stations are added
        let m = mold_line(&top, &flat, &Config::default());
        assert_eq!(m.top.len(), 2);
        // lifted over a ledge: the floor steps between the stations, which are split down to
        // MOLD_MIN (0.1) apart
        let ledge = Tris::new(vec![
            0.0, -20.0, 3.0, 20.0, -20.0, 3.0, 20.0, 20.0, 3.0, 0.0, -20.0, 3.0, 20.0, 20.0, 3.0,
            0.0, 20.0, 3.0,
        ]);
        let m = mold_line(&top, &ledge, &Config::default());
        assert!(m.top.len() > 2 && m.floor.len() == m.top.len());
        for w in m.top.windows(2) {
            assert!(w[1][0] > w[0][0]); // still in order
        }
        // the new stations lie on the straight line between their neighbours, top included
        assert!(m.top.iter().all(|p| p[1] == 0.0 && p[2] == 9.0));
    }
}
