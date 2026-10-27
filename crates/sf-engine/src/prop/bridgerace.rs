//! Port of `web/prop/bridgerace.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! A flat patch's walls turned across the slicer's bridge (prop/bridge.js) against the same
//! patch's walls the old way, along the longer free run.
//!
//! Why a race. Turning is right for the print -- walls parallel to the bridge lines hold one
//! line each (orient coupon: "junk") -- but a turned wall is shorter and its rows fit the face
//! differently, and on some parts it simply holds less: the first sweep lost >2 points of
//! coverage on 10 cases (ssdMounts flat cov0 100 -> 77 %). Quality first, so a turned patch
//! keeps its walls only while they hold about as much; otherwise the old walls stay (Matthew,
//! 2026-10-23: race per patch).
//!
//! buildProps drives it from its patch loop, like tubeRivalry:
//!
//! ```text
//! race.begin(&mut acc, &patches[i], ctx);          // top of every patch, before tubeRivalry's
//! ...lines = patchTracks(...)...                   //   mark: a settle cuts `out` back
//! race.turned(&acc, i, &lines, &mut patches, ctx); // queue the old-way twin
//! ...build the patch's walls...
//! race.finish(&mut acc, ctx);                      // after the loop
//! ```
//!
//! The twin runs right after its patch, so the two builds are the tail of `out` when they are
//! judged, and the loser can be cut off or the winner moved down.
//!
//! **Shape divergence (deliberate).** As in rival.rs, the accumulators are passed to each call,
//! and the twin is recognised by `Patch::bridge_twin` instead of by identity.

use std::collections::HashMap;
use std::sync::Arc;

use crate::jsnum;
use crate::prop::build::{Acc, Lines, Patch};
use crate::prop::ctx::BuildCtx;
use crate::prop::rival::{Mark, restore as restore_acc, snapshot as snapshot_acc};
use crate::tris::Tris;
use crate::types::V4;

/// `RACE_REL` (web/prop/bridgerace.js:35): the old walls win only by this much more held area:
/// this share of the turned walls' held area...
pub const RACE_REL: f64 = 0.02;
/// `RACE_ABS` (web/prop/bridgerace.js:36): ...and at least this many mm2 (a sliver is not worth
/// parallel walls).
pub const RACE_ABS: f64 = 2.0;

/// `snapshot()` here: the route's mark plus the tests' tine capture length
/// (`globalThis.__TINECAP?.length ?? null`).
#[derive(Clone, Debug)]
struct BMark {
    m: Mark,
    cap: Option<usize>,
}

/// `pending`: `{ patch, twin, start, mid }`.
struct Pending {
    region: usize,
    tris: Arc<Tris>,
    start: BMark,
    mid: Option<BMark>,
}

/// The object `bridgeRace` returns (`begin`, `turned`, `finish`).
#[derive(Default)]
pub struct BridgeRace {
    pending: Option<Pending>,
}

/// Port of `bridgeRace` (web/prop/bridgerace.js:38-103).
pub fn bridge_race() -> BridgeRace {
    BridgeRace::default()
}

fn snapshot(acc: &Acc, ctx: &BuildCtx) -> BMark {
    BMark {
        m: snapshot_acc(acc),
        cap: ctx.cap_len(),
    }
}

fn restore(acc: &mut Acc, m: &BMark, ctx: &BuildCtx) {
    restore_acc(acc, &m.m);
    if let Some(cap) = m.cap
        && let Some(c) = ctx.tine_cap.borrow_mut().as_mut()
    {
        c.truncate(cap);
    }
}

/// `move(r, d)`: a `[from, to]` shifted by `d`.
fn shift(r: [usize; 2], d: isize) -> [usize; 2] {
    [(r[0] as isize + d) as usize, (r[1] as isize + d) as usize]
}

