//! Port of `web/prop/ties.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! TIES -- a zigzag of struts between two neighbouring walls (local issue 036).
//!
//! The fill pass serves a figure with a forest of thin walls, each standing on its
//! own (Matthew 2026-10-08: "all the fins become a mess"). Ribs (brace.js) widen each
//! wall's footing; a tie makes two walls one frame. From the plate at wall A a strut
//! climbs to wall B, the next climbs back to A, and so on: a Warren truss in the
//! vertical plane through both walls. Every strut rises TIE.slope (55deg) -- steeper
//! than the 45deg a printer holds without support -- and each starts on the plate or
//! on the strut before it, so nothing in a tie bridges or hangs.
//!
//! Struts stop under the lower wall's tip taper (1 mm under it, as ribs do) and at
//! TIE.rise of its height, so the contact, the gap and the tines are untouched. Each
//! end runs TIE.into past its wall's centreline (overlapping solids, never flush; met
//! at a slant a corner can show through the far face, so the part clearance is checked
//! out to the ends). A tie is built only
//! where the part stays sideClear off every strut, from both walls' plate stations
//! (v1: no ties landing on the part), and only if `clear(tris)` (the caller's check
//! against the other supports) passes.

use crate::config::Config;
use crate::jsnum;
use crate::prop::surface::surface_zs_at;
use crate::solids::box_extrude;
use crate::tris::Tris;

/// `TIE` (web/prop/ties.js:25). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct TieConfig {
    // rise per run: 55deg, steeper than the 45deg a printer holds
    pub slope: f64,
    // mm: a strut's depth, measured straight up
    pub thick: f64,
    /// JS key `minSpan`.
    // mm: closer than this the two walls' feet already meet
    pub min_span: f64,
    /// JS key `maxSpan`.
    // mm: farther than this a strut is a wall of its own
    pub max_span: f64,
    /// JS key `minHeight`.
    // mm: a wall shorter than this stands without help
    pub min_height: f64,
    // ties stop at this share of the lower wall's height
    pub rise: f64,
    /// JS key `perWall`.
    // ties a wall takes part in, nearest neighbours first
    pub per_wall: f64,
    // mm: part clearance sampled this finely along a strut
    pub step: f64,
    // mm: a strut's end runs this far past each wall's centreline -- inside
    // the wall (th/2 = 0.5) square on, never ending flush on its far face
    pub into: f64,
}

impl Default for TieConfig {
    fn default() -> Self {
        TieConfig {
            slope: jsnum::tan((55.0 * std::f64::consts::PI) / 180.0),
            thick: 1.2,
            min_span: 2.0,
            max_span: 12.0,
            min_height: 8.0,
            rise: 0.7,
            per_wall: 2.0,
            step: 0.25,
            into: 0.35,
        }
    }
}

/// Port of `strutHitsPart` (web/prop/ties.js:38-55): does the part come within sideClear of
/// the strut from (u0, z0) to (u1, z1) (bottom edge; TIE.thick deep), or sit around it?
/// Frame: u from `o` along `d`, v across.
#[allow(clippy::too_many_arguments)]
fn strut_hits_part(
    tris: &Tris,
    o: [f64; 2],
    d: [f64; 2],
    u0: f64,
    z0: f64,
    u1: f64,
    z1: f64,
    cfg: &Config,
) -> bool {
    let tie = &cfg.tie;
    let w = cfg.prop.th / 2.0 + cfg.prop.side_clear;
    let c = cfg.prop.side_clear;
    let nu = jsnum::max(1.0, jsnum::ceil(jsnum::abs(u1 - u0) / tie.step));
    let nv = jsnum::ceil(2.0 * w / tie.step);
    let mut i = 0.0;
    while i <= nu {
        let u = u0 + (u1 - u0) * i / nu;
        let zb = z0 + (z1 - z0) * i / nu;
        let zt = zb + tie.thick;
        let mut k = 0.0;
        while k <= nv {
            let v = -w + 2.0 * w * k / nv;
            let x = o[0] + d[0] * u + d[1] * v;
            let y = o[1] + d[1] * u - d[0] * v;
            let mut above = 0;
            for z in surface_zs_at(tris, x, y) {
                if z >= zb - c && z <= zt + c {
                    return true;
                }
                if z > zt {
                    above += 1;
                }
            }
            if above % 2 != 0 {
                // inside the part
                return true;
            }
            k += 1.0;
        }
        i += 1.0;
    }
    false
}

/// Port of `tieWalls` (web/prop/ties.js:63-96): the tie between plan points `a` (on wall A's
/// line) and `b` (on wall B's), both standing on the plate at `z_bed`, under `z_max` (the lower
/// wall's limit). Appends the struts to `out` and returns how many, or 0 (out untouched) when
/// none fits.
pub fn tie_walls(
    a: [f64; 2],
    b: [f64; 2],
    z_max: f64,
    tris: &Tris,
    z_bed: f64,
    out: &mut Vec<[f64; 3]>,
    cfg: &Config,
) -> usize {
    let tie = &cfg.tie;
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let span = jsnum::hypot(&[dx, dy]);
    if span < tie.min_span || span > tie.max_span {
        return 0;
    }
    let d = [dx / span, dy / span];
    let o = [a[0], a[1]];
    let climb = span * tie.slope; // one strut's rise
    let e = tie.into;
    // the last strut's top, at its far end (TIE.into past the wall), stays under zMax
    let n = jsnum::floor((z_max - z_bed - tie.thick - e * tie.slope) / climb);
    if n < 1.0 {
        return 0;
    }
    let mut struts: Vec<[f64; 4]> = Vec::new();
    let mut s = 0.0;
    while s < n {
        // even struts A -> B, odd B -> A; each starts where the last one ended
        let odd = s % 2.0 != 0.0;
        let (u0, u1) = if odd { (span, 0.0) } else { (0.0, span) };
        let z0 = z_bed + s * climb;
        let z1 = z0 + climb;
        // out to its ends: past each wall's centreline, where a slanted corner can show
        let g = if odd { -1.0 } else { 1.0 };
        let rise = e * tie.slope;
        if strut_hits_part(
            tris,
            o,
            d,
            u0 - g * e,
            jsnum::max(z_bed, z0 - rise),
            u1 + g * e,
            z1 + rise,
            cfg,
        ) {
            return 0;
        }
        struts.push([u0, z0, u1, z1]);
        s += 1.0;
    }
    let h = cfg.prop.th / 2.0;
    for [u0, z0, u1, z1] in &struts {
        let (u0, z0, u1, z1) = (*u0, *z0, *u1, *z1);
        // TIE.into past each wall's centreline (overlapping solids), along the strut's slope
        let sg = jsnum::sign(u1 - u0);
        let e0 = u0 - sg * e;
        let e1 = u1 + sg * e;
        let z_at = |u: f64| z0 + (z1 - z0) * (u - u0) / (u1 - u0);
        let lo = jsnum::max(z_bed, z_at(e0));
        // (u, z) parallelogram, extruded across: frame (u, z, v) right-handed, v = u x z
        let poly = if sg > 0.0 {
            [
                [e0, lo],
                [e1, z_at(e1)],
                [e1, z_at(e1) + tie.thick],
                [e0, lo + tie.thick],
            ]
        } else {
            [
                [e1, z_at(e1)],
                [e0, lo],
                [e0, lo + tie.thick],
                [e1, z_at(e1) + tie.thick],
            ]
        };
        box_extrude(
            &poly,
            -h,
            h,
            |p, q, r| [o[0] + d[0] * p + d[1] * r, o[1] + d[1] * p - d[0] * r, q],
            out,
        );
    }
    struts.len()
}
