//! Port of `web/prop/tracks.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Where the auto-placer ("Suggest") puts walls under a region: `split_region` cuts it into
//! locally-straight sub-patches, `patch_tracks` lays straight parallel tracks across each,
//! and `tube_line` gives a curved region one wall along its lowest line.

use crate::guards::{JsSort, cmp_f64};
use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Region, Topology};
use crate::planes::face_adjacency;
use crate::prop::bridge::{Support, flat_run_axis};
use crate::prop::contact::{contact_line, straightness};
use crate::prop::endrows::end_rows;
use crate::prop::surface::surface_z_at;
use crate::tris::Tris;
use crate::types::Rot3;

/// `step` and `nU` of `lowEdges` (web/prop/tracks.js:489).
const LOW_EDGE_STEP: f64 = 0.25;
const LOW_EDGE_N_U: usize = 9;

/// What a JS `sort` comparator returning `diff` means: negative first, positive last, zero or
/// NaN equal.
fn cmp_diff(diff: f64) -> std::cmp::Ordering {
    if diff < 0.0 {
        std::cmp::Ordering::Less
    } else if diff > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// Split one overhang region into locally-straight sub-patches: `[{ faces, area }]`.
///
/// `splitRegion(topo, faces, rot)` (web/prop/tracks.js:44-89).
pub fn split_region(topo: &Topology, faces: &[usize], rot: &Rot3, cfg: &Config) -> Vec<Region> {
    let nrm = &topo.nrm;
    let area = &topo.area;
    let agree_cut = jsnum::cos((cfg.prop.split_agree_deg * std::f64::consts::PI) / 180.0);
    let adj = face_adjacency(topo);
    let (start, nbr) = (&adj.start, &adj.nbr);

    // rotated normals, for this region's faces only
    let mut rn: HashMap<usize, [f64; 3]> = HashMap::new();
    for &f in faces {
        let (x, y, z) = (nrm[f * 3], nrm[f * 3 + 1], nrm[f * 3 + 2]);
        rn.insert(
            f,
            [
                rot[0] * x + rot[3] * y + rot[6] * z,
                rot[1] * x + rot[4] * y + rot[7] * z,
                rot[2] * x + rot[5] * y + rot[8] * z,
            ],
        );
    }

    let mut order: Vec<usize> = faces.to_vec();
    order.sort_by(|&a, &b| cmp_f64(area[b], area[a]));
    let mut assigned: HashSet<usize> = HashSet::new();
    let mut patches: Vec<Region> = Vec::new();

    for &seed in &order {
        if assigned.contains(&seed) {
            continue;
        }
        let [sx, sy, sz] = rn[&seed];
        assigned.insert(seed);
        let mut members = vec![seed];
        let mut total = area[seed];
        let mut queue = vec![seed];

        while let Some(f) = queue.pop() {
            for e in start[f]..start[f + 1] {
                let g = nbr[e as usize] as usize;
                if assigned.contains(&g) {
                    continue;
                }
                let Some(gn) = rn.get(&g) else {
                    continue; // absent = not in this region
                };
                if gn[0] * sx + gn[1] * sy + gn[2] * sz < agree_cut {
                    continue;
                }
                assigned.insert(g);
                members.push(g);
                total += area[g];
                queue.push(g);
            }
        }
        patches.push(Region {
            faces: members,
            area: total,
        });
    }
    patches
}

/// The single lowest-line wall for a CURVED region -- the tube case. `None` (JS `null`) when
/// this region is not a tube; otherwise the lines with at least `minStations` stations.
///
/// `tubeLine(topo, faces, rot, pts, regionTris, step = PROP.stationStep)`
/// (web/prop/tracks.js:118-220); callers pass the default step explicitly.
pub fn tube_line(
    topo: &Topology,
    faces: &[usize],
    rot: &Rot3,
    pts: &[[f64; 3]],
    region_tris: &Tris,
    step: f64,
    cfg: &Config,
) -> Option<Vec<Vec<[f64; 3]>>> {
    let prop = &cfg.prop;
    let nrm = &topo.nrm;
    let area = &topo.area;

    let mut region_area = 0.0f64;
    for &f in faces {
        region_area += area[f];
    }
    // Below tubeMinArea only a CONVEX tube may take this route (checked once the lowest
    // line is known below).
    let small = region_area < prop.tube_min_area;
    if small && region_area < prop.tube_small_min_area {
        return None;
    }

    // area-weighted mean normal, in print space
    let (mut mx, mut my, mut mz, mut big_a) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut rn: Vec<([f64; 3], f64)> = Vec::new();
    for &f in faces {
        let (x, y, z) = (nrm[f * 3], nrm[f * 3 + 1], nrm[f * 3 + 2]);
        let v = [
            rot[0] * x + rot[3] * y + rot[6] * z,
            rot[1] * x + rot[4] * y + rot[7] * z,
            rot[2] * x + rot[5] * y + rot[8] * z,
        ];
        rn.push((v, area[f]));
        big_a += area[f];
        mx += v[0] * area[f];
        my += v[1] * area[f];
        mz += v[2] * area[f];
    }
    let mn = jsnum::hypot(&[mx, my, mz]);
    if mn < 1e-9 || big_a < 1e-9 {
        return None;
    }
    mx /= mn;
    my /= mn;
    mz /= mn;

    // The FRACTION of area that deviates, never the worst face.
    let cut = jsnum::cos((prop.tube_spread_deg * std::f64::consts::PI) / 180.0);
    let mut deviant = 0.0f64;
    for (v, a) in &rn {
        if v[0] * mx + v[1] * my + v[2] * mz < cut {
            deviant += a;
        }
    }
    if deviant / big_a < prop.tube_curved_frac {
        return None; // flat: rows handle it
    }

    // The lowest line, from the mesh's own points.
    let mut d_lo = [f64::INFINITY, f64::INFINITY];
    let mut d_hi = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in pts {
        for a in 0..2 {
            if p[a] < d_lo[a] {
                d_lo[a] = p[a];
            }
            if p[a] > d_hi[a] {
                d_hi[a] = p[a];
            }
        }
    }
    let diag = jsnum::hypot(&[d_hi[0] - d_lo[0], d_hi[1] - d_lo[1]]);
    let n_samples = jsnum::max(8.0, jsnum::min(400.0, jsnum::ceil(diag / step))) as usize;
    // A small tube is often meshed as a few long strips: sample the edges too.
    let rough = if small {
        contact_line(
            &edge_points(&region_tris.v, pts, step / 2.0),
            region_tris,
            n_samples,
            cfg,
        )
    } else {
        contact_line(pts, region_tris, n_samples, cfg)
    };
    let rough = rough?;
    if straightness(&rough) > prop.max_wander {
        return None; // a ring, not a tube
    }

    // So RESAMPLE it the way patchTracks samples a track.
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    for p in &rough {
        cx += p[0];
        cy += p[1];
    }
    cx /= rough.len() as f64;
    cy /= rough.len() as f64;
    let (mut sxx, mut sxy, mut syy) = (0.0f64, 0.0f64, 0.0f64);
    for p in &rough {
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let tr2 = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let lam = tr2 / 2.0 + jsnum::sqrt(jsnum::max(0.0, (tr2 * tr2) / 4.0 - det));
    let mut ux = sxy;
    let mut uy = lam - sxx;
    if jsnum::hypot(&[ux, uy]) < 1e-9 {
        ux = 1.0;
        uy = 0.0;
    }
    let un = jsnum::hypot(&[ux, uy]);
    ux /= un;
    uy /= un;
    let mut u_lo = f64::INFINITY;
    let mut u_hi = f64::NEG_INFINITY;
    for p in &rough {
        let u = (p[0] - cx) * ux + (p[1] - cy) * uy;
        if u < u_lo {
            u_lo = u;
        }
        if u > u_hi {
            u_hi = u;
        }
    }
    if u_hi - u_lo < 1e-6 {
        return None;
    }
    if small && !convex_about(&rn, &region_tris.v, cx, cy, ux, uy, cfg) {
        return None; // a pocket
    }
    let n_st = jsnum::max(2.0, jsnum::min(400.0, jsnum::ceil((u_hi - u_lo) / step))) as usize;

    let mut lines: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut cur: Vec<[f64; 3]> = Vec::new();
    for k in 0..=n_st {
        let u = u_lo + ((u_hi - u_lo) * k as f64) / n_st as f64;
        let x = cx + ux * u;
        let y = cy + uy * u;
        match surface_z_at(region_tris, x, y) {
            None => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Some(z) => cur.push([x, y, z]),
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    Some(
        lines
            .into_iter()
            .filter(|t| t.len() as f64 >= prop.min_stations)
            .collect(),
    )
}

/// `edgePoints(tris, pts, gap)` (web/prop/tracks.js:223-238): `pts` plus points every
/// <= `gap` mm along every edge of `tris`.
fn edge_points(tris: &[f64], pts: &[[f64; 3]], gap: f64) -> Vec<[f64; 3]> {
    let mut out = pts.to_vec();
    let mut t = 0;
    while t < tris.len() {
        for e in 0..3 {
            let a = t + e * 3;
            let b = t + ((e + 1) % 3) * 3;
            let len = jsnum::hypot(&[
                tris[b] - tris[a],
                tris[b + 1] - tris[a + 1],
                tris[b + 2] - tris[a + 2],
            ]);
            let n = jsnum::ceil(len / gap) as usize;
            for k in 1..n {
                let u = k as f64 / n as f64;
                out.push([
                    tris[a] + (tris[b] - tris[a]) * u,
                    tris[a + 1] + (tris[b + 1] - tris[a + 1]) * u,
                    tris[a + 2] + (tris[b + 2] - tris[a + 2]) * u,
                ]);
            }
        }
        t += 9;
    }
    out
}

/// Is this curved band the underside of a small TUBE (convex and two-sided)?
///
/// `convexAbout(rn, regionTris, cx, cy, ux, uy)` (web/prop/tracks.js:257-273). `rn`:
/// `[(normal, area)]` in print space, in the same order as the triangles of `region_tris`.
fn convex_about(
    rn: &[([f64; 3], f64)],
    region_tris: &[f64],
    cx: f64,
    cy: f64,
    ux: f64,
    uy: f64,
    cfg: &Config,
) -> bool {
    let wx = -uy; // across the line, in plan
    let wy = ux;
    let (mut away, mut toward, mut left, mut right) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (k, (n, a)) in rn.iter().enumerate() {
        let t = k * 9;
        let gx = (region_tris[t] + region_tris[t + 3] + region_tris[t + 6]) / 3.0;
        let gy = (region_tris[t + 1] + region_tris[t + 4] + region_tris[t + 7]) / 3.0;
        let d = (gx - cx) * wx + (gy - cy) * wy; // which side, how far
        if jsnum::abs(d) < 0.05 {
            continue; // on the line: no vote
        }
        let s = (n[0] * wx + n[1] * wy) * jsnum::sign(d);
        if s > 0.0 {
            away += a;
        } else {
            toward += a;
        }
        if d > 0.0 {
            left += a;
        } else {
            right += a;
        }
    }
    away > cfg.prop.tube_convex_frac * (away + toward)
        && jsnum::min(left, right) > cfg.prop.tube_two_sided_frac * (left + right)
}

/// The result of `patchTracks`: the JS array of tracks with `spacing` and `turned` properties
/// (web/prop/tracks.js:413-418). The early returns carry no `spacing` (`None`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tracks {
    pub lines: Vec<Vec<[f64; 3]>>,
    /// The widest lateral gap between adjacent rows (0 for one row).
    pub spacing: Option<f64>,
    /// The old-rule axis, when the bridge turned this patch (raced by prop/bridgerace.js).
    pub turned: Option<[f64; 2]>,
    /// `kept.flushable` (web/prop/tracks.js:422): a rows-only pass that left a free end's
    /// flush row to the race (prop/endrows.js). False on the early returns, where JS has none.
    pub flushable: bool,
}

/// The contact polylines for one locally-flat sub-patch: straight parallel tracks sampled on
/// the patch's own surface, spaced `span` apart, split wherever the patch does not cover them.
///
/// `patchTracks(pts, patchTris, step = PROP.stationStep, support = null,
/// span = PROP.maxUnsupportedSpan, axis = null, turn = true)` (web/prop/tracks.js:322-420);
/// callers pass the defaults explicitly.
pub fn patch_tracks(
    pts: &[[f64; 3]],
    patch_tris: &Tris,
    step: f64,
    support: Option<&Support>,
    span: f64,
    axis: Option<[f64; 2]>,
    turn: bool,
    cfg: &Config,
) -> Tracks {
    let prop = &cfg.prop;
    if pts.is_empty() {
        return Tracks {
            lines: Vec::new(),
            spacing: None,
            turned: None,
            flushable: false,
        };
    }

    // The 2x2 XY covariance of the patch, plus its cross-terms with Z.
    let (mut cx, mut cy, mut cz) = (0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        cx += p[0];
        cy += p[1];
        cz += p[2];
    }
    let np = pts.len() as f64;
    cx /= np;
    cy /= np;
    cz /= np;
    let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        let dz = p[2] - cz;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
        sxz += dx * dz;
        syz += dy * dz;
    }
    let det = sxx * syy - sxy * sxy;
    // underside plane z = a x + b y + c: gradient (a,b) = steepest descent
    let gx = if det > 1e-9 {
        (syy * sxz - sxy * syz) / det
    } else {
        0.0
    };
    let gy = if det > 1e-9 {
        (sxx * syz - sxy * sxz) / det
    } else {
        0.0
    };
    let slope = jsnum::hypot(&[gx, gy]);

    // Run direction for the parallel wall tracks (see the JS comment).
    let (mut ux, mut uy);
    let mut turned: Option<[f64; 2]> = None; // the old-rule axis, when the bridge turned this one
    if let Some(a) = axis {
        ux = a[0];
        uy = a[1];
    } else if slope >= prop.contour_slope_min {
        // run each wall DOWN the slope (steepest descent)
        ux = gx;
        uy = gy;
    } else {
        // near-flat ledge: cross the slicer's bridge lines (prop/bridge.js)
        // (`turn` false: the old pick, unraced -- the raster pass)
        let a = flat_run_axis(pts, patch_tris, step, support, cfg);
        if a.turned && !turn {
            let old = a.old.expect("a turned axis carries the old pick");
            ux = old[0];
            uy = old[1];
        } else {
            ux = a.axis[0];
            uy = a.axis[1];
            if a.turned {
                turned = a.old;
            }
        }
    }
    let un = jsnum::hypot(&[ux, uy]);
    ux /= un;
    uy /= un;
    let vx = -uy;
    let vy = ux;

    let (mut u_lo, mut u_hi, mut v_lo, mut v_hi) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for p in pts {
        let u = p[0] * ux + p[1] * uy;
        let t = p[0] * vx + p[1] * vy;
        if u < u_lo {
            u_lo = u;
        }
        if u > u_hi {
            u_hi = u;
        }
        if t < v_lo {
            v_lo = t;
        }
        if t > v_hi {
            v_hi = t;
        }
    }
    if u_hi - u_lo < 1e-6 {
        return Tracks {
            lines: Vec::new(),
            spacing: None,
            turned,
            flushable: false,
        };
    }
    let n_st = jsnum::max(2.0, jsnum::min(400.0, jsnum::ceil((u_hi - u_lo) / step))) as usize;

    // `span` is the requested row spacing from the coverage slider (see coverRowSpan).
    let row_span = jsnum::max(1.0, span);
    // Flush end rows at free edges are a candidate layout (prop/endrows.js): laid with
    // `ends` true, offered (flagged `.flushable`, mid-strip rows kept) with 'offer' --
    // buildProps races the two -- and withheld under Sway past its cutoff, where a
    // flush wall stands where the braces go (Sway is the user's own ask, the judge
    // doesn't see it).
    let ends = if !support.is_some_and(|s| s.sway)
        || slope <= jsnum::tan((prop.edge_flat_deg_sway * std::f64::consts::PI) / 180.0)
    {
        if support.is_some_and(|s| s.rows_only) {
            Ends::Offer
        } else {
            Ends::Lay
        }
    } else {
        Ends::Off
    };
    let probe = support.map(|s| Probe {
        support: s,
        patch_tris,
        ux,
        uy,
        vx,
        vy,
        u_lo,
        u_hi,
        ends,
    });
    let rows = row_offsets(v_lo, v_hi, row_span, probe.as_ref(), cfg);

    let mut tracks: Vec<Vec<[f64; 3]>> = Vec::new();
    for &v0 in &rows.rows {
        let mut cur: Vec<[f64; 3]> = Vec::new();
        for k in 0..=n_st {
            let u = u_lo + ((u_hi - u_lo) * k as f64) / n_st as f64;
            let x = ux * u + vx * v0;
            let y = uy * u + vy * v0;
            match surface_z_at(patch_tris, x, y) {
                None => {
                    if !cur.is_empty() {
                        tracks.push(std::mem::take(&mut cur));
                    }
                }
                Some(z) => cur.push([x, y, z]),
            }
        }
        if !cur.is_empty() {
            tracks.push(cur);
        }
    }
    let kept: Vec<Vec<[f64; 3]>> = tracks
        .into_iter()
        .filter(|t| t.len() as f64 >= prop.min_stations)
        .collect();
    // The lateral gap between adjacent rows this face ended up with.
    Tracks {
        lines: kept,
        spacing: Some(rows.spacing),
        turned,
        flushable: rows.flushable,
    }
}

/// `probe.ends` (web/prop/tracks.js:391-394): whether the free-edge candidate (`endRows`)
/// replaces the mid-strip rows (`true`), is only flagged (`'offer'`: the race decides), or is
/// off (`false`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ends {
    Lay,
    Offer,
    Off,
}

/// The `probe` object `rowOffsets` takes (web/prop/tracks.js:394-396).
pub(crate) struct Probe<'a> {
    pub support: &'a Support<'a>,
    pub patch_tris: &'a Tris,
    pub ux: f64,
    pub uy: f64,
    pub vx: f64,
    pub vy: f64,
    pub u_lo: f64,
    pub u_hi: f64,
    pub ends: Ends,
}

