//! PLATE TIES (issue #50): breakaway ties between neighbouring tall, thin models on one plate.
//!
//! Not a port: the first post-parity feature, with no JS counterpart. Nothing the parity paths run
//! calls into this module; with ties off the engine's output is unchanged.
//!
//! A tall, thin model (a coin on its edge, a fin, a figure's blade) wobbles while it prints: a resin
//! print is peeled off the vat floor every layer, and an FDM nozzle drags it. Two such models facing
//! each other across a small gap can hold each other up. A tie is a thin breakaway plate from one to the
//! other: [`TieTunables::depth`] tall, `contact_thickness` wide where it meets each model (where it
//! snaps off) and `middle_thickness` in the gap, rising from each contact to its middle at
//! [`TieTunables::arch`] so that every layer of it stands on the one below, as the wall ties' struts
//! do (`prop/ties.rs`).
//!
//! The steps, each its own function so the app can show the first before it asks for the second:
//!
//! 1. [`Plate::new`]: the models as they stand on the plate (part and supports, plate frame), each
//!    with a caller's key. Their outlines, heights and thin axes are measured once.
//! 2. [`Plate::detect`]: the neighbours. Two models are neighbours when both are tall and thin
//!    (height at least `min_height` and at least `min_ratio` times their thickness), their outlines are
//!    no more than `max_gap` apart and nothing stands between them, and they face each other (the line
//!    between them within [`TieTunables::facing`] of both thin axes, and they overlap by
//!    [`TieTunables::min_overlap`] seen along it). The caller's overrides switch pairs off and add pairs
//!    the criteria missed.
//! 3. [`Plate::build`]: the ties of each pair. `ties_per_pair` of them, spread over
//!    [`PlateTies::height_from`]..[`PlateTies::height_to`] of the height the two have in common and
//!    never under [`TieTunables::bed_clear`]. Each contact follows the automatic resin pins' rules
//!    (`prop/pins.rs`): the patch's four corners must be inside the part at the sink depth (so it bites
//!    the face, not its edge), and a patch that would come out the other side of a thin section sinks
//!    half as deep or is refused. A tie keeps [`TieTunables::clearance`] off every other model and
//!    every support. Each tie is one closed solid overlapping both models (never flush), ready to be
//!    merged into the plate's export.
//!
//! Everything is deterministic: pairs come in the order of their models (first model, then second),
//! ties bottom up, and the notes with them.

pub mod calibration;
mod geom;
mod mesh;
mod sat;
mod settings;
mod ties;

pub use settings::{ContactMode, NUMBERS, NumField, PlateTies, TieTunables};
pub use ties::{TieBuild, TieNote, TieNoteKind, TiePairBuild};

use geom::{
    P2, convex_gap, convex_hull, extent_along, min_width, segment_hits_convex, simplify_hull,
};
use mesh::Mesh;
use rayon::prelude::*;

/// One model on the plate: the caller's key for it (stable across calls: the app's model id), its part
/// and its supports, as flat triangle soups (nine numbers per triangle) in the plate frame: posed,
/// seated and moved to where the model stands.
#[derive(Clone, Debug, Default)]
pub struct TieModel {
    pub key: String,
    pub part: Vec<f64>,
    pub supports: Vec<f64>,
}

/// The user's changes to the detected pairs (Q2): pairs switched off, and pairs added by hand. A pair
/// is two model keys in either order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    pub disabled: Vec<(String, String)>,
    pub added: Vec<(String, String)>,
}

/// How a pair came about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairOrigin {
    /// The detection found it.
    Auto,
    /// The user added it ([`Overrides::added`]); the criteria are not applied.
    Manual,
}

/// Two neighbouring models: `a` and `b` index the plate's models (`a < b`).
#[derive(Clone, Debug, PartialEq)]
pub struct TiePair {
    pub a: usize,
    pub b: usize,
    pub origin: PairOrigin,
    /// mm between the two outlines (0 when they overlap: a manual pair of models that touch)
    pub gap: f64,
    /// The link to draw on the plate: from model `a`'s outline to model `b`'s, at the middle of the
    /// tie height range (plate frame).
    pub from: [f64; 3],
    pub to: [f64; 3],
}

