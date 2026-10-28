//! Port of `web/prop/clearance.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Which stations of a line may carry a wall: `station_is_clear` (can it reach the plate through
//! open air), `station_certified` (does the settled top measure clean against the whole part),
//! and the run/mask helpers that turn those per-station verdicts into the span actually swept.
//!
//! A line is `&[[f64; 3]]` stations `[x, y, z]`; `k` indexes a station. None of these reads the
//! crest state or the tine capture, so they take `cfg: &Config`, not a `BuildCtx`.

use crate::config::Config;
use crate::inside::{Nearest, inside_part, nearest_part};
use crate::jsnum;
use crate::overhangs::Topology;
use crate::prop::sweep::profile_half;
use crate::types::{Rot3, Xyz};
use rayon::prelude::*;

/// A line with fewer stations than this has its stations probed on the calling thread: a probe is
/// ~10-15 us, and waking the pool costs about as much as a dozen of them.
const PAR_MIN_STATIONS: usize = 16;

/// `f(k)` for each station `k` of `0..n`, in order: the stations of a long line are probed in
/// parallel (each verdict is a pure function of its station) and collected in station order.
pub fn station_verdicts(n: usize, f: impl Fn(usize) -> bool + Sync + Send) -> Vec<bool> {
    if n < PAR_MIN_STATIONS {
        (0..n).map(f).collect()
    } else {
        (0..n).into_par_iter().map(f).collect()
    }
}

/// The unit across-the-wall direction at station `k` (`(ry, -rx) / rn`), or `None` when the
/// neighbours coincide (`rn < 1e-9`: JS returns `true` early).
fn across(line: &[[f64; 3]], k: usize) -> Option<(f64, f64)> {
    let a = line[jsnum::max(0.0, k as f64 - 1.0) as usize];
    let b = line[jsnum::min((line.len() - 1) as f64, (k + 1) as f64) as usize];
    let rx = b[0] - a[0];
    let ry = b[1] - a[1];
    let rn = jsnum::hypot(&[rx, ry]);
    if rn < 1e-9 {
        return None;
    }
    Some((ry / rn, -rx / rn))
}

/// Port of `stationIsClear` (web/prop/clearance.js:21-71): can a wall under `line` actually reach
/// the plate, or is the part in the way?
///
/// The prop attaches to the BED and nothing else -- that is what makes it one clean thing to
/// remove rather than two welds to cut. An overhang tucked above other geometry has no such
/// path, and a wall driven down through the part is worse than no wall at all. Sampled down the
/// centreline of each station.
///
/// Probes the FULL height, and OUTSIDE the wall's own faces: at the wall's half-width PLUS a
/// margin (a flank 0.0002mm off the part is outside it, passes containment, and fuses solid on
/// the first layer). `clearProbes` is a FLOOR, not the count: at least one probe every 1.5mm of
/// height. Offsets step through the clearance corridor rather than testing only its far edge,
/// because `inside_part` is a parity test: a rib THINNER than the corridor can sit wholly
/// between the wall face and a single far-edge probe. Stepping at ~0.12mm resolves anything a
/// nozzle can actually print.
pub fn station_is_clear(
    line: &[[f64; 3]],
    k: usize,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    cfg: &Config,
) -> bool {
    let p = line[k];
    let Some((sx, sy)) = across(line, k) else {
        return true;
    };

    let top = p[2] - cfg.prop.gap;

    let n_probes = jsnum::max(cfg.prop.clear_probes, jsnum::ceil(top / 1.5));
    let mut i = 1.0;
    while i <= n_probes {
        let z = (top * i) / n_probes - 0.05;
        i += 1.0;
        if z <= 0.0 {
            continue;
        }
        let half = profile_half(z, top, cfg); // flange, wall, or tip taper at this z
        for m in [0.12, 0.24, cfg.prop.side_clear] {
            let w = half + m;
            if inside_part(topo, rot, offset, p[0] + sx * w, p[1] + sy * w, z) {
                return false;
            }
            if inside_part(topo, rot, offset, p[0] - sx * w, p[1] - sy * w, z) {
                return false;
            }
        }
        if inside_part(topo, rot, offset, p[0], p[1], z) {
            return false;
        }
    }
    true
}

