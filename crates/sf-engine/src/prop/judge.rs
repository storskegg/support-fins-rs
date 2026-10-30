//! Port of `web/prop/judge.js`. Read `docs/AGENTS.md` before changing anything here.

/// `JUDGE` (web/prop/judge.js:28). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct JudgeConfig {
    // mm past a wall's face (provisional, see above)
    pub cantilever: f64,
    /// JS key `bridgeDeg`.
    // anchors this far apart, seen from the point, bridge it
    pub bridge_deg: f64,
    // direction bins for the nearest anchor each way
    pub sectors: f64,
    // mm2 per sample
    pub sample: f64,
    // What plastic buys: a race (raster.js raceAgainst `price`) swaps to walls that
    // weigh more only when they hold at least this much more overhang per mm3 added
    // (0.01 = 10 mm2 per cm3, ~1.24 g of PLA). PROVISIONAL, Matthew's to set: the
    // cable reel's rim buys 22 mm2/cm3, the lamp shade's 34; a wall for a sliver, ~0.
    pub price: f64,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        JudgeConfig {
            cantilever: 2.0,
            bridge_deg: 135.0,
            sectors: 16.0,
            sample: 1.0,
            price: 0.01,
        }
    }
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::config::Config;
use crate::inside::inside_part;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::surface::seat;
use crate::types::Rot3;

/// The JS module's `cache` WeakMap (web/prop/judge.js:30), which is keyed on the **analysis
/// result**: `{ anchors, samples }`. `Analysis` has no slot for it (a `derive` change to a shared
/// type needs sign-off), so the caller owns one `JudgeCache` per analysis and passes it in: the
/// same lifetime JS gets from the result's identity. Results never depend on the cache.
#[derive(Debug, Default)]
pub struct JudgeCache {
    anchors: OnceLock<Vec<[f64; 3]>>,
    samples: Mutex<HashMap<usize, Arc<Vec<[f64; 4]>>>>,
}

/// Port of `partAnchors` (web/prop/judge.js:46-62): part anchors for a pose: points every
/// 0.5 mm along each overhang edge whose non-overhang neighbour goes down.
pub fn part_anchors<'a>(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    cache: &'a JudgeCache,
) -> &'a [[f64; 3]] {
    crate::lazy::get_or_build(&cache.anchors, |_par| {
        let (pos, adj_a, adj_b) = (&topo.pos, &topo.adj_a, &topo.adj_b);
        let off = &result.offset;
        let s = |o: usize| seat(pos, o, rot, off);
        let mut pts = Vec::new();
        for k in 0..adj_a.len() {
            let (a, b) = (adj_a[k] as usize, adj_b[k] as usize);
            if (result.over[a] == 0) == (result.over[b] == 0) {
                continue;
            }
            let (o, n) = if result.over[a] != 0 { (a, b) } else { (b, a) };
            let Some(shared) = shared_edge(pos, o, n) else {
                continue;
            };
            let (p, q) = (s(shared[0]), s(shared[1]));
            if attached(topo, result, rot, o, n, &shared, &p, &q) {
                densify(&[p, q], 0.5, &mut pts);
            }
        }
        pts
    })
}

/// Port of `faceSamples` (web/prop/judge.js:65-85): sample points `[x, y, z, weight]` over face
/// `f`, seated, ~`JUDGE.sample` mm2 each. A CAD part's flat face is a few big triangles, and
/// one centroid swung its whole area.
pub fn face_samples(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    f: usize,
    cache: &JudgeCache,
    cfg: &Config,
) -> Arc<Vec<[f64; 4]>> {
    if let Some(s) = cache.samples.lock().unwrap().get(&f) {
        return s.clone();
    }
    let off = &result.offset;
    let a_ = seat(&topo.pos, f * 9, rot, off);
    let b_ = seat(&topo.pos, f * 9 + 3, rot, off);
    let c_ = seat(&topo.pos, f * 9 + 6, rot, off);
    let a = topo.area[f];
    let k = jsnum::max(
        1.0,
        jsnum::min(24.0, jsnum::ceil(jsnum::sqrt(a / cfg.judge.sample))),
    ) as usize;
    let w = a / (k * k) as f64;
    let at = |u: f64, v: f64| -> [f64; 3] {
        let c = |i: usize| a_[i] + (b_[i] - a_[i]) * u + (c_[i] - a_[i]) * v;
        [c(0), c(1), c(2)]
    };
    let kf = k as f64;
    let mut s = Vec::new();
    for i in 0..k {
        for j in 0..k - i {
            let (fi, fj) = (i as f64, j as f64);
            let p = at((fi + 1.0 / 3.0) / kf, (fj + 1.0 / 3.0) / kf);
            s.push([p[0], p[1], p[2], w]);
            if i + j < k - 1 {
                let p = at((fi + 2.0 / 3.0) / kf, (fj + 2.0 / 3.0) / kf);
                s.push([p[0], p[1], p[2], w]);
            }
        }
    }
    let s = Arc::new(s);
    cache.samples.lock().unwrap().insert(f, s.clone());
    s
}

