//! The ties of each pair: where they touch, their shape, and the checks that keep them off everything
//! else. See the module comment in `mod.rs`.

use super::geom::{self, P2};
use super::mesh::{Mesh, inside_section, ray_hits};
use super::sat::{P3, Piece};
use super::settings::{ContactMode, PlateTies};
use super::{Plate, TiePair, centroid};
use crate::solids::ribbon;
use crate::types::V4;

/// What a note says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TieNoteKind {
    /// The edge band could not take this tie; it went on the face instead (Q3: "the app says so").
    FaceFallback,
    /// The contact sank half the sink depth: the full depth came out of a thin section.
    ShallowSink,
    /// This tie could not be placed anywhere its contact mode allows; `reason` says why.
    Skipped,
    /// The two models have no height in common above the bed clearance: no ties at all.
    NoRoom,
}

impl TieNoteKind {
    /// A stable code for the app's translations: `face-fallback`, `shallow-sink`, `skipped`, `no-room`.
    pub fn code(self) -> &'static str {
        match self {
            TieNoteKind::FaceFallback => "face-fallback",
            TieNoteKind::ShallowSink => "shallow-sink",
            TieNoteKind::Skipped => "skipped",
            TieNoteKind::NoRoom => "no-room",
        }
    }
}

/// Why a tie spot was refused (the last reason of a skipped tie).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TieMiss {
    /// The two models do not overlap enough at this height, seen across the gap.
    NoOverlap,
    /// A ray across the gap missed one of the models.
    Missed,
    /// The faces are further apart here than `max_gap` times `reach`.
    TooLong,
    /// The contact patch is on a slope or an edge of this model (more than `max_relief`).
    Slope(usize),
    /// The patch's corners are not inside this model at the sink depth (its edge, or a thin section).
    Edge(usize),
    /// The tie would come within `clearance` of this model or its supports.
    Blocked(usize),
}

impl TieMiss {
    /// A stable code: `no-overlap`, `missed`, `too-long`, `slope`, `edge`, `blocked`.
    pub fn code(self) -> &'static str {
        match self {
            TieMiss::NoOverlap => "no-overlap",
            TieMiss::Missed => "missed",
            TieMiss::TooLong => "too-long",
            TieMiss::Slope(_) => "slope",
            TieMiss::Edge(_) => "edge",
            TieMiss::Blocked(_) => "blocked",
        }
    }

    /// The model the reason is about, if any.
    pub fn model(self) -> Option<usize> {
        match self {
            TieMiss::Slope(m) | TieMiss::Edge(m) | TieMiss::Blocked(m) => Some(m),
            _ => None,
        }
    }
}

/// One note about a pair's ties.
#[derive(Clone, Debug, PartialEq)]
pub struct TieNote {
    pub kind: TieNoteKind,
    /// which tie of the pair (0 = the lowest), when the note is about one
    pub tie: Option<usize>,
    /// a skipped tie's reason
    pub reason: Option<TieMiss>,
    /// The note in English, for the CLI and logs; the app words it from `kind` and `reason`.
    pub text: String,
}

/// The ties of one pair.
#[derive(Clone, Debug, PartialEq)]
pub struct TiePairBuild {
    pub pair: TiePair,
    /// ties asked for (`ties_per_pair`) and placed
    pub requested: usize,
    pub placed: usize,
    pub notes: Vec<TieNote>,
    /// this pair's vertices in [`TieBuild::tris`]: `[start, end)`
    pub range: [usize; 2],
}

/// Every pair's ties: one closed solid per tie, outward wound, three vertices per triangle, plate frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TieBuild {
    pub tris: Vec<[f64; 3]>,
    pub pairs: Vec<TiePairBuild>,
}

/// Which contact rule placed a tie.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Spot {
    Edge,
    Face,
    Any,
}

/// The pair's frame and slices for one tie height.
struct Frame<'a> {
    a: usize,
    b: usize,
    /// a point on model a's outline, the unit direction to b and across
    o: P2,
    d: P2,
    across: P2,
    zb: f64,
    /// cross-sections at the bottom, middle and top of the tie's contact
    sa: [Vec<[P2; 2]>; 3],
    sb: [Vec<[P2; 2]>; 3],
    plate: &'a Plate,
    s: &'a PlateTies,
}

