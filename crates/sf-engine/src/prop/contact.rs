//! Port of `web/prop/contact.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Contact lines: the polyline a wall's top follows under an overhang, and the passes that
//! bring it to exactly `gap` below the part -- `contact_line` (lowest underside per bucket),
//! `lower_sag` (spans that sag above the surface), `contour_top` (the tip's full width, not
//! just its centre), `settle_top` (the true closest approach) -- plus the `straightness` gate
//! a line must pass.
//!
//! JS mutates the `line` it is given (and returns it). Rust takes `&mut [[f64; 3]]` and returns
//! nothing; the stations are `[x, y, z]`.

use crate::config::Config;
use crate::jsnum;
use crate::prop::surface::surface_z_at;
use crate::tris::Tris;

/// The principal horizontal direction of an XY spread, via the 2x2 covariance
/// (shared by `straightness` and `contactLine`, which spell it out twice).
fn principal_axis(pts: &[[f64; 3]], cx: f64, cy: f64) -> (f64, f64) {
    let (mut sxx, mut sxy, mut syy) = (0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        let (dx, dy) = (p[0] - cx, p[1] - cy);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let tr = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let lam = tr / 2.0 + jsnum::sqrt(jsnum::max(0.0, (tr * tr) / 4.0 - det));
    let (mut ax, mut ay) = (sxy, lam - sxx);
    if jsnum::hypot(&[ax, ay]) < 1e-9 {
        ax = 1.0;
        ay = 0.0;
    }
    let an = jsnum::hypot(&[ax, ay]);
    ax /= an;
    ay /= an;
    (ax, ay)
}

/// Port of `straightness` (web/prop/contact.js:46-76): how far the contact polyline strays from
/// the straight line through it: RMS perpendicular deviation from the best-fit axis, over the
/// chord it spans. 0 is a perfectly straight run; a ring reads ~0.2.
///
/// THIS IS A PRECONDITION, NOT A QUALITY SCORE (see the JS comment: `breakaway.py` assumes
/// the contact line is ~straight; a bowl's lowest points form a ring, and bucketing a ring
/// along its "principal axis" is noise). The 0.10 threshold sits in the measured gap between
/// servable ledges (0.06-0.08) and hub_post_foot's ball-hub bowl (0.15-0.21). Arc length over
/// chord was the old measure and it grew with the sampling; this does not.
pub fn straightness(line: &[[f64; 3]]) -> f64 {
    let n = line.len();
    if n < 3 {
        return f64::INFINITY;
    }

    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    for p in line {
        cx += p[0];
        cy += p[1];
    }
    cx /= n as f64;
    cy /= n as f64;

    let (ax, ay) = principal_axis(line, cx, cy);

    let (mut ss, mut lo, mut hi) = (0.0f64, f64::INFINITY, f64::NEG_INFINITY);
    for p in line {
        let (dx, dy) = (p[0] - cx, p[1] - cy);
        let t = dx * ax + dy * ay;
        let perp = -dx * ay + dy * ax;
        ss += perp * perp;
        if t < lo {
            lo = t;
        }
        if t > hi {
            hi = t;
        }
    }
    let chord = hi - lo;
    if chord < 1e-9 {
        f64::INFINITY
    } else {
        jsnum::sqrt(ss / n as f64) / chord
    }
}

/// Port of `contactLine` (web/prop/contact.js:88-137): the lowest-surface polyline under a
/// region: principal horizontal axis, then the minimum-z sample in each bucket along it.
/// `None` is JS `null`.
///
/// It must follow the part's true UNDERSIDE, not its centreline. For a tilted round tube the
/// lowest point is offset sideways from, and higher than, `centreline - R`; propping the
/// centreline mis-places the wall and can drop it straight into whatever the tube emerges from.
pub fn contact_line(
    pts: &[[f64; 3]],
    tris: &Tris,
    n_samples: usize,
    cfg: &Config,
) -> Option<Vec<[f64; 3]>> {
    if pts.len() < 3 {
        return None;
    }

    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    for p in pts {
        cx += p[0];
        cy += p[1];
    }
    cx /= pts.len() as f64;
    cy /= pts.len() as f64;

    // principal horizontal direction, via the 2x2 covariance of the XY spread
    let (ax, ay) = principal_axis(pts, cx, cy);

    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in pts {
        let t = (p[0] - cx) * ax + (p[1] - cy) * ay;
        if t < lo {
            lo = t;
        }
        if t > hi {
            hi = t;
        }
    }
    if hi - lo < 1e-6 {
        return None;
    }

    let mut buckets: Vec<Option<[f64; 3]>> = vec![None; n_samples];
    for p in pts {
        let t = (p[0] - cx) * ax + (p[1] - cy) * ay;
        let k = jsnum::floor(((t - lo) / (hi - lo)) * n_samples as f64);
        // a NaN index is a property of the JS array, not an element: nothing is bucketed
        if k.is_nan() {
            continue;
        }
        let k = if k >= n_samples as f64 {
            n_samples - 1
        } else {
            k as usize
        };
        if buckets[k].is_none() || p[2] < buckets[k].unwrap()[2] {
            buckets[k] = Some(*p);
        }
    }
    let mut line: Vec<[f64; 3]> = buckets.into_iter().flatten().collect();
    if line.len() < 3 {
        return None;
    }

    // Re-fit each station's height to the surface directly above its OWN xy. The
    // bucket only chose where the wall should stand; the z it happened to carry
    // belongs to whichever point in that cross-slice was lowest, which is usually
    // somewhere else entirely.
    for p in &mut line {
        if let Some(zz) = surface_z_at(tris, p[0], p[1]) {
            *p = [p[0], p[1], zz];
        }
    }

    contour_top(&mut line, tris, f64::INFINITY, cfg);
    lower_sag(&mut line, tris, f64::INFINITY);
    Some(line)
}

/// Port of `lowerSag` (web/prop/contact.js:146-168): lower the ends of any segment that would
/// pass above the surface between them. The wall's top runs STRAIGHT between stations while
/// the underside curves, so a midpoint can end up proud of the part -- measured at 0.005mm of
/// clearance where 0.2mm was intended, which welds. `band` defaults to `Infinity`.
pub fn lower_sag(line: &mut [[f64; 3]], tris: &Tris, band: f64) {
    for _pass in 0..3 {
        let mut moved = false;
        for i in 0..line.len().saturating_sub(1) {
            let mx = (line[i][0] + line[i + 1][0]) / 2.0;
            let my = (line[i][1] + line[i + 1][1]) / 2.0;
            let Some(zz) = surface_z_at(tris, mx, my) else {
                continue;
            };
            let mid_wall = (line[i][2] + line[i + 1][2]) / 2.0;
            let over = mid_wall - zz;
            // `band` caps how far below the wall a surface can be and still pull it
            // down: a PART-ATTACHED top must settle against the OVERHANG, not the floor
            // metres beneath it that surfaceZAt (lowest hit) would otherwise return.
            if over > band {
                continue;
            }
            if over > 1e-4 {
                line[i] = [line[i][0], line[i][1], line[i][2] - over];
                line[i + 1] = [line[i + 1][0], line[i + 1][1], line[i + 1][2] - over];
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

/// Port of `contourTop` (web/prop/contact.js:195-212): lower each station until the wall's whole
/// TOP FACE clears the part, not just its centreline. The top is a `tip`-wide flat, and the
/// gap used to be set at the centre of it; on a sloped underside the up-slope corner rises by
/// `half_tip x slope` INTO the gap. So evaluate the surface at BOTH TIP CORNERS as well as the
/// centre, and take the lowest: that makes `gap` a floor instead of an average. ACROSS the tip
/// only, never ALONG the run (sampling along it double-counts the slope). `band` defaults to
/// `Infinity`.
pub fn contour_top(line: &mut [[f64; 3]], tris: &Tris, band: f64, cfg: &Config) {
    let half = cfg.prop.tip / 2.0;
    let len = line.len();
    for i in 0..len {
        let a = line[jsnum::max(0.0, i as f64 - 1.0) as usize];
        let b = line[jsnum::min((len - 1) as f64, (i + 1) as f64) as usize];
        let rx = b[0] - a[0];
        let ry = b[1] - a[1];
        let rn = jsnum::hypot(&[rx, ry]);
        if rn < 1e-9 {
            continue;
        }
        let (sx, sy) = (ry / rn, -rx / rn); // across the wall

        let z0 = line[i][2];
        let mut z = z0;
        for o in [-half, half] {
            let zz = surface_z_at(tris, line[i][0] + sx * o, line[i][1] + sy * o);
            // Within `band` of the overhang only: past that, the lowest hit is the
            // floor a part-attached wall means to land on, not the overhang it clears.
            if let Some(zz) = zz
                && zz < z
                && z0 - zz <= band
            {
                z = zz;
            }
        }
        line[i] = [line[i][0], line[i][1], z];
    }
}

/// Port of `settleTop` (web/prop/contact.js:239-305): shift a finished contact line so its
/// CLOSEST approach to the part is exactly `gap` -- no more, no less. The top edge is the
/// straight line BETWEEN stations, so the real minimum clearance lives somewhere the station
/// samples never looked; sampling every `step` mm along the top at both tip corners, each
/// station is lowered against the span it owns (PER STATION, not one global shift), iterated
/// because lowering a station changes what its neighbours measure. LOWER ONLY, never raise.
/// `step` defaults to 0.25 and `band` to `Infinity`.
pub fn settle_top(line: &mut [[f64; 3]], tris: &Tris, step: f64, band: f64, cfg: &Config) {
    let half = cfg.prop.tip / 2.0;
    let gap = cfg.prop.gap;

    for _pass in 0..3 {
        let mut drop = vec![0.0f64; line.len()];
        let mut moved = false;

        for i in 0..line.len().saturating_sub(1) {
            let (p, q) = (line[i], line[i + 1]);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let seg = jsnum::hypot(&[dx, dy]);
            if seg < 1e-9 {
                continue;
            }
            let (sx, sy) = (dy / seg, -dx / seg);
            let n = jsnum::max(1.0, jsnum::ceil(seg / step)) as usize;

            for k in 0..=n {
                let t = k as f64 / n as f64;
                let (x, y) = (p[0] + dx * t, p[1] + dy * t);
                let top_z = (p[2] + (q[2] - p[2]) * t) - gap;
                for o in [-half, 0.0, half] {
                    let Some(zz) = surface_z_at(tris, x + sx * o, y + sy * o) else {
                        continue;
                    };
                    // A surface more than `band` below the wall top is the floor, not the
                    // overhang -- leave it to floorLine/sweepBetween.
                    if top_z - zz > band {
                        continue;
                    }
                    let need = gap - (zz - top_z);
                    if need <= 1e-4 {
                        continue;
                    }
                    // charge the deficit to whichever end of the span owns this sample
                    let j = if t < 0.5 { i } else { i + 1 };
                    if need > drop[j] {
                        drop[j] = need;
                        moved = true;
                    }
                }
            }
        }

        if !moved {
            break;
        }
        for i in 0..line.len() {
            if drop[i] > 0.0 {
                line[i] = [line[i][0], line[i][1], line[i][2] - drop[i]];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(z: f64) -> Tris {
        // a 40 x 40 plate at height z (two triangles, facing up)
        Tris::new(vec![
            -20.0, -20.0, z, 20.0, -20.0, z, 20.0, 20.0, z, //
            -20.0, -20.0, z, 20.0, 20.0, z, -20.0, 20.0, z,
        ])
    }

    #[test]
    fn straightness_reads_a_gate_not_a_score() {
        assert_eq!(straightness(&[]), f64::INFINITY);
        assert_eq!(
            straightness(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            f64::INFINITY
        );
        let line: Vec<[f64; 3]> = (0..10).map(|i| [i as f64, 2.0 * i as f64, 5.0]).collect();
        assert!(straightness(&line) < 1e-9); // dead straight, whatever its heading
        // every station at one XY: no chord at all
        assert_eq!(
            straightness(&[[1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 1.0, 2.0]]),
            f64::INFINITY
        );
        let ring: Vec<[f64; 3]> = (0..24)
            .map(|i| {
                let a = i as f64 / 24.0 * std::f64::consts::TAU;
                [10.0 * a.cos(), 10.0 * a.sin(), 0.0]
            })
            .collect();
        assert!(straightness(&ring) > 0.10, "a ring fails the 0.10 gate");
    }

    #[test]
    fn contact_line_needs_three_points_and_a_spread() {
        let cfg = Config::default();
        let t = plate(5.0);
        assert!(contact_line(&[[0.0, 0.0, 5.0], [1.0, 0.0, 5.0]], &t, 8, &cfg).is_none());
        let dot = [[1.0, 1.0, 5.0]; 4];
        assert!(contact_line(&dot, &t, 8, &cfg).is_none()); // hi - lo < 1e-6
        // 2 buckets only -> fewer than 3 stations
        let few = [[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [10.1, 0.0, 5.0]];
        assert!(contact_line(&few, &t, 2, &cfg).is_none());
    }

    #[test]
    fn contact_line_follows_the_surface_height_under_each_station() {
        let cfg = Config::default();
        let t = plate(5.0);
        // points at the wrong height: every station is re-fitted to the surface above it,
        // then lowered to leave `gap` (0.2) under it
        let pts: Vec<[f64; 3]> = (0..30)
            .map(|i| [i as f64 - 15.0, 0.0, 9.0 + (i % 3) as f64])
            .collect();
        let line = contact_line(&pts, &t, 10, &cfg).unwrap();
        assert_eq!(line.len(), 10);
        for p in &line {
            assert_eq!(p[2], 5.0);
        }
    }

    #[test]
    fn lower_sag_pulls_a_proud_span_down_to_the_surface() {
        let t = plate(0.0);
        let mut line = vec![[-5.0, 0.0, 1.0], [5.0, 0.0, 1.0]];
        lower_sag(&mut line, &t, f64::INFINITY);
        assert_eq!(line, vec![[-5.0, 0.0, 0.0], [5.0, 0.0, 0.0]]);
        // a surface further below than `band` is the floor, not the overhang: left alone
        let mut far = vec![[-5.0, 0.0, 9.0], [5.0, 0.0, 9.0]];
        lower_sag(&mut far, &t, 2.0);
        assert_eq!(far[0][2], 9.0);
        // off the surface entirely: untouched; and an empty / one-station line is fine
        let mut off = vec![[100.0, 0.0, 9.0], [110.0, 0.0, 9.0]];
        lower_sag(&mut off, &t, f64::INFINITY);
        assert_eq!(off[0][2], 9.0);
        lower_sag(&mut [], &t, f64::INFINITY);
        lower_sag(&mut [[0.0, 0.0, 1.0]], &t, f64::INFINITY);
    }

    #[test]
    fn contour_top_takes_the_lowest_of_the_tip_corners() {
        let cfg = Config::default();
        // a step: z = 0 for y < 0, z = 3 for y > 0, so a wall along x at y = 0.1 has its -y corner over the low side
        let t = Tris::new(vec![
            -20.0, -20.0, 0.0, 20.0, -20.0, 0.0, 20.0, 0.0, 0.0, //
            -20.0, -20.0, 0.0, 20.0, 0.0, 0.0, -20.0, 0.0, 0.0, //
            -20.0, 0.0, 3.0, 20.0, 0.0, 3.0, 20.0, 20.0, 3.0, //
            -20.0, 0.0, 3.0, 20.0, 20.0, 3.0, -20.0, 20.0, 3.0,
        ]);
        let mut line = vec![[-5.0, 0.1, 9.0], [0.0, 0.1, 9.0], [5.0, 0.1, 9.0]];
        contour_top(&mut line, &t, f64::INFINITY, &cfg);
        // across = (ry, -rx)/rn = (0, -1): o = -half is at y = 0.1 + 0.3 (over z=3), o = +half at y = -0.2 (over z=0)
        assert!(line.iter().all(|p| p[2] == 0.0));
        // `band` keeps a surface far below from settling the top
        let mut banded = vec![[-5.0, 0.1, 9.0], [0.0, 0.1, 9.0], [5.0, 0.1, 9.0]];
        contour_top(&mut banded, &t, 1.0, &cfg);
        assert!(banded.iter().all(|p| p[2] == 9.0));
        // coincident neighbours (rn < 1e-9) are skipped
        let mut same = vec![[1.0, 1.0, 9.0], [1.0, 1.0, 9.0]];
        contour_top(&mut same, &t, f64::INFINITY, &cfg);
        assert_eq!(same[0][2], 9.0);
    }

    #[test]
    fn settle_top_lowers_only_and_leaves_gap_under_the_overhang() {
        let cfg = Config::default();
        // the part's underside at z = 10; a wall's top edge sits `gap` (0.2) under its line, and must
        // clear the surface by at least `gap`
        let t = plate(10.0);
        let mut line = vec![[-5.0, 0.0, 10.1], [5.0, 0.0, 10.1]];
        settle_top(&mut line, &t, 0.25, f64::INFINITY, &cfg);
        assert!((line[0][2] - 10.0).abs() < 1e-12 && (line[1][2] - 10.0).abs() < 1e-12);
        let mut clear = vec![[-5.0, 0.0, 9.9], [5.0, 0.0, 9.9]];
        settle_top(&mut clear, &t, 0.25, f64::INFINITY, &cfg);
        assert_eq!(clear[0][2], 9.9); // lower only, never raised
        // a surface further above than `band` is not the overhang
        let mut floor = vec![[-5.0, 0.0, 10.1], [5.0, 0.0, 10.1]];
        settle_top(&mut floor, &t, 0.25, -1.0, &cfg);
        assert_eq!(floor[0][2], 10.1);
    }
}
