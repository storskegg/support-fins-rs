//! Port of `web/pieces.js`. Read `docs/AGENTS.md` before changing anything here.

use crate::cancel::Cancel;
use std::collections::HashMap;

use indexmap::IndexMap;

use crate::jsnum;
use crate::overhangs::{Analysis, BED_EPS, Topology};
use crate::types::Rot3;

/// One floating piece: `{ faces, lowest, drop }` (web/pieces.js:122).
#[derive(Clone, Debug, PartialEq)]
pub struct FloatingPiece {
    pub faces: usize,
    pub lowest: [f64; 3],
    pub drop: f64,
}

/// A piece while it is being gathered: JS `{ faces, lowest, vol }`.
struct Piece {
    faces: usize,
    lowest: Option<[f64; 3]>,
    vol: f64,
}

/// `find(x)` with path halving: `x = parent[x] = parent[parent[x]]`.
fn find(parent: &mut [i32], mut x: i32) -> i32 {
    while parent[x as usize] != x {
        let g = parent[parent[x as usize] as usize];
        parent[x as usize] = g;
        x = g;
    }
    x
}

/// Pieces of the part that start in mid-air: closed shells that neither touch the plate nor sit
/// on another piece, in the seated frame of `result`; `drop` is how far the lowest point hangs
/// over whatever is below it (the bare plate: its height).
///
/// `floatingPieces(topo, result, rot = IDENTITY3)` (web/pieces.js:29-125); callers pass
/// `IDENTITY3` for the default.
pub fn floating_pieces(topo: &Topology, result: &Analysis, rot: &Rot3) -> Vec<FloatingPiece> {
    floating_pieces_cancellable(topo, result, rot, &Cancel::default())
}