pub(super) fn build(plate: &Plate, s: &PlateTies, pairs: &[TiePair]) -> TieBuild {
    let mut out = TieBuild::default();
    for p in pairs {
        let start = out.tris.len();
        let mut notes = Vec::new();
        let mut placed = 0;
        let requested = s.ties_per_pair as usize;
        match frame_dir(plate, p) {
            None => notes.push(TieNote {
                kind: TieNoteKind::NoRoom,
                tie: None,
                reason: None,
                text: "the two models stand on the same spot: no ties".into(),
            }),
            Some((o, d)) => match plate.tie_range(p.a, p.b, s) {
                None => notes.push(TieNote {
                    kind: TieNoteKind::NoRoom,
                    tie: None,
                    reason: None,
                    text: format!(
                        "no height in common above {} mm off the bed: no ties",
                        s.tunables.bed_clear
                    ),
                }),
                Some((lo, hi)) => {
                    for k in 0..requested {
                        let zc = lo + (hi - lo) * (k as f64 + 0.5) / requested as f64;
                        let half = s.tunables.depth / 2.0;
                        let (zb, zt) = (zc - half, zc + half);
                        let (ma, mb) = (&plate.models[p.a].part, &plate.models[p.b].part);
                        let f = Frame {
                            a: p.a,
                            b: p.b,
                            o,
                            d,
                            across: [-d[1], d[0]],
                            zb,
                            sa: [ma.slice(zb), ma.slice(zc), ma.slice(zt)],
                            sb: [mb.slice(zb), mb.slice(zc), mb.slice(zt)],
                            plate,
                            s,
                        };
                        match place(&f, k) {
                            Ok((tris, spot, shallow)) => {
                                placed += 1;
                                out.tris.extend(tris.iter().map(|v| [v[0], v[1], v[2]]));
                                if spot == Spot::Face && s.contact == ContactMode::EdgeBand {
                                    notes.push(TieNote {
                                        kind: TieNoteKind::FaceFallback,
                                        tie: Some(k),
                                        reason: None,
                                        text: format!(
                                            "tie {} is on the face: the edge band could not take it",
                                            k + 1
                                        ),
                                    });
                                }
                                if shallow {
                                    notes.push(TieNote {
                                        kind: TieNoteKind::ShallowSink,
                                        tie: Some(k),
                                        reason: None,
                                        text: format!(
                                            "tie {} sinks {} mm: a thin section",
                                            k + 1,
                                            s.tunables.sink / 2.0
                                        ),
                                    });
                                }
                            }
                            Err(why) => notes.push(TieNote {
                                kind: TieNoteKind::Skipped,
                                tie: Some(k),
                                reason: Some(why),
                                text: format!("tie {} left out: {}", k + 1, miss_text(plate, why)),
                            }),
                        }
                    }
                }
            },
        }
        out.pairs.push(TiePairBuild {
            pair: p.clone(),
            requested,
            placed,
            notes,
            range: [start, out.tris.len()],
        });
    }
    out
}

fn miss_text(plate: &Plate, why: TieMiss) -> String {
    let key = |m: usize| plate.key(m).to_string();
    match why {
        TieMiss::NoOverlap => "the models do not overlap enough there".into(),
        TieMiss::Missed => "no face there".into(),
        TieMiss::TooLong => "the gap is too wide there".into(),
        TieMiss::Slope(m) => format!("{} is not flat enough there", key(m)),
        TieMiss::Edge(m) => format!("the contact is off {}'s face", key(m)),
        TieMiss::Blocked(m) => format!("{} is in the way", key(m)),
    }
}

/// The pair's frame: a point on model a's outline and the unit direction to model b.
fn frame_dir(plate: &Plate, p: &TiePair) -> Option<(P2, P2)> {
    let (f, t) = ([p.from[0], p.from[1]], [p.to[0], p.to[1]]);
    let d = geom::sub(t, f);
    let l = geom::len(d);
    if l > 1e-9 {
        return Some((f, [d[0] / l, d[1] / l]));
    }
    let (ca, cb) = (
        centroid(&plate.models[p.a].hull),
        centroid(&plate.models[p.b].hull),
    );
    let d = geom::sub(cb, ca);
    let l = geom::len(d);
    (l > 1e-9).then(|| (ca, [d[0] / l, d[1] / l]))
}

/// The extent across the gap that a cross-section covers at all three heights (the patch must be within
/// the silhouette from its bottom to its top).
fn silhouette(sections: &[Vec<[P2; 2]>; 3], across: P2) -> Option<(f64, f64)> {
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    for sec in sections {
        if sec.is_empty() {
            return None;
        }
        let (l, h) = sec
            .iter()
            .flatten()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| {
                let v = geom::dot(*p, across);
                (l.min(v), h.max(v))
            });
        lo = lo.max(l);
        hi = hi.min(h);
    }
    (hi > lo).then_some((lo, hi))
}

