//! Port of `web/orient.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Orientation suggester -- ranked, never auto-applied.
//!
//! The product rule (v1 spec, and the "human stays in the loop" video beat) is that the user
//! rotates; this only proposes. It returns 2-3 candidate poses with their tradeoffs and an honest
//! confidence, and the UI applies one on a click.
//!
//! We score for PRINTABILITY, not strength, on purpose. Strength wants the part tilted onto a
//! slant so the layer lines run diagonally -- but which slant is strongest depends on the load
//! direction, which geometry cannot infer (the spike's four parts each picked a different pose
//! for pull vs. lever). Worse, a strength-first solver is exactly what produced the "155mm tall,
//! balanced on its own needle, two sail-sized fins" pose the spike flagged: geometrically valid,
//! terrible print. So the auto suggester minimises support instead, and the stronger-but-tilted
//! pose is offered only once the user places the load arrow.
//!
//! Cheap first pass over every candidate using analyze() (allocation-free), then the full
//! buildFins() on the few finalists to measure how much of the overhang can actually be finned.
//! Candidates need no convex-hull kernel: a part rests on a flat face, so the large-area normal
//! clusters ARE its stable down-faces.
//!
//! Line numbers are at the checkout's commit (23039e2).

use indexmap::IndexMap;
use rayon::prelude::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use crate::config::Config;
use crate::fins::build::{FinsOpts, build_fins};
use crate::fins::seating::SeatingKind;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology, analyze};
use crate::types::{Rot3, Xyz};

type V3 = [f64; 3];

/// Port of `rotFromTo` (web/orient.js:28-62): column-major (THREE.Matrix3.elements) rotation
/// taking unit `a` onto unit `b`.
#[allow(clippy::many_single_char_names)]
pub fn rot_from_to(a: V3, b: V3) -> Rot3 {
    let cx = a[1] * b[2] - a[2] * b[1];
    let cy = a[2] * b[0] - a[0] * b[2];
    let cz = a[0] * b[1] - a[1] * b[0];
    let s2 = cx * cx + cy * cy + cz * cz; // sin^2 of the angle
    let c = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    if s2 < 1e-12 {
        if c > 0.0 {
            return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]; // already aligned
        }
        // antiparallel: 180 about any axis perpendicular to a
        let (mut px, mut py, mut pz) = (1.0, 0.0, 0.0);
        if jsnum::abs(a[0]) > 0.9 {
            px = 0.0;
            py = 1.0;
        }
        let (dx, dy, dz) = (a[0], a[1], a[2]);
        let d = px * dx + py * dy + pz * dz;
        px -= d * dx;
        py -= d * dy;
        pz -= d * dz;
        let pn = jsnum::hypot(&[px, py, pz]);
        px /= pn;
        py /= pn;
        pz /= pn;
        // R = 2 p p^T - I
        return [
            2.0 * px * px - 1.0,
            2.0 * py * px,
            2.0 * pz * px,
            2.0 * px * py,
            2.0 * py * py - 1.0,
            2.0 * pz * py,
            2.0 * px * pz,
            2.0 * py * pz,
            2.0 * pz * pz - 1.0,
        ];
    }
    // Rodrigues with k = (1-c)/s2 = 1/(1+c)
    let k = 1.0 / (1.0 + c);
    // R = I + [v]x + k [v]x^2, v = cross. Written out column-major.
    [
        1.0 + k * (-cz * cz - cy * cy),
        cz + k * (cy * cx),
        -cy + k * (cz * cx),
        -cz + k * (cx * cy),
        1.0 + k * (-cz * cz - cx * cx),
        cx + k * (cz * cy),
        cy + k * (cx * cz),
        -cx + k * (cy * cz),
        1.0 + k * (-cy * cy - cx * cx),
    ]
}

/// Port of `mul` (web/orient.js:65-75): multiply two column-major 3x3s: returns A*B. (Unused by
/// the rest of orient.js, as there.)
#[allow(dead_code)]
fn mul(a: &Rot3, b: &Rot3) -> Rot3 {
    let mut o = [0.0; 9];
    for col in 0..3 {
        for row in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += a[k * 3 + row] * b[col * 3 + k];
            }
            o[col * 3 + row] = s;
        }
    }
    o
}

