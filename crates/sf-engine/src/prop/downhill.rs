//! Port of `web/prop/downhill.js`. Read `docs/AGENTS.md` before changing anything here.

/// `DOWNHILL` (web/prop/downhill.js:21). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct DownhillConfig {
    // mm: the region footprint sampled for its top edge
    pub grid: f64,
    // mm: central-difference step for the slope
    pub h: f64,
    /// JS key `minSlope`.
    // |grad z| under this is level: no downhill to follow
    pub min_slope: f64,
    // mm: the end seeds sit this far in from the top edge's ends
    pub inset: f64,
    // mm: a line that comes this close to an earlier one stops (they merge)
    pub join: f64,
    /// JS key `maxSteps`.
    pub max_steps: f64,
    /// JS key `maxLines`.
    // Curved walls' coverage seeds stop at this many lines under one surface
    pub max_lines: f64,
    /// JS key `maxGrow`.
    // a region swaps to downhill walls only if they weigh at most this x its walls...
    pub max_grow: f64,
    // ...and hold this share more than they do (raster needs RACE_REL, 10%: the
    // #25 fixture's curved end is +9.8%, and the weight cap guards the rest)
    pub race: f64,
}

impl Default for DownhillConfig {
    fn default() -> Self {
        DownhillConfig {
            grid: 1.0,
            h: 0.5,
            min_slope: 0.05,
            inset: 1.0,
            join: 3.0,
            max_steps: 600.0,
            max_lines: 60.0,
            max_grow: 1.5,
            race: 0.03,
        }
    }
}

use crate::guards::{JsSort, cmp_diff};
use std::collections::HashMap;

use crate::config::Config;
use crate::jsnum;
use crate::prop::judge::grid_key;
use crate::prop::surface::surface_z_at;
use crate::tris::Tris;

/// Port of `grad` (web/prop/downhill.js:34-44): plan gradient of the surface z at (x, y), as
/// `[dz/dx, dz/dy, z]`, or `None` (JS `null`) off the region. `h` defaults to `DOWNHILL.h`.
fn grad(tris: &Tris, x: f64, y: f64, h: f64) -> Option<[f64; 3]> {
    let z = surface_z_at(tris, x, y)?;
    let d = |dx: f64, dy: f64| {
        let a = surface_z_at(tris, x - dx, y - dy);
        let b = surface_z_at(tris, x + dx, y + dy);
        if let (Some(a), Some(b)) = (a, b) {
            (b - a) / (2.0 * h)
        } else if let Some(b) = b {
            (b - z) / h
        } else if let Some(a) = a {
            (z - a) / h
        } else {
            0.0
        }
    };
    Some([d(h, 0.0), d(0.0, h), z])
}

/// `o` of `downhillLines` (Curved walls on a grown surface, prop/trace.js): `h` the slope's
/// step (default `DOWNHILL.h`), `cover` the `[x, y, z]` red the lines must cover, `reach`
/// (default `PROP.maxUnsupportedSpan / 2`). Each `None` is the field being absent.
#[derive(Clone, Copy, Debug, Default)]
pub struct DownhillOpts<'a> {
    pub h: Option<f64>,
    pub cover: Option<&'a [[f64; 3]]>,
    pub reach: Option<f64>,
}

/// Rounded to 1e-6 mm (`q6`, web/prop/downhill.js:92).
fn q6(v: f64) -> f64 {
    jsnum::round(v * 1e6) / 1e6
}

/// `a || b` over comparator numbers: the first that is neither 0 nor NaN, else the last.
fn or(a: f64, b: impl FnOnce() -> f64) -> f64 {
    if a != 0.0 && !a.is_nan() { a } else { b() }
}