/// Place tie `k` of the pair: the first spot its contact mode allows that passes every check.
fn place(f: &Frame, k: usize) -> Result<(Vec<V4>, Spot, bool), TieMiss> {
    let s = f.s;
    let c = s.contact_thickness;
    let base = geom::dot(f.o, f.across);
    let (Some((al, ah)), Some((bl, bh))) =
        (silhouette(&f.sa, f.across), silhouette(&f.sb, f.across))
    else {
        return Err(TieMiss::Missed);
    };
    // offsets across, measured from the frame's origin
    let (al, ah, bl, bh) = (al - base, ah - base, bl - base, bh - base);
    let (lo, hi) = (al.max(bl), ah.min(bh));
    if hi - lo < c {
        return Err(TieMiss::NoOverlap);
    }
    let mid = (lo + hi) / 2.0;
    let mut spots: Vec<(f64, Spot)> = Vec::new();
    match s.contact {
        ContactMode::EdgeBand => {
            let wb = s.band_width.max(c);
            let low = lo + wb / 2.0;
            let high = hi - wb / 2.0;
            // in the band of both models: within wb of each one's own silhouette edge
            let low_ok = low + c / 2.0 <= al + wb && low + c / 2.0 <= bl + wb && low <= high;
            let high_ok = high - c / 2.0 >= ah - wb && high - c / 2.0 >= bh - wb && low <= high;
            let mut sides = vec![(low, low_ok), (high, high_ok)];
            if k % 2 == 1 {
                sides.reverse();
            }
            spots.extend(
                sides
                    .into_iter()
                    .filter(|(_, ok)| *ok)
                    .map(|(v, _)| (v, Spot::Edge)),
            );
            spots.push((mid, Spot::Face));
        }
        ContactMode::Face => spots.push((mid, Spot::Face)),
        ContactMode::Anywhere => {
            // the shortest crossings first
            let n = 16;
            let mut by_len: Vec<(f64, f64)> = (0..=n)
                .filter_map(|i| {
                    let v = lo + c / 2.0 + (hi - lo - c) * i as f64 / n as f64;
                    let p = [f.o[0] + f.across[0] * v, f.o[1] + f.across[1] * v];
                    let ua = ray_hits(&f.sa[1], p, f.d).into_iter().reduce(f64::max)?;
                    let ub = ray_hits(&f.sb[1], p, f.d).into_iter().reduce(f64::min)?;
                    (ub > ua).then_some((ub - ua, v))
                })
                .collect();
            by_len.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
            spots.extend(by_len.into_iter().take(8).map(|(_, v)| (v, Spot::Any)));
            if spots.is_empty() {
                return Err(TieMiss::Missed);
            }
        }
    }
    let mut last = TieMiss::Missed;
    for (v, spot) in spots {
        match tie_at(f, v) {
            Ok((tris, shallow)) => return Ok((tris, spot, shallow)),
            Err(why) => last = why,
        }
    }
    Err(last)
}