/// One model, measured.
pub(crate) struct Prepared {
    pub key: String,
    pub part: Mesh,
    pub supports: Mesh,
    /// the outline on the bed: the part's convex hull, simplified to [`HULL_TOL`]
    pub hull: Vec<P2>,
    /// thickness (the hull's least width) and its axis
    pub width: f64,
    pub thin: P2,
    pub zmin: f64,
    pub zmax: f64,
}

/// mm: the outline is the hull to within this.
const HULL_TOL: f64 = 0.02;

impl Prepared {
    fn new(m: &TieModel) -> Prepared {
        let part = Mesh::new(&m.part);
        let pts: Vec<P2> = part
            .tris
            .iter()
            .flat_map(|t| t.iter().map(|p| [p[0], p[1]]))
            .collect();
        let hull = simplify_hull(&convex_hull(pts), HULL_TOL);
        let (width, thin) = min_width(&hull);
        let (zmin, zmax) = if part.is_empty() {
            (0.0, 0.0)
        } else {
            (part.bbox[2], part.bbox[5])
        };
        Prepared {
            key: m.key.clone(),
            supports: Mesh::new(&m.supports),
            part,
            hull,
            width,
            thin,
            zmin,
            zmax,
        }
    }

    fn height(&self) -> f64 {
        self.zmax - self.zmin
    }

    /// Tall and thin by `s`.
    fn tall_thin(&self, s: &PlateTies) -> bool {
        let h = self.height();
        h > 0.0 && h >= s.min_height && h >= s.min_ratio * self.width
    }
}

/// The models of one plate, measured once for any number of detections and builds.
pub struct Plate {
    pub(crate) models: Vec<Prepared>,
}

impl Plate {
    /// Measure the models. Keys must be unique and not empty.
    pub fn new(models: &[TieModel]) -> Result<Plate, String> {
        for (i, m) in models.iter().enumerate() {
            if m.key.is_empty() {
                return Err(format!("plate ties: model {} has no key", i + 1));
            }
            if models[..i].iter().any(|o| o.key == m.key) {
                return Err(format!("plate ties: two models have the key {:?}", m.key));
            }
        }
        Ok(Plate {
            models: models.par_iter().map(Prepared::new).collect(),
        })
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// The index of the model with this key.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.models.iter().position(|m| m.key == key)
    }

    /// The model's key.
    pub fn key(&self, i: usize) -> &str {
        &self.models[i].key
    }

    /// The model's thickness (least width of its outline), mm.
    pub fn thickness(&self, i: usize) -> f64 {
        self.models[i].width
    }

    /// The model's height (its part's z extent), mm.
    pub fn height(&self, i: usize) -> f64 {
        self.models[i].height()
    }

    /// The height range [lo, hi] the ties of a pair are spread over, or `None` when the two have no
    /// room for one above `bed_clear`.
    pub(crate) fn tie_range(&self, a: usize, b: usize, s: &PlateTies) -> Option<(f64, f64)> {
        let (ma, mb) = (&self.models[a], &self.models[b]);
        let z0 = ma.zmin.max(mb.zmin);
        let z1 = ma.zmax.min(mb.zmax);
        if z1 <= z0 {
            return None;
        }
        let lo =
            (z0 + s.height_from * (z1 - z0)).max(s.tunables.bed_clear + s.tunables.depth / 2.0);
        let hi = z0 + s.height_to * (z1 - z0);
        (hi > lo).then_some((lo, hi))
    }

    /// The pair's link: its gap and the closest points of the outlines, at the middle of the tie range.
    fn link(&self, a: usize, b: usize, s: &PlateTies) -> (f64, P2, P2, f64) {
        let (ma, mb) = (&self.models[a], &self.models[b]);
        let z = match self.tie_range(a, b, s) {
            Some((lo, hi)) => (lo + hi) / 2.0,
            None => (ma.zmin.max(mb.zmin) + ma.zmax.min(mb.zmax)) / 2.0,
        };
        match convex_gap(&ma.hull, &mb.hull) {
            Some((g, f, t)) => (g, f, t, z),
            None => (0.0, centroid(&ma.hull), centroid(&mb.hull), z),
        }
    }

