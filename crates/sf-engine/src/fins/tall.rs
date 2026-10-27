//! Port of `web/fins/tall.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! TALL-WALL RIBS (local issue 046; a user: "you're definitely going to want some sort of bracing
//! for fins taller than 10cm or they wobble too much, maybe just a perpendicular fin"). A wall is a
//! 1 mm sheet whose foot stops at footMax, tied to the part only by its tines at the top: tall, it
//! sways across its thin side every layer.
//!
//! Runs LAST, after every wall, wedge, fill wall and sway brace is on the plate (ribs built as
//! walls went in took plate the next walls needed -- the fill pass's lesson). Each wall standing on
//! the plate over BRACE.tallH gets face ribs along it (prop/brace.js, `kinds.tall`): only at
//! stations whose floor is the plate, each kept sideClear off the part and off every other support
//! (its own wall and tines aside), so no rib fuses two pieces. A wall the fill pass already braced
//! is left as it is. `opts.tallRibs: false` (the Brace tall walls box unticked) skips it.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::config::Config;
use crate::fins::build::{FinKind, FinsBuilt};
use crate::fins::seating::seated_part_tris;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::brace::{BraceKinds, brace_wall};
use crate::prop::surface::surface_zs_at;
use crate::tris::Tris;
use crate::types::{Rot3, V4};

type V3 = [f64; 3];

/// `CELL` (web/fins/tall.js:19): the clash grid's plan cell, mm.
const CELL: f64 = 2.0;
/// `SPLIT` (web/fins/tall.js:20): triangles are split to this across in plan before they're compared, mm.
const SPLIT: f64 = 1.0;

/// Port of `pieces` (web/fins/tall.js:25-39): `fn(x0, y0, x1, y1, zLo, zHi)` for pieces of triangle
/// (p, q, r) about SPLIT across in plan: a diagonal wall's triangle would box a whole quadrant, and a
/// rib's would count its tall stem out at its low tip. Split at the longest edge in plan.
fn pieces(
    mut p: V3,
    mut q: V3,
    mut r: V3,
    f: &mut dyn FnMut(f64, f64, f64, f64, f64, f64),
    depth: usize,
) {
    let x0 = jsnum::min(jsnum::min(p[0], q[0]), r[0]);
    let x1 = jsnum::max(jsnum::max(p[0], q[0]), r[0]);
    let y0 = jsnum::min(jsnum::min(p[1], q[1]), r[1]);
    let y1 = jsnum::max(jsnum::max(p[1], q[1]), r[1]);
    if depth >= 16 || jsnum::max(x1 - x0, y1 - y0) <= SPLIT {
        f(
            x0,
            y0,
            x1,
            y1,
            jsnum::min(jsnum::min(p[2], q[2]), r[2]),
            jsnum::max(jsnum::max(p[2], q[2]), r[2]),
        );
        return;
    }
    let d = |a: V3, b: V3| jsnum::pow(a[0] - b[0], 2.0) + jsnum::pow(a[1] - b[1], 2.0);
    let pq = d(p, q);
    let qr = d(q, r);
    let rp = d(r, p);
    if qr >= pq && qr >= rp {
        (p, q, r) = (q, r, p);
    } else if rp >= pq && rp >= qr {
        (p, q, r) = (r, p, q);
    }
    let m = [
        (p[0] + q[0]) / 2.0,
        (p[1] + q[1]) / 2.0,
        (p[2] + q[2]) / 2.0,
    ];
    pieces(p, m, r, f, depth + 1);
    pieces(m, q, r, f, depth + 1);
}

fn xyz(v: &V4) -> V3 {
    [v[0], v[1], v[2]]
}

/// One ~1 mm piece of a support triangle: its plan box, its LOWEST point and the support it
/// belongs to (`o = [x0, y0, x1, y1, z, tag]`).
#[derive(Clone, Copy, Debug)]
struct Piece {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    z: f64,
    /// `tag`: the fin index, or `None` for JS `null` (the pad).
    tag: Option<usize>,
}