/// The tie centred `v` across from the frame's origin, at the frame's height: its solid, and whether
/// a contact sank only half the sink depth.
fn tie_at(f: &Frame, v: f64) -> Result<(Vec<V4>, bool), TieMiss> {
    let s = f.s;
    let t = &s.tunables;
    let c = s.contact_thickness;
    let at = |u: f64, w: f64| -> P2 {
        [
            f.o[0] + f.d[0] * u + f.across[0] * w,
            f.o[1] + f.d[1] * u + f.across[1] * w,
        ]
    };
    // the patch's corners (across offset, slice) and its centre
    let corners = [
        (v - c / 2.0, 0),
        (v + c / 2.0, 0),
        (v + c / 2.0, 2),
        (v - c / 2.0, 2),
    ];
    let mut ua = [0.0; 5];
    let mut ub = [0.0; 5];
    for (i, &(w, h)) in corners.iter().chain(std::iter::once(&(v, 1))).enumerate() {
        let p = at(0.0, w);
        let a = ray_hits(&f.sa[h], p, f.d).into_iter().reduce(f64::max);
        let b = ray_hits(&f.sb[h], p, f.d).into_iter().reduce(f64::min);
        let (Some(a), Some(b)) = (a, b) else {
            return Err(TieMiss::Missed);
        };
        if a >= b {
            return Err(TieMiss::Missed);
        }
        ua[i] = a;
        ub[i] = b;
    }
    let max = |x: &[f64]| x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min = |x: &[f64]| x.iter().copied().fold(f64::INFINITY, f64::min);
    // the faces: model a's surface nearest b, b's nearest a
    let (usa, usb) = (max(&ua), min(&ub));
    let gap = usb - usa;
    if gap <= 0.0 {
        return Err(TieMiss::Missed);
    }
    if gap > s.max_gap * t.reach {
        return Err(TieMiss::TooLong);
    }
    if max(&ua) - min(&ua) > t.max_relief {
        return Err(TieMiss::Slope(f.a));
    }
    if max(&ub) - min(&ub) > t.max_relief {
        return Err(TieMiss::Slope(f.b));
    }
    // sink behind the deepest corner; every corner must then be inside the part (the pins' rule)
    let sunk = |sec: &[Vec<[P2; 2]>; 3], u: f64| {
        corners
            .iter()
            .all(|&(w, h)| inside_section(&sec[h], at(u, w)))
    };
    let mut shallow = false;
    let mut end_a = None;
    let mut end_b = None;
    for (i, depth) in [t.sink, t.sink / 2.0].into_iter().enumerate() {
        if end_a.is_none() && sunk(&f.sa, min(&ua) - depth) {
            end_a = Some(min(&ua) - depth);
            shallow |= i > 0;
        }
        if end_b.is_none() && sunk(&f.sb, max(&ub) + depth) {
            end_b = Some(max(&ub) + depth);
            shallow |= i > 0;
        }
    }
    let Some(uea) = end_a else {
        return Err(TieMiss::Edge(f.a));
    };
    let Some(ueb) = end_b else {
        return Err(TieMiss::Edge(f.b));
    };

    // the sections along the tie: width and rise are piecewise linear between these
    let m = s.middle_thickness;
    let neck = t.neck.min(gap / 2.0);
    let umid = (usa + usb) / 2.0;
    let guard = 2.0 * t.clearance;
    let mut us = vec![uea, usa, usa + neck, umid, usb - neck, usb, ueb];
    if usa + guard < umid {
        us.push(usa + guard);
        us.push(usb - guard);
    }
    us.sort_by(f64::total_cmp);
    us.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let slope = t.arch.to_radians().tan();
    let width = |u: f64| {
        if u <= usa || u >= usb {
            return c;
        }
        let e = (u - usa).min(usb - u);
        if neck > 0.0 {
            c + (m - c) * (e / neck).min(1.0)
        } else {
            m
        }
    };
    let rise = |u: f64| slope * (u - usa).min(usb - u).max(0.0);
    let ring = |u: f64| -> [P3; 4] {
        let w = width(u) / 2.0;
        let z0 = f.zb + rise(u);
        let z1 = z0 + t.depth;
        let p = |x: f64, z: f64| {
            let q = at(u, x);
            [q[0], q[1], z]
        };
        [p(v + w, z0), p(v - w, z0), p(v - w, z1), p(v + w, z1)]
    };
    let rings: Vec<(f64, [P3; 4])> = us.iter().map(|&u| (u, ring(u))).collect();

    // keep off everything: other models and all supports anywhere; its own two models away from the
    // contacts
    for pair in rings.windows(2) {
        let (ua0, r0) = pair[0];
        let (ub0, r1) = pair[1];
        let piece = Piece::new(r0, r1);
        let lo = piece.lo[2] - t.clearance;
        let hi = piece.hi[2] + t.clearance;
        let xy = [
            piece.lo[0] - t.clearance,
            piece.lo[1] - t.clearance,
            piece.hi[0] + t.clearance,
            piece.hi[1] + t.clearance,
        ];
        let hits = |mesh: &Mesh| mesh.each_near(lo, hi, xy, |tri| piece.near(tri, t.clearance));
        for (idx, model) in f.plate.models.iter().enumerate() {
            let own_ok = (idx == f.a && ua0 < usa + guard - 1e-9)
                || (idx == f.b && ub0 > usb - guard + 1e-9);
            if (!own_ok && hits(&model.part)) || hits(&model.supports) {
                return Err(TieMiss::Blocked(idx));
            }
        }
    }
    let secs: Vec<Vec<P3>> = rings.into_iter().map(|(_, r)| r.to_vec()).collect();
    let mut out = Vec::new();
    ribbon(&secs, &mut out);
    Ok((out, shallow))
}