/// What a [`Scorer`] says of a point (JS `'near' | 'bridged' | 'cant' | 'bare'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Near,
    Bridged,
    Cant,
    Bare,
}

/// A cell-hashed point set (`gridOf`, web/prop/judge.js:148-160).
struct Grid {
    cell: f64,
    map: HashMap<u64, Vec<[f64; 3]>>,
}

/// JS `key = (i, j) => i * 100003 + j` as a `Map` key (SameValueZero: -0 is 0, NaNs are one key).
pub(crate) fn grid_key(i: f64, j: f64) -> u64 {
    let k = i * 100003.0 + j;
    if k == 0.0 {
        0
    } else if k.is_nan() {
        u64::MAX
    } else {
        k.to_bits()
    }
}

impl Grid {
    fn of(pts: &[[f64; 3]], cell: f64) -> Grid {
        let mut map: HashMap<u64, Vec<[f64; 3]>> = HashMap::new();
        for p in pts {
            let k = grid_key(jsnum::floor(p[0] / cell), jsnum::floor(p[1] / cell));
            map.entry(k).or_default().push(*p);
        }
        Grid { cell, map }
    }
    fn each(&self, x: f64, y: f64, mut f: impl FnMut(&[f64; 3])) {
        let gi = jsnum::floor(x / self.cell);
        let gj = jsnum::floor(y / self.cell);
        for di in [-1.0, 0.0, 1.0] {
            for dj in [-1.0, 0.0, 1.0] {
                if let Some(v) = self.map.get(&grid_key(gi + di, gj + dj)) {
                    for p in v {
                        f(p);
                    }
                }
            }
        }
    }
}

/// A scorer for one set of wall tops (`scorer`'s returned closure).
pub struct Scorer {
    walls: Grid,
    part: Grid,
    span: f64,
    bridge_max: f64,
    cant_max: f64,
    n: usize,
    apart: f64,
}

/// Port of `scorer` (web/prop/judge.js:92-120): a scorer for one set of wall tops (wall lines'
/// points, densified here to 0.5 mm), with `part_pts` (`part_anchors`) as anchors too.
/// [`Scorer::score`] is the JS closure `(x, y, z) -> 'near' | 'bridged' | 'cant' | 'bare'`.
pub fn scorer(tops: &[Vec<[f64; 3]>], part_pts: &[[f64; 3]], cfg: &Config) -> Scorer {
    let span = cfg.prop.max_unsupported_span;
    let bridge_max = span + cfg.prop.th;
    let cant_max = cfg.judge.cantilever + cfg.prop.th / 2.0;
    let mut dense = Vec::new();
    for line in tops {
        densify(line, 0.5, &mut dense);
    }
    let walls = Grid::of(&dense, bridge_max);
    let part = Grid::of(part_pts, bridge_max);
    let n_f = cfg.judge.sectors;
    let apart = jsnum::ceil((cfg.judge.bridge_deg / 360.0) * n_f);
    Scorer {
        walls,
        part,
        span,
        bridge_max,
        cant_max,
        n: n_f as usize,
        apart,
    }
}

impl Scorer {
    /// Classify the point `(x, y, z)`.
    pub fn score(&self, x: f64, y: f64, z: f64) -> Hold {
        let n = self.n;
        let n_f = n as f64;
        let mut best = vec![f64::INFINITY; n];
        let mut look = |grid: &Grid| {
            grid.each(x, y, |p| {
                let dz = z - p[2];
                if dz < -0.05 || dz > 1.5 {
                    return;
                }
                let d = jsnum::hypot(&[p[0] - x, p[1] - y]);
                if d > self.bridge_max {
                    return;
                }
                let s = jsnum::floor(
                    ((jsnum::atan2(p[1] - y, p[0] - x) + std::f64::consts::PI)
                        / (2.0 * std::f64::consts::PI))
                        * n_f,
                ) % n_f;
                if s.is_nan() {
                    return; // a NaN index reads and writes nothing
                }
                let s = s as usize;
                if d < best[s] {
                    best[s] = d;
                }
            });
        };
        look(&self.walls);
        look(&self.part);
        let mut nearest = f64::INFINITY;
        for &d in &best {
            if d < nearest {
                nearest = d;
            }
        }
        if nearest <= self.cant_max {
            return Hold::Near;
        }
        for i in 0..n {
            if best[i] == f64::INFINITY {
                continue;
            }
            for k in i + 1..n {
                let sep = (k - i) as f64;
                if jsnum::min(sep, n_f - sep) >= self.apart && best[i] + best[k] <= self.bridge_max
                {
                    return Hold::Bridged;
                }
            }
        }
        if nearest <= self.span / 2.0 {
            Hold::Cant
        } else {
            Hold::Bare
        }
    }
}