/// Port of `planGrid` (web/fins/tall.js:49-86): where supports stand, in plan: ~1 mm pieces of each
/// support triangle, each with its plan box and its LOWEST point. A rib rises from the plate, and so
/// does every wall, wedge and pad it could meet, so "a piece lower than the rib's top there, within
/// sideClear of it in plan" is a clash; tines high on a wall are not. Coarser than fill.js's 3D
/// boxGrid in height only, on the safe side (a rib is refused under a leaning wedge it might have
/// cleared); that grid cost 4 s on bigplate Y35.
///
/// The JS `Map` is only ever looked up, never iterated, so a `HashMap` is exact.
#[derive(Debug, Default)]
pub struct PlanGrid {
    cells: HashMap<i64, Vec<Piece>>,
}

/// `key(i, j)`: `(i + 32768) * 65536 + (j + 32768)`.
fn key(i: f64, j: f64) -> i64 {
    ((i + 32768.0) * 65536.0 + (j + 32768.0)) as i64
}

/// `cellsOf(x0, y0, x1, y1, fn)`: every grid cell the plan box touches.
fn cells_of(x0: f64, y0: f64, x1: f64, y1: f64, f: &mut dyn FnMut(i64)) {
    let mut i = jsnum::floor(x0 / CELL);
    while i <= jsnum::floor(x1 / CELL) {
        let mut j = jsnum::floor(y0 / CELL);
        while j <= jsnum::floor(y1 / CELL) {
            f(key(i, j));
            j += 1.0;
        }
        i += 1.0;
    }
}

impl PlanGrid {
    pub fn new() -> PlanGrid {
        PlanGrid::default()
    }

    /// `add(vs, tag = null, keep = () => true)`: the triangles `vs` (three vertices each) that
    /// `keep` accepts. `tag` is JS's `null` as `None`.
    pub fn add(&mut self, vs: &[V4], tag: Option<usize>, keep: impl Fn(&V4, &V4, &V4) -> bool) {
        let mut n = 0;
        while n + 2 < vs.len() {
            let (a, b, c) = (&vs[n], &vs[n + 1], &vs[n + 2]);
            n += 3;
            if !keep(a, b, c) {
                continue;
            }
            let cells = &mut self.cells;
            pieces(
                xyz(a),
                xyz(b),
                xyz(c),
                &mut |x0, y0, x1, y1, z, _z_hi| {
                    let o = Piece {
                        x0,
                        y0,
                        x1,
                        y1,
                        z,
                        tag,
                    };
                    cells_of(x0, y0, x1, y1, &mut |k| cells.entry(k).or_default().push(o));
                },
                0,
            );
        }
    }

    /// `hits(vs, own = undefined)`: does another support (not `own`) stand within sideClear of the
    /// rib `vs`? `own` is `None` for JS `undefined` (every piece then counts, including the pad's
    /// `null` tag: `null !== undefined`).
    pub fn hits(&self, vs: &[V4], own: Option<usize>, cfg: &Config) -> bool {
        let pad = cfg.prop.side_clear;
        let mut hit = false;
        let mut n = 0;
        while n + 2 < vs.len() && !hit {
            let (a, b, c) = (xyz(&vs[n]), xyz(&vs[n + 1]), xyz(&vs[n + 2]));
            n += 3;
            pieces(
                a,
                b,
                c,
                &mut |mut x0, mut y0, mut x1, mut y1, _z_lo, z_hi| {
                    if hit {
                        return;
                    }
                    x0 -= pad;
                    y0 -= pad;
                    x1 += pad;
                    y1 += pad;
                    cells_of(x0, y0, x1, y1, &mut |k| {
                        if hit {
                            return;
                        }
                        for o in self.cells.get(&k).map(Vec::as_slice).unwrap_or(&[]) {
                            // `o[5] !== own`: equal only when both are the same fin index
                            let differs = !matches!((o.tag, own), (Some(a), Some(b)) if a == b);
                            if differs
                                && o.z <= z_hi + pad
                                && o.x0 <= x1
                                && x0 <= o.x1
                                && o.y0 <= y1
                                && y0 <= o.y1
                            {
                                hit = true;
                                return;
                            }
                        }
                    });
                },
                0,
            );
        }
        hit
    }
}