/// Port of `applyRot` (web/orient.js:78-84): apply a column-major 3x3 (THREE.Matrix3.elements
/// order) to a vector: R * v.
fn apply_rot(r: &Rot3, v: V3) -> V3 {
    [
        r[0] * v[0] + r[3] * v[1] + r[6] * v[2],
        r[1] * v[0] + r[4] * v[1] + r[7] * v[2],
        r[2] * v[0] + r[5] * v[1] + r[8] * v[2],
    ]
}

/// Port of `rotAxis` (web/orient.js:87-95): column-major rotation of `deg` about a unit world
/// axis. (Unused by the rest of orient.js, as there.)
#[allow(dead_code)]
fn rot_axis(axis: V3, deg: f64) -> Rot3 {
    let a = (deg * std::f64::consts::PI) / 180.0;
    let (c, s) = (jsnum::cos(a), jsnum::sin(a));
    let [x, y, z] = axis;
    let t = 1.0 - c;
    [
        t * x * x + c,
        t * x * y + s * z,
        t * x * z - s * y,
        t * x * y - s * z,
        t * y * y + c,
        t * y * z + s * x,
        t * x * z + s * y,
        t * y * z - s * x,
        t * z * z + c,
    ]
}

/// A normal bin of `candidateDowns`: `{ a, x, y, z }`.
struct Bin {
    a: f64,
    x: f64,
    y: f64,
    z: f64,
}

/// Port of `candidateDowns` (web/orient.js:103-132): candidate DOWN-directions (the outward
/// normal that will face the plate): the part's large flat faces, found by binning face normals
/// by area, plus the six axis directions so a boxy part is always covered. Yaw about vertical is
/// omitted -- overhang, height and bed area are all invariant under it.
///
/// `maxClusters` is JS's `maxClusters = 12`; callers pass it.
pub fn candidate_downs(topo: &Topology, max_clusters: usize) -> Vec<V3> {
    candidate_downs_with(topo, max_clusters, WEB_BUCKETS, WEB_DEDUP_COS)
}

/// `q(v) = Math.round(v * 12) / 12` of `candidateDowns`: buckets per unit of normal component.
const WEB_BUCKETS: f64 = 12.0;
/// The dedup threshold of `candidateDowns` (`> 0.996`, about 5.1 degrees).
const WEB_DEDUP_COS: f64 = 0.996;

/// The wide search's candidate set: the web's generator at a finer step. `density` is the
/// factor on the candidate budget (the web is 1.0, the wide search 10.0): the cluster cap grows so
/// the whole list (clusters plus the six axes) can reach `density` times the web's 18, and the two
/// angular steps (the normal bucket and the near-duplicate threshold) shrink by `sqrt(density)`, so a curved part, whose normals spread over the sphere, yields about
/// `density` times as many distinct poses. A part with few distinct face normals (a box) has no
/// more stable down-faces to find and keeps what it has.
pub fn wide_candidate_downs(topo: &Topology, density: f64) -> Vec<V3> {
    let density = if density.is_finite() && density > 1.0 {
        density
    } else {
        1.0
    };
    if density == 1.0 {
        return candidate_downs(topo, 12);
    }
    let s = density.sqrt();
    let max_clusters = (18.0 * density - 6.0).round() as usize;
    let dedup_cos = (WEB_DEDUP_COS.acos() / s).cos();
    candidate_downs_with(topo, max_clusters, WEB_BUCKETS * s, dedup_cos)
}

