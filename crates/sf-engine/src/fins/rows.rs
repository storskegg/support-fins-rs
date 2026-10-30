//! Port of `web/fins/rows.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! ROWS -- long walls along the model's length, laid before the fill pass's patch lines
//! (fins/fill.js, opts.longWalls). Matthew 2026-10-15: a hand-drawn wall "fits along the model so
//! nicely"; the fill pass built a figure a dozen 3-5 mm stubs at five headings, because each line is
//! cut to the stretch under one red patch.
//!
//! A row is a line along `heading` (fins/field.js) at one offset across it and one height hint,
//! spanning all the bare red within R across and ROWS.zBand of that height. One wall the whole length
//! rarely builds on a figure (its own body sits under the line somewhere, without headroom), so each
//! row keeps its LONGEST buildable pieces: the line is bisected until each half builds (or is under
//! 2 x ROWS.minPiece), and neighbouring pieces are re-joined left to right while the join still
//! builds. Every build is Draw's (draw.js drawnWall), tines off while searching.
//!
//! Then greedily, the piece holding the most still-bare red goes in, if it keeps PROP.sideClear off
//! every support (flanges may join on the plate, #196), rebuilt with tines; until no piece holds
//! FILL.minGain. Measured (prototype/fill-rows): Isaac upright, must-hold 76 -> 89%; the fill pass
//! after it takes what rows can't reach (scattered patches: an android zombie's 90% fell to 76% on
//! rows alone).

use crate::guards::JsSort;
use std::cmp::Ordering;
use std::sync::Arc;

use crate::config::Config;
use crate::draw::{DrawOpts, DrawnOk, DrawnWall, drawn_wall};
use crate::fins::coverage::reach_of;
use crate::jsnum;
use crate::prop::ctx::BuildCtx;
use crate::tris::Tris;

type V3 = [f64; 3];

/// `ROWS` (web/fins/rows.js:25). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct RowsConfig {
    // mm between rows (across the heading); the greedy pick spaces them
    pub step: f64,
    /// JS key `zStep`.
    // mm between a row's height hints
    pub z_step: f64,
    /// JS key `zBand`.
    // mm: a row spans the red within this of its height
    pub z_band: f64,
    /// JS key `minPiece`.
    // mm: a failed piece is bisected while its halves are at least this
    pub min_piece: f64,
    /// JS key `minKeep`.
    // mm: a row piece shorter than this is left to the patch lines (at 6 a small
    // figure's short rows went: the guardian lion upright held 86 -> 54%)
    pub min_keep: f64,
    /// JS key `maxRows`.
    // rows searched, heaviest red first
    pub max_rows: f64,
    /// JS key `maxBuilds`.
    // drawnWall calls while searching (a count, not a clock)...
    pub max_builds: f64,
    /// JS key `fullFaces`.
    // ...at up to this many part faces; a denser part gets proportionally fewer
    pub full_faces: f64,
    /// JS key `minBuilds`.
    // (but never under this many)
    pub min_builds: f64,
}

impl Default for RowsConfig {
    fn default() -> Self {
        RowsConfig {
            step: 1.0,
            z_step: 3.0,
            z_band: 6.0,
            min_piece: 3.0,
            min_keep: 3.0,
            max_rows: 160.0,
            max_builds: 900.0,
            full_faces: 250000.0,
            min_builds: 150.0,
        }
    }
}

/// `rowWalls`' argument object: `{ pts, heading, R, tris, drawOpts, fits, keep, minGain, room,
/// nFaces }` (web/fins/rows.js:38-46). `pts` the bare red `[x, y, z, area]`; `fits(r)` -- r keeps
/// clear of every support, its flange joining others on the plate as the patch lines' second pass
/// allows (#196); `keep(r)` adds it (and its tops to the caller's reach set); `room()` -- the
/// caller's wall cap not yet hit. `n_faces` scales the search budget (JS default 0). Plate only
/// rides in `draw_opts` (drawnWall refuses a row over the part).
pub struct RowWallsArgs<'a> {
    pub pts: &'a [[f64; 4]],
    pub heading: f64,
    pub r: f64,
    pub tris: &'a Arc<Tris>,
    pub draw_opts: &'a DrawOpts<'a>,
    pub fits: &'a mut dyn FnMut(&DrawnOk) -> bool,
    pub keep: &'a mut dyn FnMut(&DrawnOk),
    pub min_gain: f64,
    pub room: &'a mut dyn FnMut() -> bool,
    pub n_faces: f64,
}

