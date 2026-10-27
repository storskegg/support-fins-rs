//! Port of `web/prop/arc.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M2)

/// `ARC` (web/prop/arc.js:26). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct ArcConfig {
    /// JS key `maxWidth`.
    // mm across the strip: one wall then has every edge within 3 mm (half
    // maxUnsupportedSpan's 6 mm held rule); wider bands get rows
    pub max_width: f64,
    /// JS key `minGap`.
    // deg: the strip must leave at least this much of the circle open --
    // circular.js takes complete loops at gaps up to 30
    pub min_gap: f64,
    /// JS key `minLen`.
    // mm of line: Draw's DRAW_MIN_LEN, a short slot is still a slot
    pub min_len: f64,
    // mm between probes across the strip for its edges (a quarter of `gap`)
    pub probe: f64,
}

impl Default for ArcConfig {
    fn default() -> Self {
        ArcConfig {
            max_width: 6.0,
            min_gap: 30.0,
            min_len: 2.0,
            probe: 0.05,
        }
    }
}

use crate::config::Config;
use crate::jsnum;
use crate::prop::curved::line_bend;
use crate::prop::surface::surface_z_at;
use crate::tris::Tris;

/// `TAU` (web/prop/arc.js:35).
const TAU: f64 = 2.0 * std::f64::consts::PI;

/// Port of `fitCircle` (web/prop/arc.js:38-51): least-squares circle through the points' plan
/// positions (Kasa), or `None` (JS `null`) when they are collinear.
fn fit_circle(pts: &[[f64; 3]]) -> Option<[f64; 2]> {
    let o = pts[0];
    let (mut mx, mut my) = (0.0f64, 0.0f64);
    for p in pts {
        mx += p[0] - o[0];
        my += p[1] - o[1];
    }
    mx /= pts.len() as f64;
    my /= pts.len() as f64;
    let (mut xx, mut xy, mut yy, mut xr, mut yr) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        let x = p[0] - o[0] - mx;
        let y = p[1] - o[1] - my;
        let r2 = x * x + y * y;
        xx += x * x;
        xy += x * y;
        yy += y * y;
        xr += x * r2;
        yr += y * r2;
    }
    let det = xx * yy - xy * xy;
    if det <= 1e-9 {
        return None; // collinear: a straight strip
    }
    Some([
        o[0] + mx + (yy * xr - xy * yr) / (2.0 * det),
        o[1] + my + (xx * yr - xy * xr) / (2.0 * det),
    ])
}