/// Port of `stationCertified` (web/prop/clearance.js:94-124): certify a SETTLED station by
/// measuring, the way the checker will.
///
/// `settleTop` puts the top exactly on spec against the region's own triangles -- but the
/// geometry nearest a wall can be a different region, a face too steep to be an overhang, or a
/// surface arriving tangent to the wall's flank. So sample the station's cross-section outline
/// (the top flat, then both flanks every 2mm down the full height, at the profile's own
/// half-width for that height) and measure the actual distance to the part, classifying each
/// approach as `welds` does. A station that fails is trimmed by the same `longest_run`
/// machinery as every other local problem. Runs AFTER settling.
pub fn station_certified(
    line: &[[f64; 3]],
    k: usize,
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    cfg: &Config,
) -> bool {
    let p = line[k];
    let Some((sx, sy)) = across(line, k) else {
        return true;
    };

    let top = p[2] - cfg.prop.gap;

    let mut probes: Vec<[f64; 2]> = vec![
        [0.0, top],
        [cfg.prop.tip / 2.0, top],
        [-cfg.prop.tip / 2.0, top],
    ];
    let mut z = 0.4;
    while z < top - 0.05 {
        let half = profile_half(z, top, cfg);
        probes.push([half, z]);
        probes.push([-half, z]);
        z += 2.0;
    }
    for [o, z] in probes {
        if z <= 0.05 {
            continue;
        }
        // `nearestPart`'s default `rMax = 0.45`
        let hit = nearest_part(topo, rot, offset, p[0] + sx * o, p[1] + sy * o, z, 0.45);
        let Some(hit) = hit else { continue };
        if welds(&hit, cfg) {
            return false;
        }
    }
    true
}

/// Port of `welds` (web/prop/clearance.js:137-139): would a wall this close to the part weld to
/// it? `hit` is a nearest-part measurement (`{ d, cosUp }`). From ABOVE it is the breakaway
/// interface: the top stands `gap` below the part VERTICALLY, so the nearest distance on a slope
/// is only ~gap x cos(slope). The floor is a fraction of the gap (0.675 x gap: the same 0.135 at
/// 0.2 and the same ~47.5 deg at any gap). SIDEWAYS it is a flank and must clear the checker's
/// FLANK_MIN. Every weld gate uses this one test.
pub fn welds(hit: &Nearest, cfg: &Config) -> bool {
    if hit.cos_up > 0.7 {
        hit.d < cfg.prop.gap * 0.675
    } else {
        hit.d < 0.205
    }
}

/// Port of `pathToPlateIsClear` (web/prop/clearance.js:142-147): true when EVERY station can
/// reach the plate.
pub fn path_to_plate_is_clear(
    line: &[[f64; 3]],
    topo: &Topology,
    rot: &Rot3,
    offset: &Xyz,
    cfg: &Config,
) -> bool {
    for k in 0..line.len() {
        if !station_is_clear(line, k, topo, rot, offset, cfg) {
            return false;
        }
    }
    true
}