/// Port of `floorAt` (web/fins/tall.js:90-94): the highest part surface under station p (a wall's
/// centreline point, z the wall's top), or zBed: where that station's wall stands.
fn floor_at(tris: &Tris, p: V3, z_bed: f64) -> f64 {
    let mut f = z_bed;
    for z in surface_zs_at(tris, p[0], p[1]) {
        if z < p[2] - 0.5 && z > f {
            f = z;
        }
    }
    f
}

/// Port of `braceTallWalls` (web/fins/tall.js:100-136): add the ribs to `built` (buildFins' result,
/// seated frame, bed at z 0) in place: triangles appended, each braced fin's `tri_ranges` grown,
/// `built.tall_ribs` the count.
///
/// JS also writes `f.ribs = n` on each braced fin; `FinRecord` has no such field, so the pairs
/// `(fin index, ribs)` come back as the return value.
pub fn brace_tall_walls(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    built: &mut FinsBuilt,
    cfg: &Config,
) -> Vec<(usize, usize)> {
    let (brace, prop) = (&cfg.brace, &cfg.prop);
    let tall: Vec<usize> = built
        .fins
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            f.kind == FinKind::Prop
                && f.braces.is_none_or(|b| b == 0)
                && f.height > brace.tall_h
                && f.line.len() >= 2
        })
        .map(|(i, _)| i)
        .collect();
    built.tall_ribs = Some(0);
    let mut ribbed = Vec::new();
    if tall.is_empty() {
        return ribbed;
    }
    let tris = seated_part_tris(topo, rot, &result.offset);
    // Only what a rib could meet goes in: under the tallest rib (a tall wall's tines, and
    // everything above, never can)
    let heights: Vec<f64> = tall.iter().map(|&i| built.fins[i].height).collect();
    let z_top = brace.rise * jsnum::max_of(&heights) + prop.side_clear;
    let low = move |p: &V4, q: &V4, r: &V4| jsnum::min(jsnum::min(p[2], q[2]), r[2]) <= z_top;
    let mut grid = PlanGrid::new();
    for (k, f) in built.fins.iter().enumerate() {
        for [a, b] in &f.tri_ranges {
            grid.add(&built.triangles[*a..*b], Some(k), low);
        }
    }
    if !built.pad_triangles.is_empty() {
        grid.add(&built.pad_triangles, None, low);
    }
    let mut triangles = built.triangles.clone();
    let grid = RefCell::new(grid);
    let mut total = 0usize;
    for k in tall {
        let f = &built.fins[k];
        // braceWall takes the part's surface: Auto's lines sit the gap under it, the fill
        // pass's (drawnWall's `top`) are the surface itself
        let top: Vec<V3> = if f.fill {
            f.line.clone()
        } else {
            f.line
                .iter()
                .map(|p| [p[0], p[1], p[2] + prop.gap])
                .collect()
        };
        let floors: Vec<f64> = f.line.iter().map(|&p| floor_at(&tris, p, 0.0)).collect();
        let mut out: Vec<V4> = Vec::new();
        let n = brace_wall(
            &top,
            |i| floors[i],
            &tris,
            0.0,
            &mut out,
            |t| {
                if grid.borrow().hits(t, Some(k), cfg) {
                    return false;
                }
                grid.borrow_mut().add(t, Some(k), |_, _, _| true); // the next wall's ribs keep clear of these
                true
            },
            BraceKinds {
                slim: false,
                tall: true,
            },
            cfg,
        );
        if n == 0 {
            continue;
        }
        let at = triangles.len();
        triangles.extend(out);
        let f = &mut built.fins[k];
        f.tri_ranges.push([at, triangles.len()]);
        ribbed.push((k, n));
        total += n;
    }
    built.tall_ribs = Some(total);
    built.triangles = triangles;
    ribbed
}
