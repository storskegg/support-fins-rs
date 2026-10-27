//! Port of `web/prop/brace.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! BRACES -- buttresses for a short wall that stands tall on the plate (local issue 036).
//!
//! The fill pass serves a figure's hands, hems and elbows with walls 3 mm long and
//! 25-50 mm tall: up to 17x as tall as they are long. Along the line nothing reaches
//! past the wall's own 3 mm, and a part-attached wall that runs down to the plate has
//! no flange at all (Isaac's hands: 33 mm on a 0.4 x 1 mm footprint). PR #200 capped
//! the ratio -- lengthening or refusing the wall -- and left the hands bare; Matthew
//! 2026-10-07: keep the wall where the red is and make it stand.
//!
//! So a wall over BRACE.aspect x its length gets up to four ribs at the plate: one off
//! each end along its line, one off each face at its middle. A rib is a triangle in its
//! own vertical plane, the wall's thickness, rising BRACE.rise of the wall's height at
//! the stem and running out to the plate `PROP.footRatio x height` away -- the same
//! rule the foot already follows across a plate wall. Under it a flange strip, the
//! foot's height and footMin either side, grips the plate. Ribs stop 1 mm under the tip
//! taper, so the contact, the gap and the tines are untouched. New print numbers: rise
//! and minReach (docs/FIN-SPEC.md, Braces).
//!
//! A rib is built only from a station whose floor is the plate (v1: no ribs landing on
//! the part -- they would mark it), only where the part stays sideClear off it, and
//! only if `clear(tris)` (the caller's check against supports already built) passes.
//! Each rib is judged alone: a wall tucked against a thigh still gets its open sides.
//!
//! A wall taller than BRACE.tallH (local issue 046: "fins taller than 10 cm wobble") gets
//! face ribs whatever its length when the caller asks for them (`kinds.tall`, the Brace
//! tall walls box): a pair, one off each face, every BRACE.faceStep along it. A long
//! wall is stiff along its line; across it, it is a 1 mm sheet. tallH is the reporter's
//! 10 cm until the tall/ coupon prints.

use crate::config::Config;
use crate::jsnum;
use crate::prop::config::PropConfig;
use crate::prop::surface::surface_zs_at;
use crate::solids::box_extrude;
use crate::tris::Tris;

/// `BRACE` (web/prop/brace.js:35). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct BraceConfig {
    // braced when height > this x length (Auto's own short-wall limit)
    pub aspect: f64,
    // rib height at the stem, as a share of the wall's height there
    pub rise: f64,
    /// JS key `onPlate`.
    // mm: a station whose floor is this close to the bed stands on the plate
    pub on_plate: f64,
    /// JS key `minReach`.
    // mm: a rib shorter than this is not tried (blocked ribs retry at half reach)
    pub min_reach: f64,
    // mm: part clearance sampled this finely under a rib
    pub step: f64,
    /// JS key `tallH`.
    // mm: a wall taller than this gets face ribs at any length (kinds.tall; tall/ coupon)
    pub tall_h: f64,
    /// JS key `faceStep`.
    // mm: one pair of face ribs per this much wall length
    pub face_step: f64,
}

impl Default for BraceConfig {
    fn default() -> Self {
        BraceConfig {
            aspect: PropConfig::default().max_short_aspect,
            rise: 0.5,
            on_plate: 0.5,
            min_reach: 1.5,
            step: 0.25,
            tall_h: 100.0,
            face_step: 30.0,
        }
    }
}

/// Port of `ribHitsPart` (web/prop/brace.js:46-60): does part surface come within sideClear of
/// the box u0..u1 x -w..w x z0..z1(u) (rib frame: u out from the stem, v across it)?
#[allow(clippy::too_many_arguments)]
fn rib_hits_part(
    tris: &Tris,
    o: [f64; 2],
    d: [f64; 2],
    u0: f64,
    u1: f64,
    w: f64,
    z0: f64,
    z1: impl Fn(f64) -> f64,
    cfg: &Config,
) -> bool {
    let nu = jsnum::ceil((u1 - u0) / cfg.brace.step);
    let nv = jsnum::ceil(2.0 * w / cfg.brace.step);
    let mut i = 0.0;
    while i <= nu {
        let u = u0 + (u1 - u0) * i / nu;
        let mut k = 0.0;
        while k <= nv {
            let v = -w + 2.0 * w * k / nv;
            let x = o[0] + d[0] * u + d[1] * v;
            let y = o[1] + d[1] * u - d[0] * v;
            for z in surface_zs_at(tris, x, y) {
                if z >= z0 - 0.01 && z <= z1(u) + cfg.prop.side_clear {
                    return true;
                }
            }
            k += 1.0;
        }
        i += 1.0;
    }
    false
}