fn candidate_downs_with(
    topo: &Topology,
    max_clusters: usize,
    buckets: f64,
    dedup_cos: f64,
) -> Vec<V3> {
    let (nrm, area, n_faces) = (&topo.nrm, &topo.area, topo.n_faces);
    // keyed by the JS template string `${q(x)},${q(y)},${q(z)}`, in first-seen order
    let mut bins: IndexMap<String, Bin> = IndexMap::new();
    let q = |v: f64| jsnum::round(v * buckets) / buckets; // ~5deg buckets (web)
    for f in 0..n_faces {
        let (x, y, z) = (nrm[f * 3], nrm[f * 3 + 1], nrm[f * 3 + 2]);
        if x == 0.0 && y == 0.0 && z == 0.0 {
            continue;
        }
        let key = format!(
            "{},{},{}",
            jsnum::to_js_string(q(x)),
            jsnum::to_js_string(q(y)),
            jsnum::to_js_string(q(z))
        );
        match bins.get_mut(&key) {
            Some(b) => {
                b.a += area[f];
                b.x += x * area[f];
                b.y += y * area[f];
                b.z += z * area[f];
            }
            None => {
                bins.insert(
                    key,
                    Bin {
                        a: area[f],
                        x: x * area[f],
                        y: y * area[f],
                        z: z * area[f],
                    },
                );
            }
        }
    }
    let mut sorted: Vec<&Bin> = bins.values().collect();
    // `.sort((p, r) => r.a - p.a)`: stable; a NaN difference compares as 0
    sorted.sort_by(|p, r| {
        let d = r.a - p.a;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let mut downs: Vec<V3> = sorted
        .into_iter()
        .take(max_clusters)
        .map(|b| {
            let n = jsnum::hypot(&[b.x, b.y, b.z]);
            [b.x / n, b.y / n, b.z / n]
        })
        .collect();
    downs.extend([
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ]);
    // dedup near-duplicate directions
    let mut out: Vec<V3> = Vec::new();
    for d in downs {
        if !out
            .iter()
            .any(|e| e[0] * d[0] + e[1] * d[1] + e[2] * d[2] > dedup_cos)
        {
            out.push(d);
        }
    }
    out
}

/// `DOWN` (web/orient.js:134).
pub const DOWN: V3 = [0.0, 0.0, -1.0];

/// One ranked pose of `suggestOrientations` (web/orient.js:177-195): the metrics the UI shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub rot: Rot3,
    pub over_area: f64,
    pub bed_area: f64,
    pub height: f64,
    /// Full `{x, y, z}` so callers can run `layerVerdict` on the pose.
    pub size: Xyz,
    pub regions: usize,
    /// `walls`, `coverage`, `volume`, `bore` and `seating` are JS `null` / `'unknown'` (`None`) when
    /// the ranking skipped the fin builds (a part over [`RANK_CHEAP_FACES`]): nothing measured them.
    pub walls: Option<usize>,
    pub coverage: Option<f64>,
    pub volume: Option<f64>,
    /// `None` is JS's `'unknown'`.
    pub seating: Option<SeatingKind>,
    pub holes: usize,
    pub bore: Option<usize>,
    pub score: f64,
}

/// `confidence` of `suggestOrientations`: how clearly the top pick beats the runner-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
    /// A part that can only balance on a point has no printable pose at all.
    None,
}

impl Confidence {
    /// The JS string.
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
            Confidence::None => "none",
        }
    }
}

/// What `suggestOrientations` returns: `{ candidates, confidence }`.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestions {
    pub candidates: Vec<Candidate>,
    pub confidence: Confidence,
}

/// `.sort((p, r) => p.x - r.x)` comparator: stable, a NaN difference compares as 0.
fn cmp_diff(d: f64) -> std::cmp::Ordering {
    if d < 0.0 {
        std::cmp::Ordering::Less
    } else if d > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

/// Above this many faces Suggest ranks on pass 1 alone, with no fin build per finalist. Port of
/// `RANK_CHEAP_FACES` (web/orient.js:128-135). Each build is ~40 s on a 2.3M-face mask and several
/// minutes in a browser Worker short on memory, so the five finalists left "Ranking…" up for 15+
/// minutes. Those candidates carry walls/coverage/volume/bore = `None` and seating `None`
/// (`'unknown'`: no build measured them); the list then shows no fin count. The comparison is
/// strict (`topo.nFaces > cheapAbove`): a part of exactly this many faces is still built.
pub const RANK_CHEAP_FACES: f64 = 1_000_000.0;

/// The options of `suggestOrientations` beyond `top` and `threshold` (web/orient.js:143):
/// `cheapAbove` (default [`RANK_CHEAP_FACES`]) and `onProgress`.
#[derive(Clone, Copy)]
pub struct SuggestOpts<'a> {
    /// `cheapAbove`: a part with more faces than this is ranked without fin builds.
    pub cheap_above: f64,
    /// `onProgress(f)`: called with the fraction done (0..1) after every pose it scores (a finalist's
    /// fin build weighs 20 poses). Pass 1 and the builds run on several threads, so calls can arrive
    /// out of order from different threads; the values are the same set the JS produces, in order.
    pub on_progress: Option<&'a (dyn Fn(f64) + Sync)>,
}