/// `rowOffsets`' array with its `.spacing`.
struct Rows {
    rows: Vec<f64>,
    spacing: f64,
    flushable: bool,
}

/// Where the rows go across a patch (v, perpendicular to the walls), with the widest gap
/// between two rows as `spacing` (0 for one row). The mid-strip layout puts each row vExt / n
/// apart, half that from each edge; `probe.ends` says whether the free-edge candidate
/// (`end_rows`) replaces it (`Lay`), is only flagged (`flushable`, `Offer`: the race decides),
/// or is off.
///
/// `rowOffsets(vLo, vHi, rowSpan, probe)` (web/prop/tracks.js:430-448).
fn row_offsets(v_lo: f64, v_hi: f64, row_span: f64, probe: Option<&Probe>, cfg: &Config) -> Rows {
    let v_ext = v_hi - v_lo;
    // (capped: a patch of absurd extent would be a vector of a billion rows)
    let n = jsnum::max(1.0, jsnum::round(v_ext / row_span));
    let n = (if n > crate::guards::MAX_STEPS {
        crate::guards::MAX_STEPS
    } else {
        n
    }) as usize;
    let mut rows: Vec<f64> = (0..n)
        .map(|w| v_lo + (v_ext * (w as f64 + 0.5)) / n as f64)
        .collect();
    let mut flushable = false;
    if let Some(pr) = probe
        && pr.ends != Ends::Off
        && let Some(f) = end_rows(&rows, v_lo, v_hi, pr, cfg)
    {
        if pr.ends == Ends::Lay {
            rows = f;
        } else {
            flushable = true;
        }
    }
    if let Some(pr) = probe {
        let edges = low_edges(pr, v_lo, v_hi, cfg);
        rows = low_edge_rows(rows, &edges, 2.0 * cfg.prop.edge_inset, cfg);
    }
    let mut widest = 0.0f64;
    for i in 1..rows.len() {
        widest = jsnum::max(widest, rows[i] - rows[i - 1]);
    }
    let spacing = if rows.len() > 1 { widest } else { 0.0 };
    Rows {
        rows,
        spacing,
        flushable,
    }
}