/// Port of `rib` (web/prop/brace.js:66-88): one rib from station point `o` (on the plate, at
/// the stem's centre) in unit plan direction `d`: `stem` is the rib's height there, `reach` how
/// far out it lands. Its triangles, or `None` when the part is in the way. JS's `th` default is
/// `PROP.th`: callers pass it explicitly.
#[allow(clippy::too_many_arguments)]
fn rib(
    tris: &Tris,
    o: [f64; 2],
    d: [f64; 2],
    z_bed: f64,
    stem: f64,
    reach: f64,
    th: f64,
    cfg: &Config,
) -> Option<Vec<[f64; 3]>> {
    let prop = &cfg.prop;
    let u0 = -th / 2.0; // starts inside the stem: overlapping solids
    let fw = prop.foot_min + (th - prop.th) / 2.0; // keep the flange's margin past the body
    let z1 = |u: f64| z_bed + stem * jsnum::max(0.0, 1.0 - jsnum::max(0.0, u) / reach);
    // out to sideClear past the tip: a part resting on the plate just beyond where a rib
    // lands would otherwise meet its flange end-on (review: 0.05 mm). From the stem's
    // centre, not behind it: u < 0 is inside the wall's own body, and part beside that is
    // the wall's clearance, not the rib's -- checking it refused every rib on Isaac's
    // right-hand wall (it stands 0.6 mm off the boot at the plate)
    let lo = 0.0;
    let hi = reach + prop.side_clear;
    if rib_hits_part(
        tris,
        o,
        d,
        lo,
        hi,
        th / 2.0 + prop.side_clear,
        z_bed,
        z1,
        cfg,
    ) {
        return None;
    }
    if rib_hits_part(
        tris,
        o,
        d,
        lo,
        hi,
        fw + prop.side_clear,
        z_bed,
        |_| z_bed + prop.base_h,
        cfg,
    ) {
        return None;
    }
    let mut out = Vec::new();
    // the rib: (u, z) triangle, extruded across. Frame (u, z, v) right-handed: v = u x z.
    box_extrude(
        &[[u0, z_bed], [reach, z_bed], [u0, z_bed + stem]],
        -th / 2.0,
        th / 2.0,
        |a, b, c| [o[0] + d[0] * a + d[1] * c, o[1] + d[1] * a - d[0] * c, b],
        &mut out,
    );
    // its flange: (u, v') rectangle, extruded up. Frame (u, v', z): v' = z x u.
    box_extrude(
        &[[u0, -fw], [reach, -fw], [reach, fw], [u0, fw]],
        z_bed,
        z_bed + prop.base_h,
        |a, b, c| [o[0] + d[0] * a - d[1] * b, o[1] + d[1] * a + d[0] * b, c],
        &mut out,
    );
    Some(out)
}

/// Port of `sheared` (web/prop/brace.js:94-105): the part sheared into a brace's lean. JS keeps
/// the last one in a module `let` (`shear`), keyed on the identity of `tris`, `zBed` and the
/// lean, for the next call with the same lean (both arms of a brace and its nudged retries
/// share it). That cache only saves a copy of a big part and the values are identical either
/// way, so this returns a fresh local copy every call.
fn sheared(tris: &Tris, z_bed: f64, lean: [f64; 2]) -> Tris {
    let mut local = tris.v.clone();
    let mut i = 0;
    while i < local.len() {
        let z = local[i + 2] - z_bed;
        local[i] -= lean[0] * z;
        local[i + 1] -= lean[1] * z;
        i += 3;
    }
    Tris::new(local)
}

/// The `{ reach, th }` `braceCrossRib` returns for the rib it built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrossRib {
    pub reach: f64,
    pub th: f64,
}

/// Port of `braceCrossRib` (web/prop/brace.js:121-137): one CROSS rib for a sway brace
/// (web/sway.js): the same rib, stood across the brace's outer edge in unit plan direction `d`,
/// so the thin plate -- deep toward the face, barely a bead across it -- is widened on the
/// plate in the one direction it is weak. Two of them, one each way along the side, make the
/// cross-braced foot the "Brace tall walls" box asks for.
///
/// `o` is the base point on the brace's outer edge, `height` the brace's own height: the rib
/// rises `rise` of it at the stem and lands footRatio x it out (never shorter than
/// BRACE.minReach), stopping short of the brace's top. Only the horizontal reach is capped at
/// the brace's base depth (`max_reach`). `lean` is the outer edge's XY shift per mm of height;
/// part checks run in the inverse-sheared frame. `th` is this brace's actual body gauge;
/// ordinary wall ribs keep PROP.th. Returns the rib it built, or `None` when nothing fitted.
///
/// JS defaults: `clear = () => true`, `rise = BRACE.rise`, `th = PROP.th`; callers pass them.
#[allow(clippy::too_many_arguments)]
pub fn brace_cross_rib(
    out: &mut Vec<[f64; 3]>,
    tris: &Tris,
    o: [f64; 2],
    d: [f64; 2],
    z_bed: f64,
    height: f64,
    max_reach: f64,
    lean: [f64; 2],
    clear: impl Fn(&[[f64; 3]]) -> bool,
    rise: f64,
    th: f64,
    cfg: &Config,
) -> Option<CrossRib> {
    let brace = &cfg.brace;
    let ratio = if rise.is_finite() {
        jsnum::max(0.1, jsnum::min(0.9, rise))
    } else {
        brace.rise
    };
    let stem = jsnum::min(ratio * height, height - 1.0);
    if stem < cfg.prop.base_h + 1.0 {
        return None;
    }
    let reach0 = jsnum::max(
        brace.min_reach,
        jsnum::min(cfg.prop.foot_ratio * height, max_reach),
    );
    let local = sheared(tris, z_bed, lean);
    let mut reach = reach0;
    while reach >= brace.min_reach {
        if let Some(r) = rib(&local, o, d, z_bed, stem, reach, th, cfg) {
            let world: Vec<[f64; 3]> = r
                .iter()
                .map(|v| {
                    let z = v[2] - z_bed;
                    [v[0] + lean[0] * z, v[1] + lean[1] * z, v[2]]
                })
                .collect();
            if clear(&world) {
                out.extend(world);
                return Some(CrossRib { reach, th });
            }
        }
        reach /= 2.0;
    }
    None
}

