//! Port of `web/prop/rival.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! A small tube against the patch path, for one region.
//!
//! A SMALL tube (under tubeMinArea, see tubeLine) is a new claim on a region the patch path
//! used to own. It does WORSE on some regions the patch path served (bore_bracket flat: 98 ->
//! 52%, its weld check rejects the only wall; voron_drive_frame x45y30: 36 -> 32%) and better on
//! others (drive frame x60: 22 -> 34%). So the tube is built first, taken back out, the region's
//! split patches are built from the same starting point, and whichever holds more overhang area
//! stays. A tie keeps the patches -- but an arc wall (prop/arc.js), raced the same way, keeps a
//! tie.
//!
//! buildProps drives it from its patch loop:
//!
//! ```text
//! let mark = rivalry.begin(&mut acc, &patches[i]);       // top of every patch
//! ...build the patch's walls...
//! if patches[i].small_tube.is_some() { rivalry.tube_done(&mut acc, i, mark, &mut patches); }
//! rivalry.finish(&mut acc);                              // after the loop
//! ```
//!
//! **Shape divergence (deliberate).** JS hands `tubeRivalry` the loop's accumulators by
//! reference and keeps patches as shared objects (`p.rival`, `rival.last` compare identities).
//! Rust passes the accumulators ([`Acc`]) to each call as `&mut`, keeps the rivals in an arena
//! inside [`TubeRivalry`] (`Patch::rival` is an index into it), and marks the patch that is
//! `rival.last` with `Patch::rival_last` instead of comparing identities.

use std::sync::Arc;

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, MIN_REGION_AREA, Topology};
use crate::prop::build::{Acc, Patch, PropRecord, Skipped};
use crate::types::V4;

/// `snapshot()`'s result: `{ out, props, tines, skipped, sagRisk, served }`.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub out: usize,
    pub props: usize,
    pub tines: usize,
    pub skipped: Skipped,
    pub sag_risk: bool,
    pub served: indexmap::IndexSet<usize>,
}

/// Port of `routeState().snapshot` (web/prop/rival.js:34-35): snapshot of everything the patch
/// loop accumulates, for a route that is built, judged, and maybe taken back out. Shared with
/// the bridge race (prop/bridgerace.js).
pub fn snapshot(acc: &Acc) -> Mark {
    Mark {
        out: acc.out.len(),
        props: acc.props.len(),
        tines: acc.tally.tines,
        skipped: acc.skipped,
        sag_risk: acc.tally.sag_risk,
        served: acc.served.clone(),
    }
}

/// Port of `routeState().restore` (web/prop/rival.js:36-40): undo back to a [`snapshot`].
pub fn restore(acc: &mut Acc, m: &Mark) {
    acc.out.truncate(m.out);
    acc.props.truncate(m.props);
    acc.tally.tines = m.tines;
    acc.tally.sag_risk = m.sag_risk;
    acc.skipped = m.skipped;
    acc.served = m.served.clone();
}

/// `tube`: `{ mark, end, tris, props, held }`.
#[derive(Clone, Debug)]
struct Tube {
    mark: Mark,
    end: Mark,
    tris: Vec<V4>,
    props: Vec<PropRecord>,
    held: f64,
}

/// `rival`: `{ last, tube, faces, tris, reach, slivers, region, start, arc }` (`last` is
/// [`Patch::rival_last`]; `faces` is never read).
#[derive(Clone, Debug)]
struct Rival {
    tube: Tube,
    tris: Arc<Vec<f64>>,
    reach: Option<Arc<Vec<f64>>>,
    slivers: usize,
    region: usize,
    start: Option<Mark>,
    arc: bool,
}

/// What `settle(p)` reads of the previous patch.
#[derive(Clone, Copy, Debug)]
struct Prev {
    rival: Option<usize>,
    last: bool,
}

/// The object `tubeRivalry` returns (`begin`, `tubeDone`, `finish`).
pub struct TubeRivalry<'a> {
    topo: &'a Topology,
    result: &'a Analysis,
    part_tris: &'a [f64],
    rivals: Vec<Rival>,
    last_patch: Option<Prev>,
    /// `[cx, cy, cz, area]` of every overhang face, built on first use.
    over_faces: Option<Vec<[f64; 4]>>,
    span: f64,
}

/// Port of `tubeRivalry` (web/prop/rival.js:46-150). `part_tris` is the seated part soup.
pub fn tube_rivalry<'a>(
    topo: &'a Topology,
    result: &'a Analysis,
    part_tris: &'a [f64],
    cfg: &Config,
) -> TubeRivalry<'a> {
    TubeRivalry {
        topo,
        result,
        part_tris,
        rivals: Vec::new(),
        last_patch: None,
        over_faces: None,
        span: cfg.prop.max_unsupported_span,
    }
}