impl Default for SuggestOpts<'_> {
    fn default() -> Self {
        SuggestOpts {
            cheap_above: RANK_CHEAP_FACES,
            on_progress: None,
        }
    }
}

/// Port of `suggestOrientations` (web/orient.js:138-234): rank orientations for printability.
/// Returns up to `top` candidates, best first, each with the metrics the UI shows and a
/// `confidence`. JS defaults `top = 3`, `threshold = 45`; callers pass them. `cfg` is the config
/// the finalists' `buildFins` runs under.
///
/// **The analyze-aliasing hazard (issue #23), reproduced.** JS `analyze` returns the topology's
/// scratch arrays: every result of pass 1 shares ONE `over` / `kept` / `onBed`, which hold the flags
/// of the LAST `analyze` call (the last candidate of `downs`, before the sort). Pass 2 hands each
/// finalist's result to `buildFins`, so every finalist is built from its own regions, areas, size
/// and offset but the last candidate's face flags. No stage of `buildFins` writes those arrays
/// (checked: the only writes are in `analyze`). Here each `Analysis` owns its arrays, so the
/// finalists get a copy with the last candidate's flags put back.
pub fn suggest_orientations(
    topo: &Arc<Topology>,
    top: usize,
    threshold: f64,
    cfg: &Config,
) -> Suggestions {
    suggest_orientations_opts(topo, top, threshold, cfg, &SuggestOpts::default())
}

/// [`suggest_orientations`] with its `cheapAbove` and `onProgress` options.
pub fn suggest_orientations_opts(
    topo: &Arc<Topology>,
    top: usize,
    threshold: f64,
    cfg: &Config,
    opts: &SuggestOpts,
) -> Suggestions {
    let downs = candidate_downs(topo, 12);
    rank_downs(topo, &downs, top, threshold, cfg, &|| false, opts).expect("never cancelled")
}