/// One row class of `lowEdges`.
#[derive(Clone, Copy, PartialEq)]
enum Cls {
    None,
    Ok,
    Low,
    Part,
}

/// `lowEdges`' result: `[{ v, dir }]` plus the `.low(v)` method.
struct LowEdges {
    edges: Vec<(f64, f64)>,
    cls: Vec<Cls>,
    v_lo: f64,
}

impl LowEdges {
    /// `edges.low(v)`: a row's own class, `low` or not.
    fn low(&self, v: f64) -> bool {
        let i = jsnum::max(
            0.0,
            jsnum::min(
                (self.cls.len() - 1) as f64,
                jsnum::round((v - self.v_lo) / LOW_EDGE_STEP),
            ),
        );
        self.cls[i as usize] == Cls::Low
    }
}

/// Where a near-flat patch sinks to the plate: each LOW edge, the row position `v` nearest a
/// band no wall can stand in, with `dir` pointing away from the band.
///
/// `lowEdges(probe, vLo, vHi)` (web/prop/tracks.js:485-528).
fn low_edges(probe: &Probe, v_lo: f64, v_hi: f64, cfg: &Config) -> LowEdges {
    let prop = &cfg.prop;
    let wall_z = prop.min_height_squat + prop.gap + 0.02; // clearance.js's squat floor
    let brim_z = prop.squat_brim_h + prop.gap + 0.02;
    let step = LOW_EDGE_STEP;
    let n_u = LOW_EDGE_N_U;
    let at = |u: f64, v: f64| {
        surface_z_at(
            probe.patch_tris,
            probe.ux * u + probe.vx * v,
            probe.uy * u + probe.vy * v,
        )
    };
    // A patch spanning more rows than any real part has (a stray vertex at 1e16: `v += 0.25` stops changing
    // `v` and the loop never ends) has no low edges to find: one row of nothing.
    if crate::guards::too_many_steps(v_hi - v_lo, step) {
        return LowEdges {
            edges: Vec::new(),
            cls: vec![Cls::None],
            v_lo,
        };
    }
    let mut vs: Vec<f64> = Vec::new();
    let mut cls: Vec<Cls> = Vec::new();
    let mut v = v_lo;
    while v <= v_hi + 1e-9 {
        cfg.cancel.check();
        let mut hit = 0usize;
        let mut ok = 0usize;
        let mut sunk = false;
        for i in 0..n_u {
            let u = probe.u_lo + ((probe.u_hi - probe.u_lo) * (i as f64 + 0.5)) / n_u as f64;
            let Some(z) = at(u, v) else {
                continue;
            };
            hit += 1;
            if z < wall_z {
                sunk = true;
                continue;
            }
            let b1 = at(u, v - prop.squat_brim_w);
            let b2 = at(u, v + prop.squat_brim_w);
            if b1.is_none_or(|z| z >= brim_z) && b2.is_none_or(|z| z >= brim_z) {
                ok += 1;
            }
        }
        vs.push(v);
        cls.push(if hit == 0 {
            Cls::None
        } else if ok * 2 >= hit {
            Cls::Ok
        } else if ok == 0 && sunk {
            Cls::Low
        } else {
            Cls::Part
        });
        v += step;
    }
    let mut edges: Vec<(f64, f64)> = Vec::new();
    // first OK row from k on, over PART rows
    let reach = |k: i64, dir: i64| -> i64 {
        let mut j = k;
        while j >= 0 && (j as usize) < cls.len() {
            if cls[j as usize] == Cls::Ok {
                return j;
            }
            if cls[j as usize] != Cls::Part {
                return -1;
            }
            j += dir;
        }
        -1
    };
    let mut k = 0usize;
    while k < cls.len() {
        if cls[k] != Cls::Low {
            k += 1;
            continue;
        }
        let mut j = k;
        while j + 1 < cls.len() && cls[j + 1] == Cls::Low {
            j += 1;
        }
        if ((j - k + 1) as f64) * step < prop.low_band_min {
            // a dip, not a band
            k = j + 1;
            continue;
        }
        let below = reach(k as i64 - 1, -1);
        let above = reach(j as i64 + 1, 1);
        if below >= 0 {
            edges.push((vs[below as usize], -1.0));
        }
        if above >= 0 {
            edges.push((vs[above as usize], 1.0));
        }
        k = j + 1;
    }
    LowEdges { edges, cls, v_lo }
}

/// `rowOffsets`' rows plus a row at each low edge with no row within a brim width of it.
/// Only ever ADDED.
///
/// `lowEdgeRows(rows, edges, near)` (web/prop/tracks.js:540-551).
fn low_edge_rows(rows: Vec<f64>, edges: &LowEdges, near: f64, cfg: &Config) -> Vec<f64> {
    let mut out = rows.clone();
    let brim = 2.0 * cfg.prop.squat_brim_w;
    for &(v, dir) in &edges.edges {
        let blocked = out.iter().any(|&r| {
            let d = (r - v) * dir;
            if d >= 0.0 {
                d < brim
            } else {
                -d < near || (-d < brim && !edges.low(r))
            }
        });
        if !blocked {
            out.push(v);
        }
    }
    if out.len() == rows.len() {
        rows
    } else {
        out.js_sort_by(|a, b| cmp_diff(a - b));
        out
    }
}
