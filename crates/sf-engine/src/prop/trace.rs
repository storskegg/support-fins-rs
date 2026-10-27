//! Port of `web/prop/trace.js`. Read `docs/AGENTS.md` before changing anything here.

use indexmap::{IndexMap, IndexSet};

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::config::PropConfig;
use crate::prop::surface::seat;
use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `TRACE` (web/prop/trace.js:16). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceConfig {
    // a face grows the surface if its normal points down more than this
    // (cos 70deg: a steep strip between red islands, not a wall's side)
    pub grow: f64,
    // mm in plan from the red it grew from: two
    // islands a span apart join, and a line runs no further past the red
    // than a wall reaches
    pub bridge: f64,
    // mm: the slope's central-difference step: ripples under it don't steer
    pub h: f64,
}

impl Default for TraceConfig {
    fn default() -> Self {
        TraceConfig {
            grow: 0.34,
            bridge: PropConfig::default().max_unsupported_span / 2.0,
            h: 2.0,
        }
    }
}

/// The `NEIGHBOURS` WeakMap entry (web/prop/trace.js:26-37), cached per topology: face neighbours
/// as CSR from the topology's edge pairs.
#[derive(Debug)]
pub struct Neighbours {
    start: Vec<i32>,
    items: Vec<i32>,
}

/// Port of `neighbours` (web/prop/trace.js:27-37): built once per topology.
fn neighbours(topo: &Topology) -> &Neighbours {
    topo.cache.trace_neighbours.get_or_init(|| {
        let n_faces = topo.n_faces;
        let (adj_a, adj_b) = (&topo.adj_a, &topo.adj_b);
        let mut start = vec![0i32; n_faces + 1];
        for e in 0..adj_a.len() {
            start[adj_a[e] as usize + 1] += 1;
            start[adj_b[e] as usize + 1] += 1;
        }
        for f in 0..n_faces {
            start[f + 1] += start[f];
        }
        let mut fill: Vec<i32> = start[..n_faces].to_vec();
        let mut items = vec![0i32; start[n_faces] as usize];
        for e in 0..adj_a.len() {
            let (a, b) = (adj_a[e] as usize, adj_b[e] as usize);
            items[fill[a] as usize] = adj_b[e];
            fill[a] += 1;
            items[fill[b] as usize] = adj_a[e];
            fill[b] += 1;
        }
        Neighbours { start, items }
    })
}

/// One tracing group of `trace_groups`: `{ members, tris, pts, red }`.
#[derive(Debug)]
pub struct TraceGroup {
    /// The group's region indices, ascending.
    pub members: Vec<usize>,
    /// The group's faces, seated.
    pub tris: Tris,
    /// Vertices + centroids, `downhill_lines`'s input.
    pub pts: Vec<[f64; 3]>,
    /// The `[x, y, z]` centroids of every overhang face on the surface, slivers too: what its
    /// lines must cover.
    pub red: Vec<[f64; 3]>,
}

/// What `trace_groups` returns: `{ leaderOf, groups }`.
#[derive(Debug)]
pub struct TraceGroups {
    /// Region -> its group's lowest region index.
    pub leader_of: IndexMap<usize, usize>,
    /// Per leader, in the order the leaders were first met.
    pub groups: IndexMap<usize, TraceGroup>,
}