/// Port of `downhillLines` (web/prop/downhill.js:62-198): downhill tracks under one region.
/// `region_pts` are its seated vertices and face centroids (buildProps's), `region_tris` its
/// seated triangles. Returns tracks in `tubeLine`'s shape -- `[x, y, z]` stations `step` apart,
/// z the region's surface -- or `None` (JS `null`) when the region has no slope to run down (a
/// level ceiling: rows handle it).
///
/// `step` defaults to `PROP.stationStep` and `pitch` to `PROP.maxUnsupportedSpan`. With
/// `o.cover`, each line is trimmed to the stations within `reach` of red (the grown surface
/// runs past it), and red still farther than `reach` from every line seeds a line of its own,
/// highest first, so the lines cover the overhang and the fill pass takes scraps.
pub fn downhill_lines(
    region_pts: &[[f64; 3]],
    region_tris: &Tris,
    step: f64,
    pitch: f64,
    o: &DownhillOpts,
    cfg: &Config,
) -> Option<Vec<Vec<[f64; 3]>>> {
    let d = &cfg.downhill;
    let (grid, min_slope, inset, join, max_steps) =
        (d.grid, d.min_slope, d.inset, d.join, d.max_steps);
    let h = o.h.unwrap_or(d.h);
    let cover = o.cover;
    let reach = o.reach.unwrap_or(cfg.prop.max_unsupported_span / 2.0);
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for p in region_pts {
        if p[0] < x0 {
            x0 = p[0];
        }
        if p[0] > x1 {
            x1 = p[0];
        }
        if p[1] < y0 {
            y0 = p[1];
        }
        if p[1] > y1 {
            y1 = p[1];
        }
    }

    // The top edge: footprint samples from which the region ends within `grid` of
    // climbing. The climb is followed in quarter steps, re-aimed each time: one
    // straight step uphill on a CURVED slope (a spiral ramp's underside) cuts across
    // the curve and leaves through the side edges, which then read as the top.
    let leaves_uphill = |mut x: f64, mut y: f64| -> bool {
        for _s in 0..4 {
            let Some(g) = grad(region_tris, x, y, h) else {
                return true;
            };
            let m = jsnum::hypot(&[g[0], g[1]]);
            if m < min_slope {
                return false;
            }
            x += (g[0] / m) * (grid / 4.0);
            y += (g[1] / m) * (grid / 4.0);
        }
        surface_z_at(region_tris, x, y).is_none()
    };
    // The top samples' own mean downhill orders them across the slope: on a curved
    // region the whole region's mean is a chord that foreshortens the top edge.
    let mut top: Vec<[f64; 2]> = Vec::new();
    let (mut mx, mut my, mut sloped, mut n) = (0.0f64, 0.0f64, 0usize, 0usize);
    let mut y = y0 + grid / 2.0;
    while y < y1 {
        let mut x = x0 + grid / 2.0;
        while x < x1 {
            if let Some(g) = grad(region_tris, x, y, h) {
                n += 1;
                let m = jsnum::hypot(&[g[0], g[1]]);
                if m >= min_slope {
                    sloped += 1;
                    if leaves_uphill(x, y) {
                        top.push([x, y]);
                        mx -= g[0] / m;
                        my -= g[1] / m;
                    }
                }
            }
            x += grid;
        }
        y += grid;
    }
    if n == 0 || (sloped as f64) < n as f64 / 2.0 || top.is_empty() {
        return None; // mostly level: rows' job
    }
    let mm = jsnum::hypot(&[mx, my]);
    if mm < 1e-9 {
        return None;
    }
    let (ax, ay) = (-my / mm, mx / mm); // across the slope

    // Seeds: the top edge's two ends (inset) and every `pitch` between, evenly.
    // Rounded to 1e-6 mm, ties broken on rounded x then y: a symmetric part (a tube
    // at X40) puts two top samples exactly as far from a seed target, and float noise
    // must not pick between them (tests/stability.test.js).
    let mut across: Vec<(f64, usize)> = top
        .iter()
        .enumerate()
        .map(|(i, p)| (q6(p[0] * ax + p[1] * ay), i))
        .collect();
    across.js_sort_by(|a, b| {
        cmp_diff(or(a.0 - b.0, || {
            or(q6(top[a.1][0]) - q6(top[b.1][0]), || {
                q6(top[a.1][1]) - q6(top[b.1][1])
            })
        }))
    });
    let u_lo = across[0].0 + inset;
    let u_hi = across[across.len() - 1].0 - inset;
    let n_t = if u_hi <= u_lo {
        1
    } else {
        jsnum::ceil((u_hi - u_lo) / pitch) as usize + 1
    };
    let targets: Vec<f64> = if n_t == 1 {
        vec![(u_lo + u_hi) / 2.0]
    } else {
        (0..n_t)
            .map(|i| u_lo + ((u_hi - u_lo) * i as f64) / (n_t - 1) as f64)
            .collect()
    };
    let mut seeds: Vec<usize> = Vec::new();
    for u in targets {
        let mut best: Option<usize> = None;
        let mut bd = f64::INFINITY;
        for &(v, p) in &across {
            if q6(jsnum::abs(v - u)) < bd {
                bd = q6(jsnum::abs(v - u));
                best = Some(p);
            }
        }
        if let Some(b) = best
            && !seeds.contains(&b)
        {
            seeds.push(b);
        }
    }

    // Trace each seed down the steepest descent, `step` at a time, until it leaves
    // the region, levels out, turns back, or meets a line already traced.
    // `near`: within `join` of an earlier line, through a plan hash of their stations
    // (a linear scan of every station was O(lines^2 x steps^2) on a wide region)
    let mut lines: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut cells: HashMap<u64, Vec<[f64; 3]>> = HashMap::new();
    // plan hash of the red, for the trim and the coverage seeds
    let mut red_cells: HashMap<u64, Vec<[f64; 3]>> = HashMap::new();
    let rkey = |x: f64, y: f64| grid_key(jsnum::floor(x / reach), jsnum::floor(y / reach));
    for p in cover.unwrap_or(&[]) {
        red_cells.entry(rkey(p[0], p[1])).or_default().push(*p);
    }
    let near_red = |red_cells: &HashMap<u64, Vec<[f64; 3]>>, x: f64, y: f64| -> bool {
        let i0 = jsnum::floor(x / reach);
        let j0 = jsnum::floor(y / reach);
        for di in [-1.0, 0.0, 1.0] {
            for dj in [-1.0, 0.0, 1.0] {
                for q in red_cells
                    .get(&grid_key(i0 + di, j0 + dj))
                    .into_iter()
                    .flatten()
                {
                    if jsnum::hypot(&[q[0] - x, q[1] - y]) <= reach {
                        return true;
                    }
                }
            }
        }
        false
    };
    let near = |cells: &HashMap<u64, Vec<[f64; 3]>>, x: f64, y: f64| -> bool {
        let i0 = jsnum::floor(x / join);
        let j0 = jsnum::floor(y / join);
        for di in [-1.0, 0.0, 1.0] {
            for dj in [-1.0, 0.0, 1.0] {
                for q in cells.get(&grid_key(i0 + di, j0 + dj)).into_iter().flatten() {
                    if jsnum::hypot(&[q[0] - x, q[1] - y]) < join {
                        return true;
                    }
                }
            }
        }
        false
    };
    // `trace(s)`: the new line's index in `lines`, or `None` (JS `null`)
    let trace = |s: [f64; 2],
                 lines: &mut Vec<Vec<[f64; 3]>>,
                 cells: &mut HashMap<u64, Vec<[f64; 3]>>|
     -> Option<usize> {
        let mut line: Vec<[f64; 3]> = Vec::new();
        let (mut x, mut y, mut px, mut py) = (s[0], s[1], 0.0f64, 0.0f64);
        let mut k = 0.0f64;
        while k < max_steps {
            let Some(g) = grad(region_tris, x, y, h) else {
                break;
            };
            let m = jsnum::hypot(&[g[0], g[1]]);
            if m < min_slope {
                break;
            }
            let (dx, dy) = (-g[0] / m, -g[1] / m);
            if k != 0.0 && dx * px + dy * py < 0.0 {
                break; // a trough: it would turn back
            }
            if k > 2.0 && near(cells, x, y) {
                break;
            }
            line.push([x, y, g[2]]);
            px = dx;
            py = dy;
            x += dx * step;
            y += dy * step;
            k += 1.0;
        }
        if cover.is_some() {
            let (mut a, mut b) = (0usize, line.len());
            while a < b && !near_red(&red_cells, line[a][0], line[a][1]) {
                a += 1;
            }
            while b > a && !near_red(&red_cells, line[b - 1][0], line[b - 1][1]) {
                b -= 1;
            }
            line.truncate(b);
            line.drain(0..a);
        }
        if (line.len() as f64) < cfg.prop.min_stations {
            return None;
        }
        for q in &line {
            let key = grid_key(jsnum::floor(q[0] / join), jsnum::floor(q[1] / join));
            cells.entry(key).or_default().push(*q);
        }
        lines.push(line);
        Some(lines.len() - 1)
    };
    for &s in &seeds {
        trace(top[s], &mut lines, &mut cells);
    }

    // Coverage seeds: the highest red no line reaches starts a line of its own. A
    // seed that traces nothing still counts its own reach as tried, so the loop ends.
    if let Some(cover) = cover {
        let mut done = vec![false; cover.len()];
        let mut at: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, p) in cover.iter().enumerate() {
            at.entry(rkey(p[0], p[1])).or_default().push(i);
        }
        let reached = |l: &[[f64; 3]], done: &mut Vec<bool>| {
            for q in l {
                let i0 = jsnum::floor(q[0] / reach);
                let j0 = jsnum::floor(q[1] / reach);
                for di in [-1.0, 0.0, 1.0] {
                    for dj in [-1.0, 0.0, 1.0] {
                        for &c in at.get(&grid_key(i0 + di, j0 + dj)).into_iter().flatten() {
                            if !done[c]
                                && jsnum::hypot(&[q[0] - cover[c][0], q[1] - cover[c][1]]) <= reach
                            {
                                done[c] = true;
                            }
                        }
                    }
                }
            }
        };
        for l in &lines {
            reached(l, &mut done);
        }
        let mut order: Vec<usize> = (0..cover.len()).filter(|&i| !done[i]).collect();
        order.js_sort_by(|&a, &b| {
            cmp_diff(or(cover[b][2] - cover[a][2], || {
                or(cover[a][0] - cover[b][0], || cover[a][1] - cover[b][1])
            }))
        });
        for i in order {
            if done[i] {
                continue;
            }
            if lines.len() as f64 >= d.max_lines {
                break;
            }
            match trace([cover[i][0], cover[i][1]], &mut lines, &mut cells) {
                Some(li) => {
                    let l = lines[li].clone();
                    reached(&l, &mut done);
                }
                None => reached(&[cover[i]], &mut done),
            }
        }
    }
    if lines.is_empty() { None } else { Some(lines) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 x 20 plane z = slope * x as two triangles, with its corner points.
    fn ramp(slope: f64) -> (Vec<[f64; 3]>, Tris) {
        let c = |x: f64, y: f64| [x, y, slope * x];
        let (a, b, d, e) = (c(0.0, 0.0), c(20.0, 0.0), c(20.0, 20.0), c(0.0, 20.0));
        let tris = Tris::new([a, b, d, a, d, e].concat());
        (vec![a, b, d, e], tris)
    }

    #[test]
    fn a_level_or_empty_region_has_no_downhill() {
        let cfg = Config::default();
        let (pts, tris) = ramp(0.0);
        let o = DownhillOpts::default();
        assert!(downhill_lines(&pts, &tris, 1.0, 12.0, &o, &cfg).is_none());
        // |grad z| = 0.04 is under minSlope (0.05): level
        let (pts, tris) = ramp(0.04);
        assert!(downhill_lines(&pts, &tris, 1.0, 12.0, &o, &cfg).is_none());
        assert!(downhill_lines(&[], &Tris::new(vec![]), 1.0, 12.0, &o, &cfg).is_none());
    }

    #[test]
    fn a_ramp_gets_lines_running_down_it_every_pitch() {
        let cfg = Config::default();
        let (pts, tris) = ramp(0.5);
        let lines = downhill_lines(&pts, &tris, 1.0, 12.0, &DownhillOpts::default(), &cfg).unwrap();
        assert!(
            lines.len() >= 2,
            "the top edge's two ends, and one per pitch between"
        );
        for l in &lines {
            assert!(l.len() as f64 >= cfg.prop.min_stations);
            for w in l.windows(2) {
                assert!(w[1][0] < w[0][0], "downhill is -x");
                assert!((w[1][2] - 0.5 * w[1][0]).abs() < 1e-9); // z is the surface's
            }
        }
        // lines stand apart across the slope by at least `join` (3 mm), else they would have merged
        let ys: Vec<f64> = lines.iter().map(|l| l[0][1]).collect();
        for (i, a) in ys.iter().enumerate() {
            for b in &ys[i + 1..] {
                assert!((a - b).abs() >= 3.0 - 1e-9);
            }
        }
    }

    #[test]
    fn cover_trims_each_line_to_the_red_and_seeds_lines_for_uncovered_red() {
        let cfg = Config::default();
        let (pts, tris) = ramp(0.5);
        // red only near the foot of the ramp: lines are cut to within `reach` of it
        let red = [[2.0, 10.0, 1.0], [3.0, 10.5, 1.5]];
        let o = DownhillOpts {
            h: None,
            cover: Some(&red),
            reach: Some(2.0),
        };
        let lines = downhill_lines(&pts, &tris, 1.0, 12.0, &o, &cfg).unwrap();
        for l in &lines {
            assert!(
                l.iter().all(|p| p[0] <= 5.0 + 1e-9),
                "nothing far past the red"
            );
        }
        // red the seeds miss (a far corner) starts a line of its own
        let far = [[18.0, 1.0, 9.0]];
        let o = DownhillOpts {
            cover: Some(&far),
            reach: Some(3.0),
            ..Default::default()
        };
        let lines = downhill_lines(&pts, &tris, 1.0, 12.0, &o, &cfg).unwrap();
        // every kept station is within `reach` of the red
        assert!(
            lines
                .iter()
                .flatten()
                .all(|p| ((p[0] - 18.0).powi(2) + (p[1] - 1.0).powi(2)).sqrt() <= 3.0)
        );
        // maxLines caps the coverage seeds
        let many: Vec<[f64; 3]> = (0..4).map(|i| [12.0, 1.0 + 6.0 * i as f64, 6.0]).collect();
        let o = DownhillOpts {
            cover: Some(&many),
            reach: Some(3.0),
            ..Default::default()
        };
        let uncapped = downhill_lines(&pts, &tris, 1.0, 12.0, &o, &cfg).unwrap();
        let mut capped = Config::default();
        capped.downhill.max_lines = 1.0;
        let few = downhill_lines(&pts, &tris, 1.0, 12.0, &o, &capped).unwrap();
        assert!(
            few.len() < uncapped.len(),
            "{} vs {}",
            few.len(),
            uncapped.len()
        );
    }

    #[test]
    fn grad_falls_back_to_one_sided_differences_at_the_edge() {
        let (_, tris) = ramp(0.5);
        let h = 0.5;
        let g = grad(&tris, 20.0, 10.0, h).unwrap(); // x + h is off the region
        assert!((g[0] - 0.5).abs() < 1e-12 && g[1].abs() < 1e-12 && (g[2] - 10.0).abs() < 1e-12);
        let g = grad(&tris, 0.0, 10.0, h).unwrap(); // x - h is off the region
        assert!((g[0] - 0.5).abs() < 1e-12);
        assert!(grad(&tris, 30.0, 10.0, h).is_none());
    }
}