/// The wide search: `suggestOrientations` over [`wide_candidate_downs`] (`density` times the web's
/// candidate budget), scored and worded exactly as the web scores them. `cancelled` is polled
/// between candidates (from several threads); once it is true the work stops and the result is
/// `None`.
pub fn suggest_orientations_wide(
    topo: &Arc<Topology>,
    top: usize,
    threshold: f64,
    cfg: &Config,
    density: f64,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Suggestions> {
    suggest_orientations_wide_opts(
        topo,
        top,
        threshold,
        cfg,
        density,
        cancelled,
        &SuggestOpts::default(),
    )
}

/// [`suggest_orientations_wide`] with its `cheapAbove` and `onProgress` options.
pub fn suggest_orientations_wide_opts(
    topo: &Arc<Topology>,
    top: usize,
    threshold: f64,
    cfg: &Config,
    density: f64,
    cancelled: &(dyn Fn() -> bool + Sync),
    opts: &SuggestOpts,
) -> Option<Suggestions> {
    let downs = wide_candidate_downs(topo, density);
    rank_downs(topo, &downs, top, threshold, cfg, cancelled, opts)
}

/// [`suggest_orientations`] with a cancel flag polled between candidates.
pub fn suggest_orientations_cancellable(
    topo: &Arc<Topology>,
    top: usize,
    threshold: f64,
    cfg: &Config,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Suggestions> {
    suggest_orientations_cancellable_opts(
        topo,
        top,
        threshold,
        cfg,
        cancelled,
        &SuggestOpts::default(),
    )
}

/// [`suggest_orientations_cancellable`] with its `cheapAbove` and `onProgress` options.
pub fn suggest_orientations_cancellable_opts(
    topo: &Arc<Topology>,
    top: usize,
    threshold: f64,
    cfg: &Config,
    cancelled: &(dyn Fn() -> bool + Sync),
    opts: &SuggestOpts,
) -> Option<Suggestions> {
    let downs = candidate_downs(topo, 12);
    rank_downs(topo, &downs, top, threshold, cfg, cancelled, opts)
}

/// A pose after pass 1.
struct Scored {
    rot: Rot3,
    a: Analysis,
    cheap: f64,
}

/// The two passes over a candidate list. Both fan out over Rayon's global pool and collect in
/// candidate order (an indexed collect), so the stable sorts and their tie-breaks see exactly the
/// serial sequence.
fn rank_downs(
    topo: &Arc<Topology>,
    downs: &[V3],
    top: usize,
    threshold: f64,
    cfg: &Config,
    cancelled: &(dyn Fn() -> bool + Sync),
    opts: &SuggestOpts,
) -> Option<Suggestions> {
    let cheap_only = topo.n_faces as f64 > opts.cheap_above;
    // A finalist's fin build costs far more than an analyze() -- ~20x on a 2M-face part -- so
    // progress weighs it that much more. (Integer weights: the sum is exact in any order.)
    let n_final = if cheap_only {
        0
    } else {
        downs.len().min(top.saturating_add(2).max(5))
    };
    let total = (downs.len() + n_final * 20) as f64;
    let done = AtomicU64::new(0);
    let tick = |w: u64| {
        let d = done.fetch_add(w, AtomicOrdering::SeqCst) + w;
        if let Some(f) = opts.on_progress {
            f(jsnum::min(1.0, d as f64 / total));
        }
    };
    // pass 1: cheap analyze() on every candidate
    let last = downs.len().saturating_sub(1);
    let scored: Vec<Option<Scored>> = downs
        .par_iter()
        .enumerate()
        .map(|(i, &d)| {
            if cancelled() {
                return None;
            }
            let rot = rot_from_to(d, DOWN);
            let mut a = analyze(topo, threshold, &rot);
            // The small overhangs analyze() drops as slivers ARE the hole ceilings, slot roofs and
            // bore tops that print rough by surprise -- a pose that turns them UP prints them
            // clean with no support, so fewer is better. This is the quality-first counterpart to
            // the bore-guard: it gives the "rotate so the hole points up" warning a pose to
            // actually land on.
            let holes = (a.raw_region_count - a.regions.len()) as f64;
            // printability proxy: overhang area dominates, bed contact helps adhesion, height is
            // a mild time/tipping penalty, holes-in-overhang a rough-surface penalty. Lower is
            // better.
            let cheap = a.over_area
                + jsnum::max(0.0, 200.0 - a.bed_area) * 0.25
                + a.size.z * 2.0
                + holes * 2.0;
            tick(1);
            // Only the last candidate's flags are ever read (the aliasing below), so the others
            // give theirs back: a wide search would otherwise hold hundreds of face-flag sets.
            if i != last {
                a.over = Vec::new();
                a.kept = Vec::new();
                a.on_bed = Vec::new();
            }
            Some(Scored { rot, a, cheap })
        })
        .collect();
    let mut scored: Vec<Scored> = scored.into_iter().collect::<Option<_>>()?;
    // (the aliased flags: the last analyze call's, before the sort)
    let last_flags = scored
        .last()
        .map(|s| (s.a.over.clone(), s.a.kept.clone(), s.a.on_bed.clone()));
    scored.sort_by(|p, r| cmp_diff(p.cheap - r.cheap));

    if cheap_only {
        return Some(rank_cheap(&scored, top));
    }

    // pass 2: full fin build on the finalists, to measure real finnable coverage
    scored.truncate(n_final);
    let built: Vec<Option<Candidate>> = scored
        .into_par_iter()
        .map(|mut s| {
            if cancelled() {
                return None;
            }
            if let Some((over, kept, on_bed)) = &last_flags {
                s.a.over = over.clone();
                s.a.kept = kept.clone();
                s.a.on_bed = on_bed.clone();
            }
            let b = build_fins(
                topo,
                &s.a,
                &s.rot,
                &FinsOpts {
                    mode: Some("prop".to_string()),
                    bed_pad: Some(true),
                    ..Default::default()
                },
                cfg,
            );
            tick(20);
            // buildFins reports `unserved` = overhang regions that took NO wall; coverage is
            // DISPLAY only (how many fins the user will get), never a score driver -- rewarding
            // coverage rewards ADDING supports, and printability wants fewer.
            let n_reg = s.a.regions.len() as f64;
            let coverage = if n_reg != 0.0 {
                (n_reg - b.unserved) / n_reg
            } else {
                1.0
            };
            let point = b.seating.kind == SeatingKind::Point;
            // Holes/slots/bore tops in overhang (the dropped slivers) print rough. Walls inside a
            // bore are supported (Matthew's print tests, 2026-09-27) but still have to be pulled
            // out of the hole, so a pose that points the holes UP stays mildly preferred. Both
            // weighted below the plastic/height terms so they tip near-ties, not so hard they
            // chase a few holes into a worse-supported pose.
            let holes = s.a.raw_region_count - s.a.regions.len();
            let bore = b.props.iter().filter(|q| q.in_bore == Some(true)).count();
            // Printability cost, lower is better: the overhang that needs supporting at all
            // dominates; then the plastic the fins themselves cost, a height penalty (layers =
            // time, and tall = tippy), a bed-adhesion bonus, and the rough-hole penalty. A part
            // balanced on a point is unprintable in this pose -- push it to the bottom, not out,
            // so it still shows with an honest "nothing can hold this".
            let score = s.a.over_area
                + (b.volume / 1000.0) * 2.0
                + s.a.size.z * 3.0
                + jsnum::max(0.0, 300.0 - s.a.bed_area) * 0.5
                + holes as f64 * 2.0
                + bore as f64 * 5.0
                + if point { 1e6 } else { 0.0 };
            Some(Candidate {
                rot: s.rot,
                over_area: s.a.over_area,
                bed_area: s.a.bed_area,
                height: s.a.size.z,
                size: s.a.size,
                regions: s.a.regions.len(),
                walls: Some(b.props.len()),
                coverage: Some(coverage),
                volume: Some(b.volume),
                seating: Some(b.seating.kind),
                holes,
                bore: Some(bore),
                score,
            })
        })
        .collect();
    let mut built: Vec<Candidate> = built.into_iter().collect::<Option<_>>()?;
    built.sort_by(|p, r| cmp_diff(p.score - r.score));

    // Confidence is the honest part, and it is about the POSE, not the fins: how clearly does the
    // top pick beat the runner-up? A big separation means "print it this way"; a near-tie means
    // several orientations are about as good and the user's judgement (or the load, once placed)
    // should decide. A part that can only balance on a point has no printable pose at all.
    let mut confidence = Confidence::High;
    match (built.first(), built.get(1)) {
        (None, _) => confidence = Confidence::None,
        (Some(best), _) if best.seating == Some(SeatingKind::Point) => {
            confidence = Confidence::None
        }
        (Some(best), Some(second)) => {
            let sep = (second.score - best.score) / jsnum::max(1.0, best.score);
            confidence = if sep > 0.25 {
                Confidence::High
            } else if sep > 0.08 {
                Confidence::Medium
            } else {
                Confidence::Low
            };
        }
        _ => {}
    }
    built.truncate(top);
    Some(Suggestions {
        candidates: built,
        confidence,
    })
}

/// Port of `rankCheap` (web/orient.js:236-259): `suggestOrientations`' answer from pass 1 alone (a
/// part over [`RANK_CHEAP_FACES`]): the same candidate fields, the fin-build ones `None`, and
/// confidence from the pass-1 scores, capped at 'medium' since no build checked them. No build
/// means no 'point' seating check either, so it is never 'none' for a pose; pass 1's small-bed
/// penalty is all that holds a pose balanced on a point down.
fn rank_cheap(scored: &[Scored], top: usize) -> Suggestions {
    let candidates: Vec<Candidate> = scored
        .iter()
        .take(top)
        .map(|s| {
            let a = &s.a;
            Candidate {
                rot: s.rot,
                over_area: a.over_area,
                bed_area: a.bed_area,
                height: a.size.z,
                size: a.size,
                regions: a.regions.len(),
                walls: None,
                coverage: None,
                volume: None,
                seating: None,
                holes: a.raw_region_count - a.regions.len(),
                bore: None,
                score: s.cheap,
            }
        })
        .collect();
    let (best, second) = (candidates.first(), candidates.get(1));
    let mut confidence = if best.is_some() {
        Confidence::High
    } else {
        Confidence::None
    };
    if let (Some(best), Some(second)) = (best, second) {
        let sep = (second.score - best.score) / jsnum::max(1.0, best.score);
        confidence = if sep > 0.08 {
            Confidence::Medium
        } else {
            Confidence::Low
        };
    }
    if best.is_some() && second.is_none() {
        confidence = Confidence::Medium;
    }
    Suggestions {
        candidates,
        confidence,
    }
}

/// Port of `crossFraction` (web/orient.js:281-284): cos of the angle between two unit-ish
/// directions in the same frame. `None` is JS's `null`.
fn cross_fraction(dir: V3) -> Option<f64> {
    let n = jsnum::hypot(&[dir[0], dir[1], dir[2]]);
    if n < 1e-9 {
        None
    } else {
        Some(jsnum::abs(dir[2]) / n) // |d . buildZ| = share pulling across layers
    }
}

/// How well a load lines up with the layers: `alignQuality`'s `quality`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Good,
    Mixed,
    Poor,
}

impl Quality {
    /// The JS string: `'good'`, `'mixed'` or `'poor'`.
    pub fn as_str(self) -> &'static str {
        match self {
            Quality::Good => "good",
            Quality::Mixed => "mixed",
            Quality::Poor => "poor",
        }
    }
}

