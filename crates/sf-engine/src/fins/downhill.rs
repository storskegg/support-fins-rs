//! Port of `web/fins/downhill.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Downhill walls, judged on the whole build (issue #25). buildProps races downhill walls region by
//! region (prop/downhill.js, prop/raster.js raceAgainst), but what comes after the walls -- wedges,
//! the last-resort short walls, the fill pass -- builds around whatever won, and the race can't see
//! it. Two sweep cases traded: bore_bracket x30y60 served a region its short wall used to grip
//! 1.15 mm off the plate, and the part's lowest grip rose to 2.64 mm; torus x45 lost a squat wall's
//! share (98.3 -> 97.2%). So when any region swaps, the build without downhill walls is built too,
//! and the downhill one is kept only if it holds at least as much overhang, grips no higher and
//! keeps every wedge: more supported, never less.

use crate::fins::build::{FinKind, FinsBuilt};
use crate::fins::coverage::{HOLE, PLATE_Z, classify, reach_of};
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::prop::ctx::{BuildCtx, TineSeed};
use crate::prop::raster::grip_rise;
use crate::types::Rot3;

/// Port of `heldArea` (web/fins/downhill.js:21-39): overhang area (mm2) a build's wall tops reach
/// (coverage.js's rule): MUST and the near-plate strip (LOW) -- a squat wall's ledge counts (torus
/// X45, tests/tails.test.js). A tiny hole's ceiling is exempt, as on the scoreboard.
///
/// `cls` is JS's default parameter `classify(topo, result, rot)`, passed explicitly (for a result
/// that carries its coverage classes, those: `coverage::classify_targets`).
pub fn held_area(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    built: &FinsBuilt,
    cls: &[u8],
    cfg: &crate::config::Config,
) -> f64 {
    let mut tops: Vec<[f64; 3]> = Vec::new();
    for w in &built.fins {
        for p in &w.line {
            tops.push(*p);
        }
    }
    let near = reach_of(&tops, cfg.prop.max_unsupported_span / 2.0);
    let (pos, area) = (&topo.pos, &topo.area);
    let off = &result.offset;
    let off = [off.x, off.y, off.z];
    let mut held = 0.0;
    for f in 0..topo.n_faces {
        if result.over[f] == 0 || cls[f] == HOLE {
            continue;
        }
        let mut c = [0.0; 3];
        for i in 0..3 {
            let o = f * 9 + i * 3;
            for k in 0..3 {
                c[k] +=
                    (rot[k] * pos[o] + rot[3 + k] * pos[o + 1] + rot[6 + k] * pos[o + 2] + off[k])
                        / 3.0;
            }
        }
        if c[2] < PLATE_Z || near.near(c[0], c[1], c[2]) {
            held += area[f];
        }
    }
    held
}

/// Port of `usedDownhill` (web/fins/downhill.js:42): did any region swap to downhill walls?
pub fn used_downhill(built: &FinsBuilt) -> bool {
    built.props.iter().any(|q| q.downhill == Some(true))
}

/// `wedges` (web/fins/downhill.js:44): how many wedges a build kept.
fn wedges(built: &FinsBuilt) -> usize {
    built
        .fins
        .iter()
        .filter(|f| f.kind == FinKind::Wedge)
        .count()
}

/// The `slack` of `judgeDownhill`: the share of the other build's held area `build(true)` may fall
/// short by, a number or a function of `build(true)`'s result.
pub enum Slack<'a> {
    Fixed(f64),
    By(&'a dyn Fn(&FinsBuilt) -> f64),
}

/// One run of the judge's `build(flag)`: `{ b, caps, low }`.
struct Run {
    b: FinsBuilt,
    caps: Vec<TineSeed>,
    low: f64,
}

/// Port of `judgeDownhill` (web/fins/downhill.js:56-84): Auto's build, with downhill walls only
/// where they leave the part better off. `build(downhill, ctx)` is buildFinsCore with opts.downhill
/// set. The lowest grip is each build's lowest KEPT tine, read through the tine capture
/// (tines.js's globalThis.__TINECAP, here `ctx.tine_cap`, which the races already trim to the walls
/// they keep); a caller's own capture ends up holding the returned build's tines only.
/// `slack`: see [`Slack`] (0 for downhill walls; Curved walls in Full coverage allow
/// CURVED.fullSlack when a curve is there to keep).
///
/// `cls` is `classify(topo, result, rot)` (JS recomputes it in the branch that needs it; it is a pure
/// function of `result`, so `None` computes it there and `Some` is a result that carries its coverage
/// classes).
///
/// JS's `finally { globalThis.__TINECAP = outer }` is a `Drop` guard here, so a panicking build
/// restores the capture too.
pub fn judge_downhill(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    mut build: impl FnMut(bool, &mut BuildCtx) -> FinsBuilt,
    slack: Slack,
    cls: Option<&[u8]>,
    ctx: &mut BuildCtx,
) -> FinsBuilt {
    struct Restore<'a, 'b> {
        ctx: &'a mut BuildCtx<'b>,
        outer: Option<Vec<TineSeed>>,
    }
    impl Drop for Restore<'_, '_> {
        fn drop(&mut self) {
            // a throwing build must not leave the capture on
            *self.ctx.tine_cap.borrow_mut() = self.outer.take();
        }
    }
    let outer = ctx.tine_cap.borrow().clone();
    let mut guard = Restore { ctx, outer };
    let mut run = |flag: bool, ctx: &mut BuildCtx| -> Run {
        *ctx.tine_cap.borrow_mut() = Some(Vec::new());
        let b = build(flag, ctx);
        let caps = ctx.tine_cap.borrow().clone().unwrap_or_default();
        let mut low = f64::INFINITY;
        for t in &caps {
            if t.z < low {
                low = t.z;
            }
        }
        Run { b, caps, low }
    };
    let a = run(true, guard.ctx);
    let mut keep = a;
    if used_downhill(&keep.b) {
        let n = run(false, guard.ctx);
        let cfg = guard.ctx.cfg;
        let computed;
        let cls: &[u8] = match cls {
            Some(c) => c,
            None => {
                computed = classify(topo, result, rot);
                &computed
            }
        };
        let hn = held_area(topo, result, rot, &n.b, cls, cfg);
        let s = match slack {
            Slack::Fixed(s) => s,
            Slack::By(f) => f(&keep.b),
        };
        let worse = held_area(topo, result, rot, &keep.b, cls, cfg) < hn - s * hn - 1e-6
            || keep.low
                > n.low
                    + if n.low.is_finite() {
                        grip_rise(n.low)
                    } else {
                        0.0
                    }
            || wedges(&keep.b) < wedges(&n.b);
        if worse {
            keep = n;
        }
    }
    if let Some(o) = guard.outer.as_mut() {
        o.extend(keep.caps.iter().copied());
    }
    let _ = jsnum::abs(0.0);
    keep.b
}