/// What `rowWalls` returns: `{ walls, builds }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowWalls {
    pub walls: usize,
    pub builds: usize,
}

/// A row: `{ v, h, w, t1, t2 }`.
struct Row {
    v: f64,
    h: f64,
    w: f64,
    t1: f64,
    t2: f64,
}

/// A buildable stretch of a row: `{ p, q, r }`, later with `a`, `b`, `serves`, `done`.
struct Piece {
    r: DrawnOk,
    a: V3,
    b: V3,
    serves: Vec<usize>,
    done: bool,
}

/// JS `x || y` on numbers.
fn or(x: f64, y: f64) -> f64 {
    if x != 0.0 && !x.is_nan() { x } else { y }
}

fn num_ord(d: f64) -> Ordering {
    if d < 0.0 {
        Ordering::Less
    } else if d > 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// Port of `rowWalls` (web/fins/rows.js:46-133): lays the rows. Returns `{ walls, builds }`.
pub fn row_walls(args: RowWallsArgs, ctx: &mut BuildCtx) -> RowWalls {
    let cfg: &Config = ctx.cfg;
    let rows_cfg = &cfg.rows;
    let RowWallsArgs {
        pts,
        heading,
        r: reach_r,
        tris,
        draw_opts,
        fits,
        keep,
        min_gain,
        room,
        n_faces,
    } = args;
    let ux = jsnum::cos(heading);
    let uy = jsnum::sin(heading);
    let acr = |p: &[f64; 4]| -uy * p[0] + ux * p[1];
    let alo = |p: &[f64; 4]| ux * p[0] + uy * p[1];
    if pts.is_empty() {
        return RowWalls::default();
    }
    let max_builds = jsnum::max(
        rows_cfg.min_builds,
        jsnum::round(
            rows_cfg.max_builds * jsnum::min(1.0, rows_cfg.full_faces / jsnum::max(1.0, n_faces)),
        ),
    );
    // the points by their offset across the heading, so a row reads only its own band
    let mut by_acr: Vec<(f64, usize)> = pts.iter().enumerate().map(|(i, p)| (acr(p), i)).collect();
    by_acr.js_sort_by(|a, b| num_ord(a.0 - b.0));
    let from = |v: f64| {
        let (mut lo, mut hi) = (0usize, by_acr.len());
        while lo < hi {
            let m = (lo + hi) >> 1;
            if by_acr[m].0 < v {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        lo
    };
    let (mut z_min, mut z_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in pts {
        z_min = jsnum::min(z_min, p[2]);
        z_max = jsnum::max(z_max, p[2]);
    }
    let v_min = by_acr[0].0;
    let v_max = by_acr[by_acr.len() - 1].0;
    // the rows, heaviest band of red first
    let mut rows: Vec<Row> = Vec::new();
    let mut v = v_min;
    while v <= v_max + 1e-9 {
        let i0 = from(v - reach_r);
        let i1 = from(v + reach_r + 1e-9);
        let mut h = z_min;
        while h <= z_max + 1e-9 {
            let (mut w, mut t1, mut t2) = (0.0, f64::INFINITY, f64::NEG_INFINITY);
            for item in &by_acr[i0..i1] {
                let p = &pts[item.1];
                if jsnum::abs(p[2] - h) > rows_cfg.z_band {
                    continue;
                }
                w += p[3];
                let t = alo(p);
                t1 = jsnum::min(t1, t);
                t2 = jsnum::max(t2, t);
            }
            if w > 0.0 && t2 - t1 >= rows_cfg.min_keep - 2.0 {
                rows.push(Row {
                    v,
                    h,
                    w,
                    t1: t1 - 1.0,
                    t2: t2 + 1.0,
                });
            }
            h += rows_cfg.z_step;
        }
        v += rows_cfg.step;
    }
    rows.js_sort_by(|a, b| num_ord(or(or(b.w - a.w, a.v - b.v), a.h - b.h)));
    let mut search = draw_opts.clone();
    search.tines = false;
    let mut builds = 0usize;
    let mut pieces: Vec<Piece> = Vec::new();
    let take = {
        let n = jsnum::trunc(rows_cfg.max_rows);
        if n < 0.0 {
            jsnum::max(0.0, rows.len() as f64 + n) as usize
        } else {
            jsnum::min(n, rows.len() as f64) as usize
        }
    };
    for row in &rows[..take] {
        let at = |t: f64| -> V3 { [ux * t - uy * row.v, uy * t + ux * row.v, row.h] };
        let mut build =
            |p: f64, q: f64, builds: &mut usize, ctx: &mut BuildCtx| -> Option<DrawnOk> {
                if *builds as f64 >= max_builds {
                    return None;
                }
                *builds += 1;
                match drawn_wall(at(p), at(q), tris, 0.0, &search, ctx) {
                    DrawnWall::Ok(r) => Some(r),
                    DrawnWall::Refused { .. } => None,
                }
            };
        struct Leaf {
            p: f64,
            q: f64,
            r: DrawnOk,
        }
        fn split(
            p: f64,
            q: f64,
            leaves: &mut Vec<Leaf>,
            builds: &mut usize,
            max_builds: f64,
            min_piece: f64,
            build: &mut dyn FnMut(f64, f64, &mut usize, &mut BuildCtx) -> Option<DrawnOk>,
            ctx: &mut BuildCtx,
        ) {
            if let Some(r) = build(p, q, builds, ctx) {
                leaves.push(Leaf { p, q, r });
                return;
            }
            if q - p < 2.0 * min_piece || *builds as f64 >= max_builds {
                return;
            }
            let m = (p + q) / 2.0;
            split(p, m, leaves, builds, max_builds, min_piece, build, ctx);
            split(m, q, leaves, builds, max_builds, min_piece, build, ctx);
        }
        let mut leaves: Vec<Leaf> = Vec::new();
        split(
            row.t1,
            row.t2,
            &mut leaves,
            &mut builds,
            max_builds,
            rows_cfg.min_piece,
            &mut build,
            ctx,
        );
        let mut cur: Option<Leaf> = None;
        for l in leaves {
            if let Some(c) = &cur
                && jsnum::abs(c.q - l.p) < 1e-9
                && let Some(r) = build(c.p, l.q, &mut builds, ctx)
            {
                cur = Some(Leaf { p: c.p, q: l.q, r });
                continue;
            }
            if let Some(c) = cur.take()
                && c.q - c.p >= rows_cfg.min_keep
            {
                pieces.push(Piece {
                    a: at(c.p),
                    b: at(c.q),
                    r: c.r,
                    serves: Vec::new(),
                    done: false,
                });
            }
            cur = Some(l);
        }
        if let Some(c) = cur.take()
            && c.q - c.p >= rows_cfg.min_keep
        {
            pieces.push(Piece {
                a: at(c.p),
                b: at(c.q),
                r: c.r,
                serves: Vec::new(),
                done: false,
            });
        }
        if builds as f64 >= max_builds {
            break;
        }
    }
    // each piece's red: the points its top reaches
    for c in &mut pieces {
        let reach = reach_of(&c.r.top, reach_r);
        c.serves = pts
            .iter()
            .enumerate()
            .filter(|(_, p)| reach.near(p[0], p[1], p[2]))
            .map(|(i, _)| i)
            .collect();
    }
    let mut held = vec![false; pts.len()];
    let mut walls = 0usize;
    while room() {
        let mut best: Option<usize> = None;
        let mut best_gain = min_gain;
        for (ci, c) in pieces.iter().enumerate() {
            if c.done {
                continue;
            }
            let mut g = 0.0;
            for &i in &c.serves {
                if !held[i] {
                    g += pts[i][3];
                }
            }
            if g >= best_gain && (best.is_none() || g > best_gain) {
                best = Some(ci);
                best_gain = g;
            }
        }
        let Some(bi) = best else { break };
        pieces[bi].done = true;
        if !fits(&pieces[bi].r) {
            continue;
        }
        // the wall that goes in carries its tines (the search built them off)
        let r = if draw_opts.tines {
            match drawn_wall(pieces[bi].a, pieces[bi].b, tris, 0.0, draw_opts, ctx) {
                DrawnWall::Ok(r) => Some(r),
                DrawnWall::Refused { .. } => None,
            }
        } else {
            Some(pieces[bi].r.clone())
        };
        builds += 1;
        let Some(r) = r else { continue };
        if !fits(&r) {
            continue;
        }
        keep(&r);
        walls += 1;
        let reach = reach_of(&r.top, reach_r);
        for (i, p) in pts.iter().enumerate() {
            if !held[i] && reach.near(p[0], p[1], p[2]) {
                held[i] = true;
            }
        }
    }
    RowWalls { walls, builds }
}