/// `{ quality, text }` of `alignQuality`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Align {
    pub quality: Quality,
    pub text: &'static str,
}

/// Port of `alignQuality` (web/orient.js:287-303): bucket a cross-fraction into an honest
/// three-step verdict + copy.
fn align_quality(cross: f64) -> Align {
    // cross = |load . build-Z|: 0 = load lies in the layer plane (strong),
    // 1 = load pulls straight across the layers (weak). Bounds at 60deg / 30deg
    // from the plate: within 30deg of in-plane reads "well aligned".
    if cross <= 0.5 {
        return Align {
            quality: Quality::Good,
            text: "The load runs along the layers — the strong direction. Good.",
        };
    }
    if cross <= 0.866 {
        return Align {
            quality: Quality::Mixed,
            text: "The load partly crosses the layers.",
        };
    }
    Align {
        quality: Quality::Poor,
        text: "The load pulls straight across the layers — where prints split first.",
    }
}

/// `layerVerdict`'s `posture`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Posture {
    Weak,
    Strong,
    Mixed,
}

impl Posture {
    /// The JS string: `'weak'`, `'strong'` or `'mixed'`.
    pub fn as_str(self) -> &'static str {
        match self {
            Posture::Weak => "weak",
            Posture::Strong => "strong",
            Posture::Mixed => "mixed",
        }
    }
}