/// [`floating_pieces`] that polls `cancel`.
pub fn floating_pieces_cancellable(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    cancel: &Cancel,
) -> Vec<FloatingPiece> {
    let pos = &topo.pos;
    let n_faces = topo.n_faces;
    let (adj_a, adj_b) = (&topo.adj_a, &topo.adj_b);
    const RESTS_ON: f64 = 0.3;
    let off = &result.offset;
    let mut parent: Vec<i32> = (0..n_faces as i32).collect();
    for e in 0..adj_a.len() {
        if e & 0xfff == 0 {
            cancel.check();
        }
        let ra = find(&mut parent, adj_a[e]);
        let rb = find(&mut parent, adj_b[e]);
        if ra != rb {
            parent[ra as usize] = rb;
        }
    }
    // Seat every vertex once: the lookup below reads each triangle many times.
    let mut pp = vec![0.0f64; n_faces * 9];
    let mut p = 0;
    while p < n_faces * 9 {
        if p & 0x3fff == 0 {
            cancel.check();
        }
        pp[p] = rot[0] * pos[p] + rot[3] * pos[p + 1] + rot[6] * pos[p + 2] + off.x;
        pp[p + 1] = rot[1] * pos[p] + rot[4] * pos[p + 1] + rot[7] * pos[p + 2] + off.y;
        pp[p + 2] = rot[2] * pos[p] + rot[5] * pos[p + 1] + rot[8] * pos[p + 2] + off.z;
        p += 3;
    }
    let mut root = vec![0i32; n_faces];
    let mut pieces: IndexMap<i32, Piece> = IndexMap::new();
    for f in 0..n_faces {
        if f & 0xfff == 0 {
            cancel.check();
        }
        let r = find(&mut parent, f as i32);
        root[f] = r;
        let g = pieces.entry(r).or_insert(Piece {
            faces: 0,
            lowest: None,
            vol: 0.0,
        });
        g.faces += 1;
        let t = f * 9;
        let mut i = 0;
        while i < 9 {
            if g.lowest.is_none_or(|l| pp[t + i + 2] < l[2]) {
                g.lowest = Some([pp[t + i], pp[t + i + 1], pp[t + i + 2]]);
            }
            i += 3;
        }
        g.vol += pp[t] * (pp[t + 4] * pp[t + 8] - pp[t + 5] * pp[t + 7])
            - pp[t + 1] * (pp[t + 3] * pp[t + 8] - pp[t + 5] * pp[t + 6])
            + pp[t + 2] * (pp[t + 3] * pp[t + 7] - pp[t + 4] * pp[t + 6]);
    }
    if pieces.len() < 2 {
        return Vec::new();
    }

    // Only an OUTWARD shell is a piece. An inward one (negative volume) is the wall of a sealed
    // cavity inside another piece, and its lowest point is that void's floor.
    let mut lifted: Vec<(i32, [f64; 3], usize)> = Vec::new();
    for (r, g) in &pieces {
        let lowest = g.lowest.expect("a piece has a face");
        if g.vol > 0.0 && lowest[2] >= BED_EPS {
            lifted.push((*r, lowest, g.faces));
        }
    }
    if lifted.is_empty() {
        return Vec::new();
    }

    // Triangles bucketed by plan footprint, so each lifted piece tests only the few under its
    // lowest point instead of every triangle in the part.
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    let mut p = 0;
    while p < pp.len() {
        if pp[p] < x0 {
            x0 = pp[p];
        }
        if pp[p] > x1 {
            x1 = pp[p];
        }
        if pp[p + 1] < y0 {
            y0 = pp[p + 1];
        }
        if pp[p + 1] > y1 {
            y1 = pp[p + 1];
        }
        p += 3;
    }
    let gsz = jsnum::max(
        1.0,
        jsnum::min(512.0, jsnum::round(jsnum::sqrt(n_faces as f64 / 4.0))),
    ) as usize;
    let cw = jsnum::max(1e-6, (x1 - x0) / gsz as f64);
    let ch = jsnum::max(1e-6, (y1 - y0) / gsz as f64);
    let cx = |x: f64| -> usize {
        jsnum::min(
            (gsz - 1) as f64,
            jsnum::max(0.0, jsnum::floor((x - x0) / cw)),
        ) as usize
    };
    let cy = |y: f64| -> usize {
        jsnum::min(
            (gsz - 1) as f64,
            jsnum::max(0.0, jsnum::floor((y - y0) / ch)),
        ) as usize
    };
    let mut cells: HashMap<usize, Vec<usize>> = HashMap::new();
    for f in 0..n_faces {
        let t = f * 9;
        let i0 = cx(jsnum::min_of(&[pp[t], pp[t + 3], pp[t + 6]]));
        let i1 = cx(jsnum::max_of(&[pp[t], pp[t + 3], pp[t + 6]]));
        let j0 = cy(jsnum::min_of(&[pp[t + 1], pp[t + 4], pp[t + 7]]));
        let j1 = cy(jsnum::max_of(&[pp[t + 1], pp[t + 4], pp[t + 7]]));
        for i in i0..=i1 {
            for j in j0..=j1 {
                cells.entry(i * gsz + j).or_default().push(f);
            }
        }
    }

    let mut out: Vec<FloatingPiece> = Vec::new();
    for (r, lowest, faces) in lifted {
        let (px, py, pz) = (lowest[0], lowest[1], lowest[2]);
        // highest surface of ANOTHER piece straight below the lowest point
        let mut below = f64::NEG_INFINITY;
        if let Some(cell) = cells.get(&(cx(px) * gsz + cy(py))) {
            for &f in cell {
                if root[f] == r {
                    continue;
                }
                let t = f * 9;
                let (ax, ay, bx, by, qx, qy) =
                    (pp[t], pp[t + 1], pp[t + 3], pp[t + 4], pp[t + 6], pp[t + 7]);
                let d = (by - qy) * (ax - qx) + (qx - bx) * (ay - qy);
                if jsnum::abs(d) < 1e-12 {
                    continue;
                }
                let l1 = ((by - qy) * (px - qx) + (qx - bx) * (py - qy)) / d;
                let l2 = ((qy - ay) * (px - qx) + (ax - qx) * (py - qy)) / d;
                let l3 = 1.0 - l1 - l2;
                if l1 < -1e-9 || l2 < -1e-9 || l3 < -1e-9 {
                    continue;
                }
                let z = l1 * pp[t + 2] + l2 * pp[t + 5] + l3 * pp[t + 8];
                if z <= pz + 1e-6 && z > below {
                    below = z;
                }
            }
        }
        let drop = if below == f64::NEG_INFINITY {
            pz
        } else {
            pz - below
        };
        if drop > RESTS_ON {
            out.push(FloatingPiece {
                faces,
                lowest,
                drop,
            });
        }
    }
    out
}
