//! Port of `web/prop/rim.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M2)

/// `RIM` (web/prop/rim.js:32). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct RimConfig {
    /// JS key `pieceLen`.
    // mm: the longest rim piece
    pub piece_len: f64,
    /// JS key `pieceTurn`.
    // deg: the most a piece may turn (a U can't be pulled out)
    pub piece_turn: f64,
    /// JS key `pieceGap`.
    // mm between pieces
    pub piece_gap: f64,
    /// JS key `levelDeg`.
    // deg off horizontal: a rim stands under a LEVEL region (most of its area) (a flange, a shelf -- the
    // "flat + curved edges" class). A sloped patch's rows already run downhill to
    // its edges (the flush end rows, #143/#269); a rim round a tilted face just
    // rings it with walls (tube X25: 8 for 4).
    pub level_deg: f64,
}

impl Default for RimConfig {
    fn default() -> Self {
        RimConfig {
            piece_len: 40.0,
            piece_turn: 90.0,
            piece_gap: 1.5,
            level_deg: 10.0,
        }
    }
}

use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::judge::{attached, grid_key, shared_edge};
use crate::prop::surface::{seat, surface_z_at};
use crate::tris::Tris;
use crate::types::Rot3;

/// A rim track or a row: `[x, y, z]` stations. JS tags every rim track `rim: true` (so the wall
/// built on it says `rimWall: true`); every track `rim_tracks` returns is one.
pub type Track = Vec<[f64; 3]>;

/// Port of `rimTracks` (web/prop/rim.js:48-70): the rim tracks for a region (`faces`, its seated
/// triangles `region_tris`), or empty when it has no free edge. `step` is JS's default
/// `PROP.stationStep`, passed explicitly.
///
/// Only a LEVEL region (RIM.levelDeg, most of its area) gets a rim -- its whole outline, a
/// flange's rounded lip included. The outline is cut into pieces (RIM.pieceLen long,
/// RIM.pieceTurn of turn at most, RIM.pieceGap apart) so each can come out.
pub fn rim_tracks(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    faces: &[usize],
    region_tris: &Tris,
    step: f64,
    cfg: &Config,
) -> Vec<Track> {
    let level = -jsnum::cos((cfg.rim.level_deg * std::f64::consts::PI) / 180.0);
    let n = &topo.nrm;
    let (mut flat, mut all) = (0.0f64, 0.0f64);
    for &f in faces {
        all += topo.area[f];
        if rot[2] * n[f * 3] + rot[5] * n[f * 3 + 1] + rot[8] * n[f * 3 + 2] <= level {
            flat += topo.area[f];
        }
    }
    if flat < all / 2.0 {
        return Vec::new();
    }
    let own: HashSet<usize> = faces.iter().copied().collect();
    let segs = free_edges(topo, result, rot, &own, cfg);
    let mut tracks: Vec<Track> = Vec::new();
    let gap = piece_gap(step, cfg);
    for chain in chains(&segs) {
        let mut line = resample(&chain, step);
        // a closed outline (a round flange) leaves a gap at its seam too, or its last
        // piece meets its first and the ring can't come out
        let (a, b) = (chain[0], chain[chain.len() - 1]);
        if jsnum::hypot(&[a[0] - b[0], a[1] - b[1], a[2] - b[2]]) < 1e-6 {
            line.truncate(line.len().saturating_sub(gap));
        }
        let line = inset(&line, region_tris, cfg);
        for run in runs(&line) {
            tracks.extend(pieces(&run, step, cfg));
        }
    }
    // (tagged, so the wall built on it says rimWall: true)
    tracks
        .into_iter()
        .filter(|t| t.len() as f64 >= cfg.prop.min_stations)
        .collect()
}

/// `Math.max(1, Math.round(RIM.pieceGap / step))`, the gap between pieces in stations.
fn piece_gap(step: f64, cfg: &Config) -> usize {
    jsnum::max(1.0, jsnum::round(cfg.rim.piece_gap / step)) as usize
}