/// Port of `layerVerdict` (web/orient.js:314-322): how the CURRENT pose lays the part down, as a
/// posture only. A printed part is weakest across its layer lines -- the flat, horizontal planes
/// -- so it splits most easily when pulled straight up the build axis: standing the longest span
/// up the layers is the classic weak print, lying it flat is the strong one. The always-on text
/// readout for this was dropped (it earned its keep only in the weak case), so the posture now
/// feeds one place: the strength caveat on a suggested pose. `size` is the seated bounding box
/// {x, y, z}, z the build height.
pub fn layer_verdict(size: &Xyz) -> Posture {
    let z = size.z;
    let mut dims = [size.x, size.y, size.z];
    // `.sort((a, b) => a - b)`: stable, a NaN difference compares as 0
    dims.sort_by(|a, b| {
        let d = a - b;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    let span = dims[2];
    let eps = 0.05 * jsnum::max(1.0, span);
    if z >= span - eps {
        return Posture::Weak;
    }
    if z <= dims[0] + eps {
        return Posture::Strong;
    }
    Posture::Mixed
}

/// What `loadAlignment` returns: `{ cross, quality, text }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadAlignment {
    pub cross: f64,
    pub quality: Quality,
    pub text: &'static str,
}

/// Port of `loadAlignment` (web/orient.js:332-336): qualitative strength readout for a load in
/// the CURRENT pose. `dir_world` is the load direction in seated print space (so world +Z is the
/// build axis). Pull model only: the critical tensile direction IS the applied force. (Lever --
/// where the part bends and the tensile stress runs along the beam, perpendicular to the push --
/// picks a different pose and can't be inferred from one arrow; it's a future toggle, not a
/// default.) `None` is JS's `null`.
pub fn load_alignment(dir_world: V3) -> Option<LoadAlignment> {
    let cross = cross_fraction(dir_world)?;
    let a = align_quality(cross);
    Some(LoadAlignment {
        cross,
        quality: a.quality,
        text: a.text,
    })
}

/// What `suggestStrengthPose` returns: `{ rot, cross, quality, text, height, over, bedArea }`.
#[derive(Clone, Debug, PartialEq)]
pub struct StrengthPose {
    pub rot: Rot3,
    pub cross: f64,
    pub quality: Quality,
    pub text: &'static str,
    pub height: f64,
    pub over: f64,
    pub bed_area: f64,
}

/// Port of `suggestStrengthPose` (web/orient.js:348-385): the strongest PRINTABLE pose for a
/// given load. The arrow is anchored to a feature, so its direction is fixed in the part's own
/// (as-loaded) frame; as we try candidate orientations that direction swings relative to the
/// build axis and we want the pose that lays it most in-plane.
///
/// The guard is the whole point. A pure strength solver is exactly what produced the spike's
/// "155mm tall, balanced on its own needle" pose -- geometrically the layers were perfect, the
/// print was impossible. So we only consider poses that actually SIT on the plate (a real bed
/// footprint, not a point) and whose printability cost is within a bounded budget of the best
/// available; among those, we pick the strongest. Returns `None` if nothing beats simply not
/// tilting.
///
/// `threshold` is JS's `{ threshold = 45 }`; callers pass it. Every `analyze` result is a
/// scalar read here, so the analyze-aliasing hazard (issue #23) does not arise: no flag array
/// of an earlier result is read after a later `analyze`.
pub fn suggest_strength_pose(
    topo: &Topology,
    dir_local: V3,
    threshold: f64,
) -> Option<StrengthPose> {
    let dl = {
        let n = jsnum::hypot(&[dir_local[0], dir_local[1], dir_local[2]]);
        if n < 1e-9 {
            None
        } else {
            Some([dir_local[0] / n, dir_local[1] / n, dir_local[2] / n])
        }
    }?;

    struct Cand {
        rot: Rot3,
        cross: f64,
        print_cost: f64,
        height: f64,
        over: f64,
        bed_area: f64,
    }
    let cands: Vec<Cand> = candidate_downs(topo, 12)
        .into_iter()
        .map(|d| {
            let rot = rot_from_to(d, DOWN);
            let a = analyze(topo, threshold, &rot);
            let cross = jsnum::abs(apply_rot(&rot, dl)[2]); // rot orthonormal, dl unit -> result unit
            // Same printability proxy suggestOrientations' pass 1 uses (overhang dominates, bed
            // contact helps, height is a mild penalty) -- lower is better.
            let print_cost =
                a.over_area + jsnum::max(0.0, 200.0 - a.bed_area) * 0.25 + a.size.z * 2.0;
            Cand {
                rot,
                cross,
                print_cost,
                height: a.size.z,
                over: a.over_area,
                bed_area: a.bed_area,
            }
        })
        .collect();

    // A pose with ~no bed contact is a tip/needle -- unprintable regardless of how nicely the
    // layers line up. Drop those before ranking; if that leaves nothing, there's no printable
    // pose to recommend.
    let seated: Vec<&Cand> = cands.iter().filter(|c| c.bed_area >= 1.0).collect();
    if seated.is_empty() {
        return None;
    }

    // Bound the strength search by printability: only poses within a budget of the most
    // printable one are eligible, so we never trade a sane print for a tower.
    let best_print = jsnum::min_of(&seated.iter().map(|c| c.print_cost).collect::<Vec<_>>());
    let mut affordable: Vec<&Cand> = seated
        .into_iter()
        .filter(|c| c.print_cost <= best_print + 60.0)
        .collect();
    // `.sort((p, r) => p.cross - r.cross)`: stable; a NaN difference compares as 0
    affordable.sort_by(|p, r| {
        let d = p.cross - r.cross;
        if d < 0.0 {
            std::cmp::Ordering::Less
        } else if d > 0.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });

    // (JS reads `best.rot` of `undefined` and throws when a NaN cost empties the list)
    let best = affordable
        .first()
        .expect("suggestStrengthPose: no affordable pose (NaN printability cost)");
    let a = align_quality(best.cross);
    Some(StrengthPose {
        rot: best.rot,
        cross: best.cross,
        quality: a.quality,
        text: a.text,
        height: best.height,
        over: best.over,
        bed_area: best.bed_area,
    })
}