impl BridgeRace {
    /// keep `[mid, end)` in place of `[start, end)`: restore to start, push the tail back
    fn keep_tail(
        &mut self,
        acc: &mut Acc,
        start: &BMark,
        mid: &BMark,
        region: usize,
        ctx: &BuildCtx,
    ) {
        let end = snapshot(acc, ctx);
        let tris: Vec<V4> = acc.out[mid.m.out..].to_vec();
        let qs = acc.props[mid.m.props..].to_vec();
        let caps = match mid.cap {
            None => Vec::new(),
            Some(c) => ctx.tine_cap.borrow().as_ref().unwrap()[c..].to_vec(),
        };
        restore(acc, start, ctx);
        let sh = acc.out.len() as isize - mid.m.out as isize;
        acc.out.extend_from_slice(&tris);
        let cap_shift = match start.cap {
            None => 0,
            Some(_) => {
                ctx.tine_cap.borrow().as_ref().unwrap().len() as isize - mid.cap.unwrap() as isize
            }
        };
        if let Some(c) = ctx.tine_cap.borrow_mut().as_mut() {
            c.extend(caps);
        }
        let had = !qs.is_empty();
        for mut q in qs {
            for r in q.tri_ranges.iter_mut() {
                *r = shift(*r, sh);
            }
            if let Some(w) = q.wall_range {
                q.wall_range = Some(shift(w, sh));
            }
            if let Some(c) = q.caps {
                q.caps = Some(shift(c, cap_shift));
            }
            acc.props.push(q);
        }
        acc.tally.tines += end.m.tines - mid.m.tines;
        acc.skipped.add_diff(&end.m.skipped, &mid.m.skipped);
        acc.tally.sag_risk = acc.tally.sag_risk || end.m.sag_risk;
        for r in &end.m.served {
            if !mid.m.served.contains(r) {
                acc.served.insert(*r);
            }
        }
        // the turned walls had served the patch's own region before mid, so the diff
        // above misses it: restored to start, the region read unserved (lowledge cov0)
        if had {
            acc.served.insert(region);
        }
    }

    fn settle(&mut self, acc: &mut Acc, ctx: &BuildCtx) {
        if self.pending.as_ref().is_none_or(|p| p.mid.is_none()) {
            return;
        }
        let Pending {
            region,
            tris,
            start,
            mid,
        } = self.pending.take().unwrap();
        let mid = mid.unwrap();
        let span = ctx.cfg.prop.max_unsupported_span;
        let turned = held_pieces(&tris.v, &acc.out, start.m.out, mid.m.out, span);
        let old = held_pieces(&tris.v, &acc.out, mid.m.out, acc.out.len(), span);
        if old > turned + jsnum::max(RACE_ABS, RACE_REL * turned) {
            self.keep_tail(acc, &start, &mid, region, ctx);
        } else {
            restore(acc, &mid, ctx);
        }
    }

    /// Top of every patch: settle a finished race, or mark where the twin starts. Before any
    /// other mark is taken -- a settle cuts `out` back, and a small tube's mark taken first undid
    /// to past the end (holes in `out`).
    pub fn begin(&mut self, acc: &mut Acc, patch: &Patch, ctx: &BuildCtx) {
        match self.pending.as_mut() {
            Some(p) if patch.bridge_twin => p.mid = Some(snapshot(acc, ctx)),
            Some(_) => self.settle(acc, ctx),
            None => {}
        }
    }

    /// The patch's tracks are laid: if they were turned (`lines.turned` = the old axis), queue
    /// its old-way twin next (after `patches[i]`). Nothing is built between begin and here, so
    /// the race starts here: only turned patches pay for a snapshot.
    pub fn turned(
        &mut self,
        acc: &Acc,
        i: usize,
        lines: &Lines,
        patches: &mut Vec<Patch>,
        ctx: &BuildCtx,
    ) {
        let patch = &patches[i];
        let Some(old_axis) = lines.turned else { return };
        if patch.bridge_twin
            || patch.rival.is_some()
            || patch.small_tube.is_some()
            || patch.axis.is_some()
        {
            return;
        }
        let mut twin = patch.clone();
        twin.axis = Some(old_axis);
        twin.bridge_twin = true;
        self.pending = Some(Pending {
            region: patch.region,
            tris: patch.tris.clone(),
            start: snapshot(acc, ctx),
            mid: None,
        });
        patches.insert(i + 1, twin); // for...of reaches it next
    }