/// Port of `arcLine` (web/prop/arc.js:57-109): the centre line of a level strip that curves in
/// plan, as one track (`Some(vec![line])`, in tubeLine's shape -- `[x, y, z]` stations ~`step`
/// apart, z the region's surface), or `None` (JS `null`) when the region isn't one. `step` is
/// JS's default `PROP.stationStep`, passed explicitly.
///
/// Only a strip: narrow (ARC.maxWidth across everywhere); level (its whole rise within
/// PROP.contourSlopeMin of maxWidth: a slope is downhill.js's); open (a complete ring is
/// circular.js's); and bending more than CURVED.minBend (a straight strip is the patch path's).
/// A circle fitted to its points gives a centre, stations are stepped round it, and each sits
/// midway across the strip there (its edges probed along the radius).
pub fn arc_line(
    region_pts: &[[f64; 3]],
    region_tris: &Tris,
    step: f64,
    cfg: &Config,
) -> Option<Vec<Vec<[f64; 3]>>> {
    let arc = &cfg.arc;
    if region_pts.len() < 6 {
        return None;
    }
    // Level ACROSS the strip too, not only along the line: on a sloping chamfer the
    // fitted arc can run along a contour and read level (ssdMounts-Chamfer X60, a
    // 75 x 8 mm strip falling 4.8 mm across, took an arc and lost 96 -> 11%).
    let (mut z_lo, mut z_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in region_pts {
        z_lo = jsnum::min(z_lo, p[2]);
        z_hi = jsnum::max(z_hi, p[2]);
    }
    if z_hi - z_lo > cfg.prop.contour_slope_min * arc.max_width {
        return None;
    }
    let c = fit_circle(region_pts)?;
    // Polar coordinates round the centre, the angle measured from the strip's
    // middle so the strip never straddles the -pi/pi seam.
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    for p in region_pts {
        let a = jsnum::atan2(p[1] - c[1], p[0] - c[0]);
        sx += jsnum::cos(a);
        sy += jsnum::sin(a);
    }
    let mid = jsnum::atan2(sy, sx);
    let polar: Vec<[f64; 2]> = region_pts
        .iter()
        .map(|p| {
            let mut a = jsnum::atan2(p[1] - c[1], p[0] - c[0]) - mid;
            a -= TAU * jsnum::round(a / TAU);
            [a, jsnum::hypot(&[p[0] - c[0], p[1] - c[1]])]
        })
        .collect();
    let (mut a0, mut a1, mut r_sum) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
    for &[a, r] in &polar {
        a0 = jsnum::min(a0, a);
        a1 = jsnum::max(a1, a);
        r_sum += r;
    }
    if (TAU - (a1 - a0)) * 180.0 / std::f64::consts::PI < arc.min_gap {
        return None; // a ring
    }
    let r_mean = r_sum / polar.len() as f64;
    if r_mean < arc.max_width + arc.probe {
        return None; // the probes would cross the centre
    }
    // One station every `step` along the mean radius. At each, the strip's edges are
    // found by probing the region's surface out along the radius (ARC.probe apart):
    // the vertices alone are too sparse -- a long facet spans several stations, and a
    // station seeing only one edge's vertices sat 0.5 mm off the middle of a 1.6 mm strip.
    // The probes span rMean +- maxWidth, so a strip whose radius drifts more than ~3 mm
    // from its mean (a spiral) is clipped there -- a slot's radius doesn't drift.
    let n = jsnum::max(2.0, jsnum::ceil(((a1 - a0) * r_mean) / step)) as usize;
    let reach = arc.max_width + arc.probe;
    let mut line: Vec<[f64; 3]> = Vec::new();
    for i in 0..=n {
        let a = mid + a0 + (i as f64 / n as f64) * (a1 - a0);
        let (ca, sa) = (jsnum::cos(a), jsnum::sin(a));
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut r = r_mean - reach;
        while r <= r_mean + reach {
            if surface_z_at(region_tris, c[0] + r * ca, c[1] + r * sa).is_some() {
                lo = jsnum::min(lo, r);
                hi = jsnum::max(hi, r);
            }
            r += arc.probe;
        }
        if lo > hi {
            continue; // off the strip: an end's corner
        }
        if hi - lo > arc.max_width {
            return None; // a band, not a strip
        }
        let r = (lo + hi) / 2.0;
        let (x, y) = (c[0] + r * ca, c[1] + r * sa);
        if let Some(z) = surface_z_at(region_tris, x, y) {
            line.push([x, y, z]);
        }
    }
    if (line.len() as f64) < cfg.prop.min_stations {
        return None;
    }
    let mut run = 0.0f64;
    for i in 1..line.len() {
        run += jsnum::hypot(&[line[i][0] - line[i - 1][0], line[i][1] - line[i - 1][1]]);
    }
    if run < arc.min_len {
        return None;
    }
    if line_bend(&line, cfg) <= cfg.curved.min_bend {
        return None;
    }
    Some(vec![line])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An annular strip of `n` facets (r0..r1, angles 0..a1) at height z, with its points
    /// (vertices + centroids, as buildProps hands them).
    fn strip(r0: f64, r1: f64, a1: f64, z: f64, n: usize) -> (Vec<[f64; 3]>, Tris) {
        let p = |r: f64, a: f64| [r * a.cos(), r * a.sin(), z];
        let (mut tris, mut pts) = (Vec::new(), Vec::new());
        for i in 0..n {
            let (s, e) = (a1 * i as f64 / n as f64, a1 * (i + 1) as f64 / n as f64);
            let q = [p(r0, s), p(r1, s), p(r1, e), p(r0, e)];
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                tris.extend(t.iter().flatten());
                pts.extend(t);
                pts.push([
                    (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                    (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                    (t[0][2] + t[1][2] + t[2][2]) / 3.0,
                ]);
            }
        }
        (pts, Tris::new(tris))
    }

    #[test]
    fn fit_circle_finds_the_centre_and_gives_up_on_a_line() {
        let ring: Vec<[f64; 3]> = (0..12)
            .map(|i| {
                let a = i as f64 * 0.4;
                [3.0 + 5.0 * a.cos(), -2.0 + 5.0 * a.sin(), 0.0]
            })
            .collect();
        let c = fit_circle(&ring).unwrap();
        assert!((c[0] - 3.0).abs() < 1e-9 && (c[1] + 2.0).abs() < 1e-9);
        let line: Vec<[f64; 3]> = (0..8).map(|i| [i as f64, 2.0 * i as f64, 0.0]).collect();
        assert!(fit_circle(&line).is_none());
    }

    #[test]
    fn a_slot_in_a_tube_wall_gets_one_arc_track_along_its_middle() {
        let cfg = Config::default();
        let (pts, tris) = strip(14.2, 15.8, std::f64::consts::FRAC_PI_2, 5.0, 24);
        let tracks = arc_line(&pts, &tris, 1.0, &cfg).expect("an arc");
        assert_eq!(tracks.len(), 1);
        let line = &tracks[0];
        assert!(line.len() >= 15);
        for p in line {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - 15.0).abs() < 0.1, "midway across the strip: {r}");
            assert!((p[2] - 5.0).abs() < 1e-9);
        }
    }

    #[test]
    fn what_is_not_a_level_open_narrow_bent_strip_gets_none() {
        let cfg = Config::default();
        let q = std::f64::consts::FRAC_PI_2;
        let (pts, tris) = strip(14.2, 15.8, q, 5.0, 24);
        // too few points
        assert!(arc_line(&pts[..5], &tris, 1.0, &cfg).is_none());
        // a band wider than ARC.maxWidth (6 mm)
        let (p, t) = strip(14.0, 22.0, q, 5.0, 24);
        assert!(arc_line(&p, &t, 1.0, &cfg).is_none());
        // a full ring is circular.js's
        let (p, t) = strip(14.2, 15.8, std::f64::consts::TAU, 5.0, 72);
        assert!(arc_line(&p, &t, 1.0, &cfg).is_none());
        // too straight (a huge radius): bends less than CURVED.minBend
        let (p, t) = strip(400.0, 401.6, 0.0175, 5.0, 12);
        assert!(arc_line(&p, &t, 1.0, &cfg).is_none());
        // a tighter ARC.minLen than the line is long
        let mut long = Config::default();
        long.arc.min_len = 99.0;
        assert!(arc_line(&pts, &tris, 1.0, &long).is_none());
        // a slope across the strip steeper than contourSlopeMin x maxWidth
        let tilted: Vec<[f64; 3]> = pts
            .iter()
            .map(|p| [p[0], p[1], p[2] + (p[0] * p[0] + p[1] * p[1]).sqrt() - 14.2])
            .collect();
        let mut steep = tilted.clone();
        steep[0][2] += 5.0;
        assert!(arc_line(&steep, &tris, 1.0, &cfg).is_none());
        // a radius smaller than the probes' reach
        let (p, t) = strip(3.0, 4.6, q, 5.0, 24);
        assert!(arc_line(&p, &t, 1.0, &cfg).is_none());
    }
}