/// Port of `judgeRuler` (web/prop/judge.js:128-145): a race ruler (raster.js `raceAgainst`'s
/// `ruler`): `(faces, tops) -> held area`, `faces` in `overhangFaces`' shape
/// `[cx, cy, cz, area, region, face]` (the face index is element 5), `tops` the wall lines'
/// points, flattened.
pub fn judge_ruler<'a>(
    topo: &'a Topology,
    result: &'a Analysis,
    rot: &'a Rot3,
    cache: &'a JudgeCache,
    cfg: &'a Config,
) -> impl Fn(&[[f64; 6]], &[[f64; 3]]) -> f64 + 'a {
    let part_pts = part_anchors(topo, result, rot, cache);
    move |faces: &[[f64; 6]], tops: &[[f64; 3]]| {
        // (tops come flattened, every wall's points in one list: each its own line, so
        // none is joined to the next wall's)
        let held = scorer(
            &tops.iter().map(|p| vec![*p]).collect::<Vec<_>>(),
            part_pts,
            cfg,
        );
        let mut sum = 0.0f64;
        for fc in faces {
            for s in face_samples(topo, result, rot, fc[5] as usize, cache, cfg).iter() {
                let c = held.score(s[0], s[1], s[2]);
                if c == Hold::Near || c == Hold::Bridged {
                    sum += s[3];
                }
            }
        }
        sum
    }
}

/// Port of `densify` (web/prop/judge.js:147-160): `line`'s points, plus points every `step` mm
/// between neighbours, pushed onto `into`.
fn densify(line: &[[f64; 3]], step: f64, into: &mut Vec<[f64; 3]>) {
    for i in 0..line.len() {
        let p = line[i];
        into.push(p);
        let Some(&q) = line.get(i + 1) else { break };
        let n = jsnum::floor(jsnum::hypot(&[q[0] - p[0], q[1] - p[1], q[2] - p[2]]) / step);
        let mut k = 1.0;
        while k < n {
            let t = k / n;
            into.push([
                p[0] + (q[0] - p[0]) * t,
                p[1] + (q[1] - p[1]) * t,
                p[2] + (q[2] - p[2]) * t,
            ]);
            k += 1.0;
        }
    }
}

/// Port of `attached` (web/prop/judge.js:174-188): does the part go DOWN from overhang face
/// `o`'s edge `p`-`q` (seated; `shared` its offsets, `n` the face across it)? By the face across
/// it -- or, for a part built of unwelded bodies (a Tinkercad union: a deck laid on a post), by
/// the part itself just outside the edge and under it.
pub fn attached(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    o: usize,
    n: usize,
    shared: &[usize; 2],
    p: &[f64; 3],
    q: &[f64; 3],
) -> bool {
    let (pos, off) = (&topo.pos, &result.offset);
    let far = seat(pos, n * 9 + 3 * other_vertex(pos, n, shared), rot, off);
    let z = jsnum::min(p[2], q[2]);
    if far[2] < z - 0.2 {
        return true;
    }
    let c: Vec<[f64; 3]> = (0..3).map(|i| seat(pos, o * 9 + i * 3, rot, off)).collect();
    let mut nx = p[1] - q[1];
    let mut ny = q[0] - p[0];
    let len = jsnum::hypot(&[nx, ny]);
    if len < 1e-9 {
        return false;
    }
    let (mx, my) = ((p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0);
    let cx = (c[0][0] + c[1][0] + c[2][0]) / 3.0;
    let cy = (c[0][1] + c[1][1] + c[2][1]) / 3.0;
    if nx * (mx - cx) + ny * (my - cy) < 0.0 {
        nx = -nx;
        ny = -ny;
    }
    inside_part(
        topo,
        rot,
        off,
        mx + (nx / len) * 0.5,
        my + (ny / len) * 0.5,
        z - 0.5,
    )
}

/// Port of `sharedEdge` (web/prop/judge.js:191-199): the two vertex offsets (into `pos`) face `o`
/// shares with face `n`, by position; `None` (JS `null`) unless exactly two.
pub fn shared_edge(pos: &[f64], o: usize, n: usize) -> Option<[usize; 2]> {
    let mut out = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            let a = o * 9 + i * 3;
            let b = n * 9 + j * 3;
            if pos[a] == pos[b] && pos[a + 1] == pos[b + 1] && pos[a + 2] == pos[b + 2] {
                out.push(a);
                break;
            }
        }
    }
    if out.len() == 2 {
        Some([out[0], out[1]])
    } else {
        None
    }
}

