//! Port of `web/fins/shortwalls.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Short walls, the LAST RESORT (issue #121). A ring-and-strut lattice is all 4-6 mm strut
//! undersides, every one under PROP.minSpan, so the walls dropped them as stubs and half the red
//! printed into air. Once the walls and wedges are in, the regions still bare get one more
//! placement pass with the short-wall floor (prop/clearance.js minSpanFor `short`): a wall down to
//! minSpanShort, kept only while it is at most maxShortAspect times as tall as it is long.
//!
//! Why last and not in the normal pass: a short wall that "serves" a face stands a wedge or a
//! raster row down, and those hold it better (a tube lost 14 wedge tines that way,
//! tests/tails.test.js). Here it only lands where nothing else did, so a part the engine already
//! served is untouched.
//!
//! A wall that comes within sideClear of a wall or wedge already built is dropped (bounding boxes,
//! conservative), and so is one that comes within sideClear of a short wall kept before it.

use indexmap::IndexSet;

use crate::fins::wedges::bare_after_wedges;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::build::{BuildOpts, PropRecord, build_short_walls};
use crate::prop::ctx::BuildCtx;
use crate::types::{Rot3, V4};

/// `[minX, minY, minZ, maxX, maxY, maxZ]`.
type Box6 = [f64; 6];

/// Port of `boxOf` (web/fins/shortwalls.js:22-29): the box over `tris[from..to)`, one vertex per
/// entry.
fn box_of(tris: &[V4], from: usize, to: usize) -> Box6 {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for v in &tris[from..to] {
        for k in 0..3 {
            if v[k] < b[k] {
                b[k] = v[k];
            }
            if v[k] > b[k + 3] {
                b[k + 3] = v[k];
            }
        }
    }
    b
}

/// `overlaps` (web/fins/shortwalls.js:30-32).
fn overlaps(a: &Box6, b: &Box6, pad: f64) -> bool {
    a[0] - pad <= b[3]
        && b[0] <= a[3] + pad
        && a[1] - pad <= b[4]
        && b[1] <= a[4] + pad
        && a[2] - pad <= b[5]
        && b[2] <= a[5] + pad
}

/// What `lastResortWalls` returns: `{ triangles, props, served, tines }`. Every record in `props`
/// carries `short: true` in JS (`FinRecord::short` once `buildFins` turns it into a fin), with its
/// `tri_ranges` into `triangles`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShortWalls {
    pub triangles: Vec<V4>,
    pub props: Vec<PropRecord>,
    pub served: Vec<usize>,
    pub tines: usize,
}

/// Port of `lastResortWalls` (web/fins/shortwalls.js:40-69): short walls for the regions
/// `served_regions` and `wedge_tris` left bare. `existing` is every support triangle already built
/// (vertex list). Returns the kept walls: triangles, props with triRanges into those triangles,
/// the regions they serve and their tines.
pub fn last_resort_walls(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    opts: &BuildOpts,
    served_regions: &[usize],
    wedge_tris: &[V4],
    existing: &[V4],
    ctx: &BuildCtx,
) -> ShortWalls {
    let cfg = ctx.cfg;
    cfg.cancel.check();
    let served_set: IndexSet<usize> = served_regions.iter().copied().collect();
    let bare = bare_after_wedges(topo, rot, result, &served_set, wedge_tris, cfg);
    if bare.is_empty() {
        return ShortWalls::default();
    }
    let regions: IndexSet<usize> = bare.into_iter().collect();
    let built = build_short_walls(topo, result, rot, opts, &regions, ctx);
    if built.props.is_empty() {
        return ShortWalls::default();
    }

    let mut others: Vec<Box6> = Vec::new();
    let mut i = 0;
    while i < existing.len() {
        others.push(box_of(existing, i, i + 3));
        i += 3;
    }
    let mut triangles: Vec<V4> = Vec::new();
    let mut props: Vec<PropRecord> = Vec::new();
    let mut served: IndexSet<usize> = IndexSet::new();
    let mut tines = 0usize;
    for p in &built.props {
        cfg.cancel.check();
        let ranges = &p.tri_ranges;
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for [a, z] in ranges {
            let r = box_of(&built.triangles, *a, *z);
            for k in 0..3 {
                b[k] = jsnum::min(b[k], r[k]);
            }
            for k in 3..6 {
                b[k] = jsnum::max(b[k], r[k]);
            }
        }
        if others.iter().any(|o| overlaps(&b, o, cfg.prop.side_clear)) {
            continue;
        }
        let tri_ranges: Vec<[usize; 2]> = ranges
            .iter()
            .map(|[a, z]| {
                let at = triangles.len();
                triangles.extend_from_slice(&built.triangles[*a..*z]);
                [at, triangles.len()]
            })
            .collect();
        others.push(b); // and the next short wall keeps off this one
        props.push(PropRecord {
            tri_ranges,
            ..p.clone()
        });
        served.extend(p.region);
        tines += p.tines;
    }
    ShortWalls {
        triangles,
        props,
        served: served.into_iter().collect(),
        tines,
    }
}