    /// The pairs to tie: the detected neighbours less `ov.disabled`, plus `ov.added`, in model order.
    /// An added pair names a model that is not on the plate, or the same model twice: an error. A
    /// disabled pair that names no model here is ignored (a stale override).
    pub fn detect(&self, s: &PlateTies, ov: &Overrides) -> Result<Vec<TiePair>, String> {
        s.validate()?;
        let pair_of = |(x, y): &(String, String)| -> Option<(usize, usize)> {
            let (i, j) = (self.index_of(x)?, self.index_of(y)?);
            Some((i.min(j), i.max(j)))
        };
        let mut added: Vec<(usize, usize)> = Vec::new();
        for p in &ov.added {
            let Some((i, j)) = pair_of(p) else {
                return Err(format!(
                    "plate ties: the added pair {:?} - {:?} names a model not on this plate",
                    p.0, p.1
                ));
            };
            if i == j {
                return Err(format!("plate ties: {:?} cannot be tied to itself", p.0));
            }
            added.push((i, j));
        }
        let disabled: Vec<(usize, usize)> = ov.disabled.iter().filter_map(pair_of).collect();
        let n = self.models.len();
        let tall: Vec<bool> = self.models.iter().map(|m| m.tall_thin(s)).collect();
        let cos_facing = s.tunables.facing.to_radians().cos();
        let mut out = Vec::new();
        for a in 0..n {
            for b in a + 1..n {
                if disabled.contains(&(a, b)) {
                    continue;
                }
                let manual = added.contains(&(a, b));
                let auto = tall[a] && tall[b] && self.neighbours(a, b, s, cos_facing);
                if !manual && !auto {
                    continue;
                }
                let (gap, f, t, z) = self.link(a, b, s);
                out.push(TiePair {
                    a,
                    b,
                    origin: if auto {
                        PairOrigin::Auto
                    } else {
                        PairOrigin::Manual
                    },
                    gap,
                    from: [f[0], f[1], z],
                    to: [t[0], t[1], z],
                });
            }
        }
        Ok(out)
    }

    /// Do `a` and `b` face each other across a gap of at most `max_gap`, with nothing between them?
    fn neighbours(&self, a: usize, b: usize, s: &PlateTies, cos_facing: f64) -> bool {
        let (ma, mb) = (&self.models[a], &self.models[b]);
        if ma.hull.len() < 3 || mb.hull.len() < 3 {
            return false;
        }
        let Some((gap, f, t)) = convex_gap(&ma.hull, &mb.hull) else {
            return false;
        };
        if gap > s.max_gap || gap <= 0.0 {
            return false;
        }
        let d = [(t[0] - f[0]) / gap, (t[1] - f[1]) / gap];
        if geom::dot(d, ma.thin).abs() < cos_facing || geom::dot(d, mb.thin).abs() < cos_facing {
            return false;
        }
        let across = [-d[1], d[0]];
        let (al, ah) = extent_along(&ma.hull, across);
        let (bl, bh) = extent_along(&mb.hull, across);
        if ah.min(bh) - al.max(bl) < s.tunables.min_overlap {
            return false;
        }
        // nothing in between: the link crosses no third model's outline
        !self.models.iter().enumerate().any(|(k, m)| {
            k != a && k != b && m.hull.len() >= 3 && segment_hits_convex(&m.hull, f, t)
        })
    }

    /// Build the ties of `pairs` (from [`detect`](Self::detect)): one closed solid per tie, with the
    /// notes of every pair.
    pub fn build(&self, s: &PlateTies, pairs: &[TiePair]) -> Result<TieBuild, String> {
        s.validate()?;
        for p in pairs {
            if p.a >= self.models.len() || p.b >= self.models.len() || p.a == p.b {
                return Err(format!(
                    "plate ties: pair {} - {} is not two models of this plate",
                    p.a, p.b
                ));
            }
        }
        Ok(ties::build(self, s, pairs))
    }
}

fn centroid(poly: &[P2]) -> P2 {
    if poly.is_empty() {
        return [0.0, 0.0];
    }
    let n = poly.len() as f64;
    let (x, y) = poly
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
    [x / n, y / n]
}