/// Port of `traceGroups` (web/prop/trace.js:48-125): tracing groups for the regions in `tried`
/// (JS: a `Set` of region indices; insertion order matters, so an `IndexSet`).
///
/// Each region is grown across the down-facing faces around it -- the slivers and the steep
/// strips between them -- up to `bridge` mm from the red it grew from, and regions whose growth
/// meets become one surface, traced once.
///
/// `excluded_support_faces` is JS `result.excludedSupportFaces`, which `fins/coverage.js` adds
/// to a *copy* of the `analyze` result (`Analysis` has no such field); `None` is the
/// `?.` miss.
pub fn trace_groups(
    topo: &Topology,
    result: &Analysis,
    excluded_support_faces: Option<&[u8]>,
    rot: &Rot3,
    off: &Xyz,
    tried: &IndexSet<usize>,
    cfg: &Config,
) -> TraceGroups {
    let (pos, n_faces, nrm) = (&topo.pos, topo.n_faces, &topo.nrm);
    let Neighbours { start, items } = neighbours(topo);
    let (grow, bridge) = (cfg.trace.grow, cfg.trace.bridge);
    let mut label = vec![-1i32; n_faces];
    let mut cx = vec![0.0f64; n_faces];
    let mut cy = vec![0.0f64; n_faces];
    let mut sx = vec![0.0f64; n_faces]; // the red it grew from
    let mut sy = vec![0.0f64; n_faces];
    let centre = |f: usize, cx: &mut Vec<f64>, cy: &mut Vec<f64>| {
        let (mut x, mut y) = (0.0f64, 0.0f64);
        for i in 0..3 {
            let v = seat(pos, f * 9 + i * 3, rot, off);
            x += v[0];
            y += v[1];
        }
        cx[f] = x / 3.0;
        cy[f] = y / 3.0;
    };
    let nz = |f: usize| rot[2] * nrm[f * 3] + rot[5] * nrm[f * 3 + 1] + rot[8] * nrm[f * 3 + 2];

    // union-find over region indices
    let mut parent: Vec<usize> = (0..result.regions.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    fn join(parent: &mut [usize], a: usize, b: usize) {
        let (a, b) = (find(parent, a), find(parent, b));
        if a != b {
            parent[a.max(b)] = a.min(b);
        }
    }

    // Grow every tried region at once, breadth first, so a face goes to the nearer
    // region and two growths meet between their islands
    let mut queue: Vec<usize> = Vec::new();
    for &ri in tried {
        for &f in &result.regions[ri].faces {
            label[f] = ri as i32;
            centre(f, &mut cx, &mut cy);
            sx[f] = cx[f];
            sy[f] = cy[f];
            queue.push(f);
        }
    }
    let mut grown: Vec<usize> = Vec::new();
    while !queue.is_empty() {
        let mut next: Vec<usize> = Vec::new();
        for &f in &queue {
            for k in start[f] as usize..start[f + 1] as usize {
                let g = items[k] as usize;
                if excluded_support_faces.is_some_and(|e| e[g] != 0) {
                    continue;
                }
                if label[g] >= 0 {
                    if label[g] != label[f] {
                        join(&mut parent, label[g] as usize, label[f] as usize);
                    }
                    continue;
                }
                if result.on_bed[g] != 0 || nz(g) > -grow {
                    continue;
                }
                centre(g, &mut cx, &mut cy);
                if jsnum::hypot(&[cx[g] - sx[f], cy[g] - sy[f]]) > bridge {
                    continue;
                }
                label[g] = label[f];
                sx[g] = sx[f];
                sy[g] = sy[f];
                next.push(g);
                grown.push(g);
            }
        }
        queue = next;
    }

    struct Pending {
        members: Vec<usize>,
        list: Vec<usize>,
    }
    let mut leader_of: IndexMap<usize, usize> = IndexMap::new();
    let mut faces: IndexMap<usize, Pending> = IndexMap::new();
    for &ri in tried {
        let l = find(&mut parent, ri);
        leader_of.insert(ri, l);
        faces
            .entry(l)
            .or_insert_with(|| Pending {
                members: vec![],
                list: vec![],
            })
            .members
            .push(ri);
    }
    for &ri in tried {
        let l = find(&mut parent, ri);
        faces
            .get_mut(&l)
            .unwrap()
            .list
            .extend_from_slice(&result.regions[ri].faces);
    }
    for &g in &grown {
        let l = find(&mut parent, label[g] as usize);
        faces.get_mut(&l).unwrap().list.push(g);
    }

    let mut groups: IndexMap<usize, TraceGroup> = IndexMap::new();
    for (
        l,
        Pending {
            mut members,
            mut list,
        },
    ) in faces
    {
        list.sort(); // face order, not growth order: the same surface either way
        let mut tris = vec![0.0f64; list.len() * 9];
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let mut red: Vec<[f64; 3]> = Vec::new();
        for (k, &f) in list.iter().enumerate() {
            let (mut gx, mut gy, mut gz) = (0.0f64, 0.0f64, 0.0f64);
            for i in 0..3 {
                let v = seat(pos, f * 9 + i * 3, rot, off);
                tris[k * 9 + i * 3] = v[0];
                tris[k * 9 + i * 3 + 1] = v[1];
                tris[k * 9 + i * 3 + 2] = v[2];
                pts.push([v[0], v[1], v[2]]);
                gx += v[0];
                gy += v[1];
                gz += v[2];
            }
            pts.push([gx / 3.0, gy / 3.0, gz / 3.0]);
            if result.over[f] != 0 {
                red.push([gx / 3.0, gy / 3.0, gz / 3.0]);
            }
        }
        members.sort();
        groups.insert(
            l,
            TraceGroup {
                members,
                tris: Tris::new(tris),
                pts,
                red,
            },
        );
    }
    TraceGroups { leader_of, groups }
}
