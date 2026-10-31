//! The "Plate ties" calibration print (issue #50, slice 3): three pairs of upright coins, each pair
//! tied at another tie thickness, so a user can see which thickness snaps off clean on their printer.
//!
//! Only the geometry and the three settings are here; finning each coin and writing the plate is the
//! caller's (`sf_cli::calibration`, which the CLI and the app share). Nothing on a parity path calls it.
//!
//! - **Coins**: 38 mm wide, 40 mm tall, 4 mm thick (the user's coins), standing on their rim, thin
//!   along Y; the two of a pair 4 mm apart, the pairs side by side along X.
//! - **Variants** ([`variants`]): the contact thickness of the profile's settings bracketed by its
//!   thinnest tie (`minThickness`, from the printer's pixel size), so the print adapts to the printer:
//!   one step thinner, the setting itself, one step thicker, the step the distance from the setting
//!   down to the thinnest tie (half the thinnest tie when that is closer). The middle thickness keeps
//!   the settings' middle-to-contact ratio. Every other setting is the caller's.
//! - **Labels**: the variant's number (1, 2, 3, thinnest first) raised 0.5 mm on the outer face of both
//!   coins of its pair (the face away from the ties), reading the right way round from outside, as
//!   seven-segment strokes. Each stroke's lower side slopes at 55 degrees, so a label has no overhang
//!   for the fins to support.

use super::PlateTies;

/// mm: a coin's width (X), height (Z) and thickness (Y).
pub const COIN_WIDTH: f64 = 38.0;
pub const COIN_HEIGHT: f64 = 40.0;
pub const COIN_THICKNESS: f64 = 4.0;
/// mm: between the two coins of a pair (face to face).
pub const COIN_GAP: f64 = 4.0;
/// mm: between two pairs (rim to rim, along X): well past any `maxGap`, so pairs never tie across.
pub const PAIR_SPACING: f64 = 14.0;
/// mm: how far a label stands off its coin's face.
pub const LABEL_RAISE: f64 = 0.5;
/// mm: a label's height and width, and its strokes' width.
const DIGIT_HEIGHT: f64 = 10.0;
const DIGIT_WIDTH: f64 = 6.0;
const STROKE: f64 = 1.5;
/// degrees: a stroke's lower side slopes this steeply (from the horizontal).
const UNDERSIDE: f64 = 55.0;
/// Points around a coin's rim.
const RIM_POINTS: usize = 96;

/// One tie thickness of the print.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// 1, 2 or 3: thinnest first, as raised on its coins.
    pub label: u8,
    /// mm: where a tie meets a coin (where it snaps off).
    pub contact_thickness: f64,
    /// mm: in the middle of the gap.
    pub middle_thickness: f64,
    /// The settings' own contact thickness (the printer profile's default unless the caller changed it).
    pub is_default: bool,
}

impl Variant {
    /// `base` with this variant's thicknesses.
    pub fn settings(&self, base: &PlateTies) -> PlateTies {
        PlateTies {
            contact_thickness: self.contact_thickness,
            middle_thickness: self.middle_thickness,
            ..base.clone()
        }
    }
}