/// Port of `clipToRim` (web/prop/rim.js:77-100): `rows` with every station within a wall and its
/// side clearance of the rim dropped (a row is then split into its runs): a row stops short of the
/// rim instead of fusing into it, and a flush end row along a straight rim goes.
pub fn clip_to_rim(rows: &[Track], rim: &[Track], cfg: &Config) -> Vec<Track> {
    if rim.is_empty() {
        return rows.to_vec();
    }
    let keep = cfg.prop.th + cfg.prop.side_clear;
    // rim stations by `keep` cells
    let mut grid: HashMap<u64, Vec<[f64; 3]>> = HashMap::new();
    for q in rim.iter().flatten() {
        let k = grid_key(jsnum::floor(q[0] / keep), jsnum::floor(q[1] / keep));
        grid.entry(k).or_default().push(*q);
    }
    let near = |p: &[f64; 3]| -> bool {
        let i = jsnum::floor(p[0] / keep);
        let j = jsnum::floor(p[1] / keep);
        for di in [-1.0, 0.0, 1.0] {
            for dj in [-1.0, 0.0, 1.0] {
                for q in grid.get(&grid_key(i + di, j + dj)).into_iter().flatten() {
                    if jsnum::hypot(&[q[0] - p[0], q[1] - p[1]]) < keep {
                        return true;
                    }
                }
            }
        }
        false
    };
    let mut out: Vec<Track> = Vec::new();
    for row in rows {
        let mut cur: Track = Vec::new();
        for p in row {
            if near(p) {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            } else {
                cur.push(*p);
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out.into_iter()
        .filter(|t| t.len() as f64 >= cfg.prop.min_stations)
        .collect()
}

/// Port of `freeEdges` (web/prop/rim.js:103-122): the patch's free outline edges, seated:
/// `[p, q]` pairs.
///
/// FREE: the patch meets part that goes UP from the edge (a rim curling up, a plate's side), a
/// sliver of overhang too small for a region (a rounded edge's lower half), or nothing. Not free:
/// a kept overhang face (the overhang carries on), or part going DOWN (a ledge's spine, a
/// flange's hub -- the layer builds out from it), or an edge down at the bed (it rests there).
fn free_edges(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    own: &HashSet<usize>,
    cfg: &Config,
) -> Vec<[[f64; 3]; 2]> {
    let (pos, adj_a, adj_b) = (&topo.pos, &topo.adj_a, &topo.adj_b);
    let off = &result.offset;
    let mut segs = Vec::new();
    for k in 0..adj_a.len() {
        let (a, b) = (adj_a[k] as usize, adj_b[k] as usize);
        if own.contains(&a) == own.contains(&b) {
            continue;
        }
        let (o, n) = if own.contains(&a) { (a, b) } else { (b, a) };
        // the overhang carries on -- unless only as a sliver the engine drops (the lower
        // half of a rounded edge: the reel's round lip, beyond the flat patch's edge)
        if result.over[n] != 0 && result.kept[n] != 0 {
            continue;
        }
        let Some(shared) = shared_edge(pos, o, n) else {
            continue;
        };
        let (p, q) = (
            seat(pos, shared[0], rot, off),
            seat(pos, shared[1], rot, off),
        );
        if jsnum::min(p[2], q[2]) < cfg.prop.min_height_squat {
            continue; // resting at the bed
        }
        if attached(topo, result, rot, o, n, &shared, &p, &q) {
            continue; // part goes down
        }
        segs.push([p, q]);
    }
    segs
}

/// The station key of `chains`: the point rounded to 1e-4, as the JS template string.
fn chain_key(p: &[f64; 3]) -> String {
    format!(
        "{},{},{}",
        jsnum::to_js_string(jsnum::round(p[0] * 1e4)),
        jsnum::to_js_string(jsnum::round(p[1] * 1e4)),
        jsnum::to_js_string(jsnum::round(p[2] * 1e4))
    )
}

/// Port of `chains` (web/prop/rim.js:125-155): segments joined end to end into polylines (a
/// closed loop comes back closed).
fn chains(segs: &[[[f64; 3]; 2]]) -> Vec<Track> {
    let mut at: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        for p in s {
            at.entry(chain_key(p)).or_default().push(i);
        }
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    let grow = |line: &mut Track, from: &[f64; 3], used: &mut Vec<bool>| loop {
        let last_key = chain_key(&line[line.len() - 1]);
        let next = at
            .get(&last_key)
            .and_then(|v| v.iter().copied().find(|&i| !used[i]));
        let Some(next) = next else { return };
        used[next] = true;
        let [p, q] = segs[next];
        line.push(if chain_key(&p) == last_key { q } else { p });
        if chain_key(&line[line.len() - 1]) == chain_key(from) {
            return; // closed
        }
    };
    for i in 0..segs.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut line: Track = vec![segs[i][0], segs[i][1]];
        let first = line[0];
        grow(&mut line, &first, &mut used);
        if chain_key(&line[line.len() - 1]) != chain_key(&line[0]) {
            // open: grow the other end too
            line.reverse();
            let first = line[0];
            grow(&mut line, &first, &mut used);
        }
        out.push(line);
    }
    out
}

/// Port of `resample` (web/prop/rim.js:158-171): stations every `step` along a polyline (its
/// own vertices' z, linearly).
fn resample(line: &[[f64; 3]], step: f64) -> Track {
    let mut out = vec![line[0]];
    let mut carry = 0.0f64;
    for i in 1..line.len() {
        let (p, q) = (line[i - 1], line[i]);
        let len = jsnum::hypot(&[q[0] - p[0], q[1] - p[1]]);
        let mut t = step - carry;
        while t <= len {
            let u = t / len;
            out.push([
                p[0] + (q[0] - p[0]) * u,
                p[1] + (q[1] - p[1]) * u,
                p[2] + (q[2] - p[2]) * u,
            ]);
            t += step;
        }
        carry = len - (t - step);
    }
    out
}

/// Port of `inset` (web/prop/rim.js:175-189): each station moved `PROP.edgeInset` in, toward the
/// region's underside, z its surface there; `None` where the surface isn't under it.
fn inset(line: &[[f64; 3]], region_tris: &Tris, cfg: &Config) -> Vec<Option<[f64; 3]>> {
    let e = cfg.prop.edge_inset;
    let len = line.len();
    (0..len)
        .map(|i| {
            let p = line[i];
            let a = line[jsnum::max(0.0, i as f64 - 1.0) as usize];
            let b = line[jsnum::min((len - 1) as f64, (i + 1) as f64) as usize];
            let mut tx = b[0] - a[0];
            let mut ty = b[1] - a[1];
            let tl = jsnum::hypot(&[tx, ty]);
            if tl < 1e-9 {
                return None;
            }
            tx /= tl;
            ty /= tl;
            let side = [1.0, -1.0].into_iter().find(|s| {
                surface_z_at(region_tris, p[0] - s * ty * 0.5, p[1] + s * tx * 0.5).is_some()
            })?;
            let x = p[0] - side * ty * e;
            let y = p[1] + side * tx * e;
            let z = surface_z_at(region_tris, x, y)?;
            Some([x, y, z])
        })
        .collect()
}

/// Port of `runs` (web/prop/rim.js:192-199): the runs of stations between nulls.
fn runs(line: &[Option<[f64; 3]>]) -> Vec<Track> {
    let mut out = Vec::new();
    let mut cur: Track = Vec::new();
    for p in line {
        if let Some(p) = p {
            cur.push(*p);
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Port of `pieces` (web/prop/rim.js:202-228): a run cut into pull-out pieces, RIM.pieceGap
/// apart.
fn pieces(run: &[[f64; 3]], step: f64, cfg: &Config) -> Vec<Track> {
    let mut out = Vec::new();
    let gap = piece_gap(step, cfg);
    let max_n = jsnum::max(
        cfg.prop.min_stations,
        jsnum::round(cfg.rim.piece_len / step),
    );
    let max_turn = (cfg.rim.piece_turn * std::f64::consts::PI) / 180.0;
    let pi = std::f64::consts::PI;
    let mut cur: Track = Vec::new();
    let mut turn = 0.0f64;
    let mut i = 0;
    while i < run.len() {
        let p = run[i];
        if cur.len() >= 2 {
            let (a, b) = (cur[cur.len() - 2], cur[cur.len() - 1]);
            let h = jsnum::atan2(p[1] - b[1], p[0] - b[0]);
            let h0 = jsnum::atan2(b[1] - a[1], b[0] - a[0]);
            let mut d = h - h0;
            while d > pi {
                d -= 2.0 * pi;
            }
            while d < -pi {
                d += 2.0 * pi;
            }
            turn += d;
        }
        if cur.len() as f64 >= max_n || jsnum::abs(turn) > max_turn {
            out.push(std::mem::take(&mut cur));
            turn = 0.0;
            i += gap - 1; // leave the gap
            i += 1;
            continue;
        }
        cur.push(p);
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: [f64; 3], b: [f64; 3]) -> [[f64; 3]; 2] {
        [a, b]
    }

    #[test]
    fn resample_steps_along_the_polyline_with_carry_and_interpolates_z() {
        let line = [[0.0, 0.0, 0.0], [2.5, 0.0, 5.0], [2.5, 2.0, 5.0]];
        let r = resample(&line, 1.0);
        let xy: Vec<[f64; 2]> = r.iter().map(|p| [p[0], p[1]]).collect();
        assert_eq!(
            xy,
            vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [2.5, 0.5], [2.5, 1.5]]
        );
        assert_eq!(r[1][2], 2.0); // z linearly along the first segment
        assert_eq!(resample(&[[1.0, 1.0, 1.0]], 1.0), vec![[1.0, 1.0, 1.0]]);
    }

    #[test]
    fn chains_join_segments_end_to_end_open_or_closed() {
        let (a, b, c, d) = (
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        );
        // given out of order and flipped: one open polyline of 4 points
        let open = chains(&[seg(b, c), seg(a, b), seg(d, c)]);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].len(), 4);
        let ends = [open[0][0], open[0][3]];
        assert!(ends.contains(&a) && ends.contains(&d));
        // a loop comes back closed
        let ring = chains(&[seg(a, b), seg(b, c), seg(c, d), seg(d, a)]);
        assert_eq!(ring.len(), 1);
        assert_eq!(ring[0].first(), ring[0].last());
        // two separate pieces stay two (1e-4 rounding joins points that close)
        let two = chains(&[seg(a, b), seg([10.0, 0.0, 1.0], [11.0, 0.0, 1.0])]);
        assert_eq!(two.len(), 2);
        let near = chains(&[seg(a, b), seg([1.00001, 0.0, 1.0], c)]);
        assert_eq!(near.len(), 1);
        assert!(chains(&[]).is_empty());
    }

    #[test]
    fn runs_split_at_the_nulls() {
        let p = Some([1.0, 0.0, 0.0]);
        assert_eq!(
            runs(&[p, p, None, None, p])
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert!(runs(&[None, None]).is_empty());
        assert!(runs(&[]).is_empty());
    }

    #[test]
    fn pieces_are_cut_by_length_and_by_turn_with_a_gap() {
        let cfg = Config::default(); // pieceLen 40, pieceTurn 90, pieceGap 1.5
        // a straight run of 100 stations, 1 mm apart: pieces of 40, a 2-station gap (round(1.5)) each
        let run: Vec<[f64; 3]> = (0..100).map(|i| [i as f64, 0.0, 5.0]).collect();
        let p = pieces(&run, 1.0, &cfg);
        assert_eq!(p.iter().map(Vec::len).collect::<Vec<_>>(), vec![40, 40, 16]);
        assert_eq!(p[1][0][0], 42.0); // the station that tripped the cut and one more are left out
        // a U turn: cut where the heading has turned more than 90 degrees
        let u: Vec<[f64; 3]> = (0..60)
            .map(|i| {
                let a = i as f64 * 0.1;
                [10.0 * a.sin(), 10.0 * (1.0 - a.cos()), 5.0]
            })
            .collect();
        let p = pieces(&u, 1.0, &cfg);
        assert!(p.len() >= 2);
        assert!(pieces(&[], 1.0, &cfg).is_empty());
    }

    #[test]
    fn clip_to_rim_drops_stations_within_a_wall_and_its_clearance() {
        let cfg = Config::default();
        let keep = cfg.prop.th + cfg.prop.side_clear;
        let rim = vec![vec![[5.0, 0.0, 2.0]; 3]];
        let row: Vec<[f64; 3]> = (0..11).map(|i| [i as f64, 0.0, 2.0]).collect();
        let out = clip_to_rim(std::slice::from_ref(&row), &rim, &cfg);
        // stations closer than `keep` to x = 5 go; the two ends are the runs left
        let xs: Vec<Vec<f64>> = out
            .iter()
            .map(|r| r.iter().map(|p| p[0]).collect())
            .collect();
        assert!(
            keep > 1.0 && keep < 2.0,
            "the default clearance is between 1 and 2 mm"
        );
        assert_eq!(
            xs,
            vec![vec![0.0, 1.0, 2.0, 3.0], vec![7.0, 8.0, 9.0, 10.0]]
        );
        // no rim: the rows come back untouched, short ones included
        let short = vec![vec![[0.0, 0.0, 1.0]]];
        assert_eq!(clip_to_rim(&short, &[], &cfg), short);
        // a rim far away keeps the row whole
        assert_eq!(
            clip_to_rim(
                std::slice::from_ref(&row),
                &[vec![[100.0, 100.0, 0.0]]],
                &cfg
            ),
            vec![row]
        );
        // runs under minStations are dropped
        let tiny = vec![vec![[4.0, 0.0, 2.0], [4.0, 5.0, 2.0]]];
        assert!(clip_to_rim(&tiny, &rim, &cfg).is_empty());
    }
}