/// Port of `otherVertex` (web/prop/judge.js:201-207): which vertex (0-2) of face `n` is not on
/// the `shared` edge.
pub fn other_vertex(pos: &[f64], n: usize, shared: &[usize; 2]) -> usize {
    for j in 0..3 {
        let b = n * 9 + j * 3;
        if !shared
            .iter()
            .any(|&a| pos[a] == pos[b] && pos[a + 1] == pos[b + 1] && pos[a + 2] == pos[b + 2])
        {
            return j;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(x: f64) -> Vec<Vec<[f64; 3]>> {
        vec![vec![[x, 0.0, 0.0]]]
    }

    #[test]
    fn a_scorer_tells_near_bridged_cant_and_bare() {
        let cfg = Config::default();
        let span = cfg.prop.max_unsupported_span;
        let cant_max = cfg.judge.cantilever + cfg.prop.th / 2.0;
        let bridge_max = span + cfg.prop.th;
        assert!(
            cant_max < span / 2.0,
            "the default numbers leave a 'cant' band"
        );
        // near: an anchor within cantilever past a wall's face
        let s = scorer(&wall(0.0), &[], &cfg);
        assert_eq!(s.score(1.0, 0.0, 0.5), Hold::Near);
        // cant: one side only, past the cantilever but within half a span
        let d = (cant_max + span / 2.0) / 2.0;
        assert_eq!(s.score(d, 0.0, 0.5), Hold::Cant);
        // bare: past half a span, with or without an anchor in reach
        assert_eq!(s.score(span / 2.0 + 0.5, 0.0, 0.5), Hold::Bare);
        assert_eq!(s.score(bridge_max + 5.0, 0.0, 0.5), Hold::Bare);
        // bridged: anchors on opposite sides whose distances add up to a span (plus a wall)
        let a = bridge_max / 2.0 - 0.01;
        assert!(a > cant_max);
        let two = vec![vec![[-a, 0.0, 0.0]], vec![[a, 0.0, 0.0]]];
        let s = scorer(&two, &[], &cfg);
        assert_eq!(s.score(0.0, 0.0, 0.5), Hold::Bridged);
        // the same pair too far apart is not a bridge
        let wide = vec![vec![[-a - 0.5, 0.0, 0.0]], vec![[a + 0.5, 0.0, 0.0]]];
        assert_eq!(scorer(&wide, &[], &cfg).score(0.0, 0.0, 0.5), Hold::Bare);
        // two anchors on the SAME side never bridge
        let same = vec![vec![[a, 0.0, 0.0]], vec![[a, 0.5, 0.0]]];
        assert_ne!(scorer(&same, &[], &cfg).score(0.0, 0.0, 0.5), Hold::Bridged);
    }

    #[test]
    fn only_anchors_0_to_1_5_mm_under_the_point_count() {
        let cfg = Config::default();
        let s = scorer(&wall(0.0), &[], &cfg);
        assert_eq!(s.score(0.5, 0.0, 1.5), Hold::Near); // 1.5 under: still an anchor
        assert_ne!(s.score(0.5, 0.0, 1.6), Hold::Near); // further under: not
        assert_eq!(s.score(0.5, 0.0, -0.05), Hold::Near); // 0.05 above is allowed
        assert_ne!(s.score(0.5, 0.0, -0.06), Hold::Near);
    }

    #[test]
    fn part_points_are_anchors_too() {
        let cfg = Config::default();
        let s = scorer(&[], &[[0.0, 0.0, 0.0]], &cfg);
        assert_eq!(s.score(1.0, 0.0, 0.5), Hold::Near);
    }

    #[test]
    fn densify_adds_a_point_every_step_between_neighbours() {
        let mut out = Vec::new();
        densify(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], 0.5, &mut out);
        assert_eq!(out, vec![[0.0, 0.0, 0.0], [0.5, 0.0, 0.0], [1.0, 0.0, 0.0]]);
        let mut out = Vec::new();
        densify(&[[0.0, 0.0, 0.0], [0.4, 0.0, 0.0]], 0.5, &mut out); // closer than a step: just the ends
        assert_eq!(out.len(), 2);
        let mut out = Vec::new();
        densify(&[], 0.5, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn shared_edge_needs_exactly_two_shared_vertices() {
        let tri = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let mut pos = tri.to_vec();
        pos.extend([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0]); // shares (0,0,0) and (1,0,0)
        assert_eq!(shared_edge(&pos, 0, 1), Some([0, 3]));
        assert_eq!(other_vertex(&pos, 1, &[0, 3]), 2);
        let mut one = tri.to_vec();
        one.extend([0.0, 0.0, 0.0, 5.0, 5.0, 5.0, 6.0, 5.0, 5.0]); // one shared vertex
        assert_eq!(shared_edge(&one, 0, 1), None);
    }
}