/// Which rules `braceWall` may fire (`kinds`): `slim` (over BRACE.aspect x its length: ends and
/// a face pair) and `tall` (over BRACE.tallH: face pairs along it). JS's default is
/// `{ slim: true }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BraceKinds {
    pub slim: bool,
    pub tall: bool,
}

/// Port of `braceWall` (web/prop/brace.js:146-174): brace the wall whose contact line is `top`
/// (stations `[x, y, z]`, z the part above) and whose bottom at station i is `floor_z(i)`.
/// Appends the ribs' triangles to `out` and returns how many were built (0 when the wall is
/// stocky enough or nothing fits). JS default `clear = () => true`: callers pass it.
#[allow(clippy::too_many_arguments)]
pub fn brace_wall(
    top: &[[f64; 3]],
    floor_z: impl Fn(usize) -> f64,
    tris: &Tris,
    z_bed: f64,
    out: &mut Vec<[f64; 3]>,
    clear: impl Fn(&[[f64; 3]]) -> bool,
    kinds: BraceKinds,
    cfg: &Config,
) -> usize {
    let (prop, brace) = (&cfg.prop, &cfg.brace);
    let n = top.len();
    if n < 2 {
        return 0;
    }
    let a = top[0];
    let b = top[n - 1];
    let len = jsnum::hypot(&[b[0] - a[0], b[1] - a[1]]);
    if len < 1e-6 {
        return 0;
    }
    let mut height = 0.0;
    for (i, t) in top.iter().enumerate() {
        height = jsnum::max(height, t[2] - prop.gap - floor_z(i));
    }
    let slim = kinds.slim && height > brace.aspect * len;
    let tall = kinds.tall && height > brace.tall_h;
    if !slim && !tall {
        return 0;
    }
    let dx = (b[0] - a[0]) / len;
    let dy = (b[1] - a[1]) / len;
    let reach0 = if tall {
        jsnum::max(brace.min_reach, prop.foot_ratio * height)
    } else {
        prop.foot_ratio * height
    };
    // ends only for a slim wall; face pairs at the middle of each faceStep share of a
    // tall one (one pair, at the middle station, otherwise)
    let mut ribs: Vec<(usize, [f64; 2])> = if slim {
        vec![(0, [-dx, -dy]), (n - 1, [dx, dy])]
    } else {
        Vec::new()
    };
    let pairs = if tall {
        jsnum::max(1.0, jsnum::round(len / brace.face_step))
    } else {
        1.0
    };
    let mut j = 0.0;
    while j < pairs {
        let i = jsnum::round((n as f64 - 1.0) * (j + 0.5) / pairs) as usize;
        ribs.push((i, [dy, -dx]));
        ribs.push((i, [-dy, dx]));
        j += 1.0;
    }
    let mut built = 0;
    for (i, d) in ribs {
        if floor_z(i) > z_bed + brace.on_plate {
            continue;
        }
        // under the tip taper: the contact and its breakaway stay exactly as they were
        let stem = jsnum::min(
            brace.rise * (top[i][2] - prop.gap - z_bed),
            top[i][2] - prop.gap - prop.tip_h - 1.0 - z_bed,
        );
        if stem < prop.base_h + 1.0 {
            continue;
        }
        let mut reach = reach0;
        while reach >= brace.min_reach {
            let r = rib(
                tris,
                [top[i][0], top[i][1]],
                d,
                z_bed,
                stem,
                reach,
                prop.th,
                cfg,
            );
            if let Some(r) = r
                && clear(&r)
            {
                out.extend(r);
                built += 1;
                break;
            }
            reach /= 2.0;
        }
    }
    built
}