impl TubeRivalry<'_> {
    /// Undo the region's patches and put its tube's walls back instead, moved to the end of
    /// `out` (so every triRange shifts by the same amount).
    fn keep_tube(&self, acc: &mut Acc, r: &Rival) {
        let t = &r.tube;
        if let Some(start) = &r.start {
            restore(acc, start);
        }
        let shift = acc.out.len() as isize - t.mark.out as isize;
        acc.out.extend_from_slice(&t.tris);
        for q in &t.props {
            let mut q = q.clone();
            for rg in q.tri_ranges.iter_mut() {
                *rg = [
                    (rg[0] as isize + shift) as usize,
                    (rg[1] as isize + shift) as usize,
                ];
            }
            acc.props.push(q);
        }
        acc.tally.tines += t.end.tines - t.mark.tines;
        acc.skipped.add_diff(&t.end.skipped, &t.mark.skipped);
        acc.tally.sag_risk = acc.tally.sag_risk || t.end.sag_risk;
        if !t.props.is_empty() {
            acc.served.insert(r.region);
        } else {
            // nothing either way: dropped, as before
            acc.skipped.sliver += r.slivers;
        }
    }

    /// Overhang area held by support triangles `out[from..]`, judged over EVERY overhang face
    /// within reach of the region, not just the region's own: a wall under one band holds its
    /// neighbours too, and the sweep's coverage counts them all. Same rule as that coverage (a
    /// support vertex within maxUnsupportedSpan in plan and 0-3 mm below the face), so the pick
    /// agrees with the gate that judges it. `own` is `r.reach ?? r.tris`.
    fn held_area(&mut self, own: &[f64], tris: &[V4], from: usize) -> f64 {
        let span = self.span;
        if self.over_faces.is_none() {
            let (pt, area) = (self.part_tris, &self.topo.area);
            let mut over = Vec::new();
            for g in &self.result.regions {
                for &f in &g.faces {
                    let t = f * 9;
                    over.push([
                        (pt[t] + pt[t + 3] + pt[t + 6]) / 3.0,
                        (pt[t + 1] + pt[t + 4] + pt[t + 7]) / 3.0,
                        (pt[t + 2] + pt[t + 5] + pt[t + 8]) / 3.0,
                        area[f],
                    ]);
                }
            }
            self.over_faces = Some(over);
        }
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        let mut k = 0;
        while k < own.len() {
            x0 = jsnum::min(x0, own[k]);
            x1 = jsnum::max(x1, own[k]);
            y0 = jsnum::min(y0, own[k + 1]);
            y1 = jsnum::max(y1, own[k + 1]);
            k += 3;
        }
        let reach = 2.0 * span; // a wall under the region, then its span
        let mut held = 0.0f64;
        for &[cx, cy, cz, a] in self.over_faces.as_ref().unwrap() {
            if cx < x0 - reach || cx > x1 + reach || cy < y0 - reach || cy > y1 + reach {
                continue;
            }
            for v in &tris[from..] {
                if v[2] > cz - 3.0
                    && v[2] < cz + 0.5
                    && jsnum::hypot(&[v[0] - cx, v[1] - cy]) <= span
                {
                    held += a;
                    break;
                }
            }
        }
        held
    }

    /// `settle(p)`: a small tube's region is settled once its last patch is done, however that
    /// patch exits (most of its skips `continue`); its patches' output is then the tail of `out`,
    /// since they were queued together and ran back to back.
    fn settle(&mut self, acc: &mut Acc, p: Option<Prev>) {
        let Some(p) = p else { return };
        let Some(ri) = p.rival else { return };
        if !p.last {
            return;
        }
        let r = self.rivals[ri].clone();
        let own = r.reach.clone().unwrap_or_else(|| r.tris.clone());
        // an arc (prop/arc.js) keeps a tie: it stands under the strip's whole length,
        // the straight tracks only reach it
        let rival = self.held_area(&own, &acc.out, r.start.as_ref().unwrap().out);
        let keep = if r.arc {
            r.tube.held >= rival - 1e-6
        } else {
            r.tube.held > rival + 1e-6
        };
        if keep {
            self.keep_tube(acc, &r);
        } else {
            acc.skipped.sliver += r.slivers;
        }
    }

    /// Top of every patch: settle the previous one; the mark a small tube undoes to.
    pub fn begin(&mut self, acc: &mut Acc, patch: &Patch) -> Option<Mark> {
        let last = self.last_patch;
        self.settle(acc, last);
        self.last_patch = Some(Prev {
            rival: patch.rival,
            last: patch.rival_last,
        });
        if let Some(ri) = patch.rival
            && self.rivals[ri].start.is_none()
        {
            self.rivals[ri].start = Some(snapshot(acc));
        }
        patch.small_tube.as_ref().map(|_| snapshot(acc))
    }

    /// A small tube's walls are built (a tube patch always gets here: its lines are never
    /// empty). Take them back out and queue the region's patches (`patches[i].small_tube`) on
    /// `patches` to have a go; with none big enough, the tube keeps the region.
    pub fn tube_done(&mut self, acc: &mut Acc, i: usize, mark: Mark, patches: &mut Vec<Patch>) {
        let patch = patches[i].clone();
        let own = patch.reach.clone().unwrap_or_else(|| patch.tris.clone());
        let held = self.held_area(&own, &acc.out, mark.out);
        let tube = Tube {
            end: snapshot(acc),
            tris: acc.out[mark.out..].to_vec(),
            props: acc.props[mark.props..].to_vec(),
            held,
            mark: mark.clone(),
        };
        restore(acc, &mark);
        let small = patch.small_tube.clone().unwrap_or_default();
        let mut rivals: Vec<Patch> = small
            .iter()
            .filter(|p| p.area >= MIN_REGION_AREA)
            .cloned()
            .collect();
        let ri = self.rivals.len();
        // slivers count only if the patch path keeps the region, as they did before
        let rival = Rival {
            tube,
            tris: patch.tris.clone(),
            reach: patch.reach.clone(),
            slivers: small.len() - rivals.len(),
            region: patch.region,
            start: None,
            arc: patch.arc,
        };
        if rivals.is_empty() {
            self.keep_tube(acc, &rival);
        }
        self.rivals.push(rival);
        let n = rivals.len();
        for (k, p) in rivals.iter_mut().enumerate() {
            p.region = patch.region;
            p.tris = patch.tris.clone();
            p.rival = Some(ri);
            p.rival_last = k + 1 == n;
        }
        patches.extend(rivals); // for...of reaches patches pushed mid-loop
    }

    /// After the patch loop: settle the last region.
    pub fn finish(&mut self, acc: &mut Acc) {
        let last = self.last_patch;
        self.settle(acc, last);
    }
}