/// Rounded to a micrometre, so `0.6 - 0.296` is written `0.304`.
fn um(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// The print's three tie thicknesses around `base`'s contact thickness (see the module comment).
/// When `base`'s contact is the thinnest tie itself the three step up from it, so the default is
/// then variant 1.
pub fn variants(base: &PlateTies) -> [Variant; 3] {
    let min = base.tunables.min_thickness;
    let contact = base.contact_thickness;
    let step = (contact - min).max(min / 2.0);
    let lift = (min - (contact - step)).max(0.0);
    let ratio = if contact > 0.0 {
        (base.middle_thickness / contact).max(1.0)
    } else {
        1.0
    };
    let at = |k: u8| {
        let c = um(contact - step + lift + step * f64::from(k));
        Variant {
            label: k + 1,
            contact_thickness: c,
            middle_thickness: um(c * ratio),
            is_default: (c - contact).abs() < 5e-4,
        }
    };
    [at(0), at(1), at(2)]
}

/// One coin of the print, where it stands on the plate (its rim on the bed at z = 0).
#[derive(Clone, Debug)]
pub struct Coin {
    /// `1a`, `1b`, `2a`, ...: the variant's label and which of the pair.
    pub name: String,
    /// The variant's label (1, 2, 3).
    pub label: u8,
    /// Coin and label as one flat triangle soup (nine numbers per triangle), plate frame.
    pub positions: Vec<f64>,
    /// How many of those triangles are the label's.
    pub label_triangles: usize,
}

/// The six coins: pair `k` (variant `k + 1`) at x = (k - 1) * pitch, its `a` coin on the -Y side.
pub fn coins() -> Vec<Coin> {
    let pitch = COIN_WIDTH + PAIR_SPACING;
    let mut out = Vec::with_capacity(6);
    for k in 0..3u8 {
        let cx = (f64::from(k) - 1.0) * pitch;
        for (side, sign) in [("a", -1.0), ("b", 1.0)] {
            // the coin's inner face is COIN_GAP / 2 off the pair's middle, its outer face beyond it
            let inner = sign * COIN_GAP / 2.0;
            let outer = sign * (COIN_GAP / 2.0 + COIN_THICKNESS);
            let mut positions = coin_body(cx, inner.min(outer), inner.max(outer));
            let before = positions.len();
            digit(&mut positions, k + 1, cx, outer, sign);
            out.push(Coin {
                name: format!("{}{side}", k + 1),
                label: k + 1,
                label_triangles: (positions.len() - before) / 9,
                positions,
            });
        }
    }
    out
}

/// The coin's rim in the XZ plane, counter-clockwise, its lowest point on z = 0.
fn rim(cx: f64) -> Vec<[f64; 2]> {
    let (a, c) = (COIN_WIDTH / 2.0, COIN_HEIGHT / 2.0);
    (0..RIM_POINTS)
        .map(|i| {
            let t = i as f64 / RIM_POINTS as f64 * std::f64::consts::TAU;
            [cx + a * t.cos(), c + c * t.sin()]
        })
        .collect()
}

fn coin_body(cx: f64, y0: f64, y1: f64) -> Vec<f64> {
    let r = rim(cx);
    let back: Vec<[f64; 3]> = r.iter().map(|p| [p[0], y0, p[1]]).collect();
    let front: Vec<[f64; 3]> = r.iter().map(|p| [p[0], y1, p[1]]).collect();
    let mut out = Vec::new();
    convex_loft(&mut out, &back, &front);
    out
}

/// Seven-segment strokes: `[u0, u1, v0, v1]` in a digit's frame (u along the reading direction, v
/// up, the digit's middle at the origin).
fn strokes(d: u8) -> Vec<[f64; 4]> {
    let (w, h, s) = (DIGIT_WIDTH / 2.0, DIGIT_HEIGHT / 2.0, STROKE);
    let top = [-w, w, h - s, h];
    let mid = [-w, w, -s / 2.0, s / 2.0];
    let bottom = [-w, w, -h, -h + s];
    let upper_right = [w - s, w, 0.0, h];
    let lower_right = [w - s, w, -h, 0.0];
    let lower_left = [-w, -w + s, -h, 0.0];
    match d {
        1 => vec![upper_right, lower_right],
        2 => vec![top, upper_right, mid, lower_left, bottom],
        _ => vec![top, upper_right, mid, lower_right, bottom],
    }
}

/// The digit `d` raised on the face at y = `face` (outward `sign`), centred on x = `cx` a little above
/// the coin's middle. Seen from outside, +X is on the right of the -Y face and on the left of the +Y
/// face, so the +Y face's digit runs along -X.
fn digit(out: &mut Vec<f64>, d: u8, cx: f64, face: f64, sign: f64) {
    let zc = COIN_HEIGHT / 2.0 + 2.0;
    // the lower side climbs this much over the raise: 55 degrees from the horizontal
    let slope = LABEL_RAISE * UNDERSIDE.to_radians().tan();
    let u_dir = -sign; // -Y face: u = +X; +Y face: u = -X
    for [u0, u1, v0, v1] in strokes(d) {
        let quad = |y: f64, lo: f64| -> Vec<[f64; 3]> {
            [[u0, lo], [u1, lo], [u1, v1], [u0, v1]]
                .iter()
                .map(|[u, v]| [cx + u_dir * u, y, zc + v])
                .collect()
        };
        // sunk 0.05 mm into the coin so the label overlaps it (never flush)
        let base = quad(face - sign * 0.05, v0);
        let top = quad(face + sign * LABEL_RAISE, (v0 + slope).min(v1 - 0.2));
        convex_loft(out, &base, &top);
    }
}

/// A closed convex solid from two matching rings of points (`a[i]` joined to `b[i]`): both caps fanned
/// from their middles and the sides as quads, every triangle turned to face away from the solid's
/// middle.
fn convex_loft(out: &mut Vec<f64>, a: &[[f64; 3]], b: &[[f64; 3]]) {
    let mid = |r: &[[f64; 3]]| {
        let n = r.len() as f64;
        let s = r
            .iter()
            .fold([0.0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
        [s[0] / n, s[1] / n, s[2] / n]
    };
    let (ma, mb) = (mid(a), mid(b));
    let centre = [
        (ma[0] + mb[0]) / 2.0,
        (ma[1] + mb[1]) / 2.0,
        (ma[2] + mb[2]) / 2.0,
    ];
    let mut tri = |p: [f64; 3], q: [f64; 3], r: [f64; 3]| {
        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let c = [
            (p[0] + q[0] + r[0]) / 3.0 - centre[0],
            (p[1] + q[1] + r[1]) / 3.0 - centre[1],
            (p[2] + q[2] + r[2]) / 3.0 - centre[2],
        ];
        let (q, r) = if n[0] * c[0] + n[1] * c[1] + n[2] * c[2] < 0.0 {
            (r, q)
        } else {
            (q, r)
        };
        out.extend(p.into_iter().chain(q).chain(r));
    };
    let n = a.len();
    for i in 0..n {
        let j = (i + 1) % n;
        tri(ma, a[i], a[j]);
        tri(mb, b[j], b[i]);
        tri(a[i], a[j], b[j]);
        tri(a[i], b[j], b[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn the_variants_bracket_the_default_from_the_thinnest_tie() {
        // the Saturn 4 Ultra 16K: thinnest tie 0.304 mm, contact 0.6, middle 1.2
        let mut s = PlateTies::default();
        s.tunables.min_thickness = 0.304;
        let v = variants(&s);
        let c: Vec<f64> = v.iter().map(|v| v.contact_thickness).collect();
        let m: Vec<f64> = v.iter().map(|v| v.middle_thickness).collect();
        assert_eq!(c, [0.304, 0.6, 0.896]);
        assert_eq!(m, [0.608, 1.2, 1.792]);
        assert_eq!(
            v.iter().map(|v| v.is_default).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert_eq!(v.iter().map(|v| v.label).collect::<Vec<_>>(), [1, 2, 3]);
        for v in &v {
            v.settings(&s).validate().unwrap();
        }
        // a coarser printer: the bracket follows its thinnest tie
        s.tunables.min_thickness = 0.4;
        let c: Vec<f64> = variants(&s).iter().map(|v| v.contact_thickness).collect();
        assert_eq!(c, [0.4, 0.6, 0.8]);
        // a contact at the thinnest tie: the three step up from it, the default first
        s.contact_thickness = 0.4;
        let v = variants(&s);
        let c: Vec<f64> = v.iter().map(|v| v.contact_thickness).collect();
        assert_eq!(c, [0.4, 0.6, 0.8]);
        assert!(v[0].is_default && !v[1].is_default);
        for v in &v {
            v.settings(&s).validate().unwrap();
        }
    }

    /// Every edge of the soup is used exactly twice, once each way: closed, outward-consistent shells.
    fn closed(p: &[f64]) -> bool {
        let key = |i: usize| {
            let q = &p[i * 3..i * 3 + 3];
            [q[0].to_bits(), q[1].to_bits(), q[2].to_bits()]
        };
        let mut edges: HashMap<([u64; 3], [u64; 3]), i32> = HashMap::new();
        for t in 0..p.len() / 9 {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                *edges.entry((key(t * 3 + i), key(t * 3 + j))).or_default() += 1;
            }
        }
        edges
            .iter()
            .all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
    }

    #[test]
    fn six_closed_coins_in_three_labelled_pairs() {
        let c = coins();
        let names: Vec<&str> = c.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["1a", "1b", "2a", "2b", "3a", "3b"]);
        // 2 strokes for a 1, 5 for a 2 or a 3; 4 triangles a side and 2 cap fans of 4
        let strokes = |n: usize| n * 16;
        let want = [2, 2, 5, 5, 5, 5].map(strokes);
        assert_eq!(
            c.iter().map(|c| c.label_triangles).collect::<Vec<_>>(),
            want
        );
        for coin in &c {
            // the body, then one shell per stroke
            let body = RIM_POINTS * 4 * 9;
            assert!(
                closed(&coin.positions[..body]),
                "{} is not closed",
                coin.name
            );
            for (k, stroke) in coin.positions[body..].chunks(16 * 9).enumerate() {
                assert!(closed(stroke), "{} stroke {k} is not closed", coin.name);
            }
            let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for q in coin.positions.chunks(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            assert!((hi[0] - lo[0] - COIN_WIDTH).abs() < 1e-9);
            assert!(lo[2].abs() < 1e-9 && (hi[2] - COIN_HEIGHT).abs() < 1e-9);
            // the coin plus the raised label
            assert!((hi[1] - lo[1] - COIN_THICKNESS - LABEL_RAISE).abs() < 1e-9);
            // the label is on the outer face: away from the pair's middle (y = 0)
            let inner = if coin.name.ends_with('a') {
                hi[1]
            } else {
                -lo[1]
            };
            assert!(
                (inner + COIN_GAP / 2.0).abs() < 1e-9,
                "{}: {inner}",
                coin.name
            );
        }
    }

    #[test]
    fn a_label_has_no_face_pointing_down_steeper_than_its_underside() {
        // every downward face of a label is at least 50 degrees off the horizontal: no overhang to fin
        for coin in coins() {
            let label = &coin.positions[coin.positions.len() - coin.label_triangles * 9..];
            for t in label.chunks(9) {
                let u = [t[3] - t[0], t[4] - t[1], t[5] - t[2]];
                let v = [t[6] - t[0], t[7] - t[1], t[8] - t[2]];
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                let down = -n[2] / len; // sine of the face's tilt below vertical
                assert!(down < 50f64.to_radians().cos(), "{}: {down}", coin.name);
            }
        }
    }
}