/// Port of `longestRun` (web/prop/clearance.js:166-177): the longest contiguous run of stations
/// that can carry a wall, as `[a, b)`; `None` is JS `null`. A tie keeps the earlier run.
///
/// THIS IS THE `t0/t1` TRIM: `breakaway_wall(bm, contact, t0, t1, ...)` takes the span as an
/// argument, and a single bad station used to discard the whole region.
pub fn longest_run(usable: &[bool]) -> Option<[usize; 2]> {
    let mut best: Option<[usize; 2]> = None;
    let mut start: Option<usize> = None;
    for i in 0..=usable.len() {
        if i < usable.len() && usable[i] {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start {
            if best.is_none_or(|b| i - s > b[1] - b[0]) {
                best = Some([s, i]);
            }
            start = None;
        }
    }
    best
}

/// Port of `withLowTails` (web/prop/clearance.js:192-201): the stations a plate wall may occupy:
/// the longest run of TALL stations (`tall[k]`, >= minHeight) extended at each end through the
/// contiguous LOW ones (`low[k]`, >= minHeightSquat) -- the wall's tail running on down the
/// slope. A run with no tall station is never a wall (that band is the squat pass's).
pub fn with_low_tails(tall: &[bool], low: &[bool]) -> Vec<bool> {
    let mut mask = vec![false; tall.len()];
    let Some([mut a, mut b]) = longest_run(tall) else {
        return mask;
    };
    while a > 0 && low[a - 1] {
        a -= 1;
    }
    while b < low.len() && low[b] {
        b += 1;
    }
    for m in &mut mask[a..b] {
        *m = true;
    }
    mask
}

/// Port of `bodyMask` (web/prop/clearance.js:210-212): which stations of a run are its
/// full-height BODY (top >= minHeight), as a mask. Taken BEFORE settleTop: settling legitimately
/// lowers the station next to a new low tail by a hair, so body membership is decided once.
pub fn body_mask(pts: &[[f64; 3]], cfg: &Config) -> Vec<bool> {
    pts.iter()
        .map(|p| p[2] - cfg.prop.gap >= cfg.prop.min_height)
        .collect()
}

/// Port of `tallBody` (web/prop/clearance.js:215-218): `[first, last]` body station of `mask`,
/// or `None` when the body is < 2 stations.
pub fn tall_body(mask: &[bool]) -> Option<[usize; 2]> {
    let a = mask.iter().position(|&m| m)?;
    let b = mask.iter().rposition(|&m| m)?;
    if b <= a { None } else { Some([a, b]) }
}

/// Port of `tallSpan` (web/prop/clearance.js:225-228): XY span of a run's BODY, first to last
/// body station. The minSpan gate measures this, not the whole run: a low tail extends a wall
/// that already earned its place, it must not promote a stub into one.
pub fn tall_span(pts: &[[f64; 3]], mask: &[bool]) -> f64 {
    match tall_body(mask) {
        Some(r) => jsnum::hypot(&[pts[r[1]][0] - pts[r[0]][0], pts[r[1]][1] - pts[r[0]][1]]),
        None => 0.0,
    }
}

/// Port of `minSpanFor` (web/prop/clearance.js:237-244): the least body span (`tall_span`) a run
/// needs to be a wall: minSpan, or minSpanTube for a small tube (`small_tube`, JS default
/// `false`). With `short` (default `false`; the last-resort pass, see web/fins/shortwalls.js) it
/// relaxes down to minSpanShort for a LOW wall: height (tallest body top, less the gap, over a
/// plate at z 0) at most maxShortAspect x the span.
pub fn min_span_for(
    pts: &[[f64; 3]],
    mask: &[bool],
    small_tube: bool,
    short: bool,
    cfg: &Config,
) -> f64 {
    if small_tube {
        return cfg.prop.min_span_tube;
    }
    let r = if short { tall_body(mask) } else { None };
    let Some(r) = r else {
        return cfg.prop.min_span;
    };
    let mut h = 0.0f64;
    for p in &pts[r[0]..=r[1]] {
        h = jsnum::max(h, p[2] - cfg.prop.gap);
    }
    jsnum::min(
        cfg.prop.min_span,
        jsnum::max(cfg.prop.min_span_short, h / cfg.prop.max_short_aspect),
    )
}

/// Port of `insertFloorStations` (web/prop/clearance.js:254-272): where a line's END dips under
/// the squat floor, insert one station exactly AT the floor (top = minHeightSquat), linearly
/// between the first station above it and the last below (an edge row's track can run past the
/// knife edge, so several end stations may sit under the floor). Stations are 1mm apart, so
/// without this the wall's low end snaps to whichever station happens to clear the floor. Mutates
/// `line` (JS returns it).
pub fn insert_floor_stations(line: &mut Vec<[f64; 3]>, cfg: &Config) {
    if line.is_empty() {
        return;
    }
    // a hair over, so float noise can't drop it
    let floor_z = cfg.prop.min_height_squat + cfg.prop.gap + 0.02;
    // i below the floor, j above
    let cross = |line: &[[f64; 3]], i: usize, j: usize| -> Option<[f64; 3]> {
        let (zi, zj) = (line[i][2], line[j][2]);
        if !(zi < floor_z && zj > floor_z + 1e-3) {
            return None;
        }
        let t = (floor_z - zi) / (zj - zi);
        let mut out = line[i];
        for c in 0..3 {
            out[c] = line[i][c] + (line[j][c] - line[i][c]) * t;
        }
        Some(out)
    };
    // walk in from each end past every sub-floor station to the first crossing
    let mut j = line.len() - 1;
    while j > 0 && line[j][2] < floor_z {
        j -= 1;
    }
    let hi_end = if j < line.len() - 1 {
        cross(line, j + 1, j)
    } else {
        None
    };
    if let Some(p) = hi_end {
        line.insert(j + 1, p);
    }
    let mut i = 0;
    while i < line.len() - 1 && line[i][2] < floor_z {
        i += 1;
    }
    let lo_end = if i > 0 { cross(line, i - 1, i) } else { None };
    if let Some(p) = lo_end {
        line.insert(i, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(zs: &[f64]) -> Vec<[f64; 3]> {
        zs.iter()
            .enumerate()
            .map(|(i, &z)| [i as f64, 0.0, z])
            .collect()
    }

    #[test]
    fn welds_from_above_is_a_fraction_of_the_gap_and_sideways_a_flat_floor() {
        let mut cfg = Config::default(); // gap 0.2
        let h = |d, cos_up| Nearest { d, cos_up };
        assert!(welds(&h(0.134, 1.0), &cfg) && !welds(&h(0.136, 1.0), &cfg)); // 0.675 x 0.2 = 0.135
        assert!(welds(&h(0.2, 0.7), &cfg) && !welds(&h(0.206, 0.7), &cfg)); // 0.7 is not "above": FLANK_MIN
        assert!(!welds(&h(0.136, 0.7000001), &cfg));
        cfg.prop.gap = 0.3; // PETG: the same ~47.5 deg
        assert!(welds(&h(0.2, 1.0), &cfg) && !welds(&h(0.21, 1.0), &cfg));
        assert!(welds(&h(0.2, -1.0), &cfg)); // below / sideways: 0.205 whatever the gap
    }

    #[test]
    fn longest_run_picks_the_first_of_equal_runs_and_none_of_nothing() {
        assert_eq!(longest_run(&[]), None);
        assert_eq!(longest_run(&[false, false]), None);
        assert_eq!(longest_run(&[true]), Some([0, 1]));
        assert_eq!(longest_run(&[true, true, false, true, true]), Some([0, 2])); // a tie keeps the earlier
        assert_eq!(
            longest_run(&[true, false, true, true, true, false]),
            Some([2, 5])
        );
        assert_eq!(longest_run(&[false, true, true]), Some([1, 3])); // a run to the end
    }

    #[test]
    fn low_tails_extend_the_tall_run_through_contiguous_low_stations() {
        let t = |s: &str| s.chars().map(|c| c == 'T' || c == 't').collect::<Vec<_>>();
        // tall = T, low (also true for tall) = T or t
        let m = with_low_tails(&t("..TT.."), &t(".tTTt."));
        assert_eq!(m, t(".tTTt."));
        // a gap in the low stations stops the tail
        assert_eq!(with_low_tails(&t("...T..."), &t(".t.Ttt.")), t("...Ttt."));
        // no tall station: never a wall, however low the rest
        assert_eq!(with_low_tails(&t("....."), &t("ttttt")), t("....."));
        // only the LONGEST tall run counts
        assert_eq!(with_low_tails(&t("T.TT"), &t("TtTT")), t("TtTT"));
        assert!(with_low_tails(&[], &[]).is_empty());
    }

    #[test]
    fn body_membership_is_decided_once_by_height_over_the_gap() {
        let cfg = Config::default();
        let min = cfg.prop.min_height;
        let l = line(&[min + cfg.prop.gap - 1e-9, min + cfg.prop.gap, 0.0, 99.0]);
        assert_eq!(body_mask(&l, &cfg), vec![false, true, false, true]);
        assert_eq!(tall_body(&[false, true, true, false, true]), Some([1, 4]));
        assert_eq!(tall_body(&[true]), None); // a one-station body is no body
        assert_eq!(tall_body(&[false, false]), None);
        assert_eq!(tall_body(&[]), None);
    }

    #[test]
    fn tall_span_is_the_plan_distance_between_the_first_and_last_body_station() {
        let pts = [
            [0.0, 0.0, 9.0],
            [3.0, 0.0, 9.0],
            [3.0, 4.0, 9.0],
            [9.0, 9.0, 9.0],
        ];
        assert_eq!(tall_span(&pts, &[true, false, true, false]), 5.0);
        assert_eq!(tall_span(&pts, &[false, false, true, false]), 0.0);
    }

    #[test]
    fn min_span_for_relaxes_only_for_a_short_low_wall() {
        let cfg = Config::default();
        let p = &cfg.prop;
        let pts = line(&[p.gap + 2.0, p.gap + 2.0, p.gap + 2.0]);
        let mask = [true, true, true];
        assert_eq!(min_span_for(&pts, &mask, true, true, &cfg), p.min_span_tube); // a small tube wins
        assert_eq!(min_span_for(&pts, &mask, false, false, &cfg), p.min_span);
        let relaxed = min_span_for(&pts, &mask, false, true, &cfg);
        assert_eq!(
            relaxed,
            jsnum::min(
                p.min_span,
                jsnum::max(p.min_span_short, 2.0 / p.max_short_aspect)
            )
        );
        // no body: back to minSpan even with `short`
        assert_eq!(
            min_span_for(&pts, &[true, false, false], false, true, &cfg),
            p.min_span
        );
        // a tall wall never goes under minSpanShort, and never over minSpan
        let tall = line(&[p.gap + 500.0, p.gap + 500.0]);
        assert_eq!(
            min_span_for(&tall, &[true, true], false, true, &cfg),
            p.min_span
        );
    }

    #[test]
    fn floor_stations_are_inserted_at_each_dipping_end() {
        let cfg = Config::default();
        let floor_z = cfg.prop.min_height_squat + cfg.prop.gap + 0.02;
        let mut l = vec![
            [0.0, 0.0, 0.1],
            [1.0, 0.0, 0.3],
            [2.0, 0.0, 5.0],
            [3.0, 0.0, 5.0],
            [4.0, 0.0, 0.3],
            [5.0, 0.0, 0.1],
        ];
        insert_floor_stations(&mut l, &cfg);
        assert_eq!(l.len(), 8);
        assert!((l[2][2] - floor_z).abs() < 1e-12 && (l[5][2] - floor_z).abs() < 1e-12);
        assert!(l[2][0] > 1.0 && l[2][0] < 2.0); // on the segment from the last sub-floor station up
        assert!(l[5][0] > 3.0 && l[5][0] < 4.0);
        // nothing to insert: all high, all low, empty, one station
        for mut l in [line(&[5.0, 6.0]), line(&[0.0, 0.1]), vec![], line(&[5.0])] {
            let before = l.clone();
            insert_floor_stations(&mut l, &cfg);
            assert_eq!(l, before);
        }
        // a line whose ends are already above the floor gets nothing
        let mut flat = line(&[floor_z + 1.0, floor_z + 2.0, floor_z + 1.0]);
        let before = flat.clone();
        insert_floor_stations(&mut flat, &cfg);
        assert_eq!(flat, before);
    }
}