    /// After the patch loop: settle the last race.
    pub fn finish(&mut self, acc: &mut Acc, ctx: &BuildCtx) {
        self.settle(acc, ctx);
    }
}

/// `SUB` (web/prop/bridgerace.js:108).
const SUB: f64 = 3.0;

/// A JS `Map` key for a number (SameValueZero): -0 is 0, every NaN is one key.
fn key_bits(k: f64) -> u64 {
    if k.is_nan() {
        f64::NAN.to_bits()
    } else if k == 0.0 {
        0
    } else {
        k.to_bits()
    }
}

/// Port of `heldPieces` (web/prop/bridgerace.js:109-142): region area held by support triangles
/// `tris[from..to)`, on the sweep's own ruler (prototype/sweep/sweep.js coverage): each face cut
/// into ~SUB mm pieces, a piece held when a support vertex sits within maxUnsupportedSpan in
/// plan and 0-3 mm below it. Judged by centroid, a flat ceiling of two big triangles read fully
/// held by rows 30 mm apart (lowledge cov0 tied 1760 = 1760 mm2 while the sweep said 100 ->
/// 82 %). `faces` = the region's seated triangles, flat xyz.
fn held_pieces(faces: &[f64], tris: &[V4], from: usize, to: usize, span: f64) -> f64 {
    let mut grid: HashMap<u64, Vec<&V4>> = HashMap::new();
    let key = |i: f64, j: f64| key_bits(i * 100003.0 + j);
    for v in &tris[from..to] {
        let k = key(jsnum::floor(v[0] / span), jsnum::floor(v[1] / span));
        grid.entry(k).or_default().push(v);
    }
    let held_at = |cx: f64, cy: f64, cz: f64| -> bool {
        let gi = jsnum::floor(cx / span);
        let gj = jsnum::floor(cy / span);
        for di in [-1.0, 0.0, 1.0] {
            for dj in [-1.0, 0.0, 1.0] {
                if let Some(vs) = grid.get(&key(gi + di, gj + dj)) {
                    for v in vs {
                        if v[2] > cz - 3.0
                            && v[2] < cz + 0.5
                            && jsnum::hypot(&[v[0] - cx, v[1] - cy]) <= span
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    };
    let mut held = 0.0f64;
    let mut t = 0;
    while t + 8 < faces.len() {
        let (ax, ay, az) = (faces[t], faces[t + 1], faces[t + 2]);
        let (ux, uy, uz) = (faces[t + 3] - ax, faces[t + 4] - ay, faces[t + 5] - az);
        let (vx, vy, vz) = (faces[t + 6] - ax, faces[t + 7] - ay, faces[t + 8] - az);
        t += 9;
        let area = jsnum::hypot(&[uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx]) / 2.0;
        if area < 1e-9 {
            continue;
        }
        let nf = jsnum::max(
            1.0,
            jsnum::ceil(
                jsnum::max(
                    jsnum::max(jsnum::hypot(&[ux, uy, uz]), jsnum::hypot(&[vx, vy, vz])),
                    jsnum::hypot(&[vx - ux, vy - uy, vz - uz]),
                ) / SUB,
            ),
        );
        let piece = area / (nf * nf);
        // `i < n` is false for a NaN n: nothing is counted
        let mut i = 0.0f64;
        while i < nf {
            let mut j = 0.0f64;
            while j < nf - i {
                // upright piece at (i, j), plus the inverted one beside it
                let pts: &[[f64; 2]] = if j < nf - i - 1.0 {
                    &[
                        [i + 1.0 / 3.0, j + 1.0 / 3.0],
                        [i + 2.0 / 3.0, j + 2.0 / 3.0],
                    ]
                } else {
                    &[[i + 1.0 / 3.0, j + 1.0 / 3.0]]
                };
                for &[s1, t1] in pts {
                    if held_at(
                        ax + (ux * s1 + vx * t1) / nf,
                        ay + (uy * s1 + vy * t1) / nf,
                        az + (uz * s1 + vz * t1) / nf,
                    ) {
                        held += piece;
                    }
                }
                j += 1.0;
            }
            i += 1.0;
        }
    }
    held
}
