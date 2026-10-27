//! Port of `web/prop/crest.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! INTERFACE CREST -- the band of a wall that touches the part, printed in a second
//! material (GitHub #21). A toolchanger can lay the last few layers under an overhang
//! in a material that will not bond to the part (PETG under PLA), so the wall holds
//! the part up and still comes away clean.
//!
//! THE MODES (`PROP.iface`). 'flat' (the default when on): a wall station gets a
//! crest only where the part's underside above it is within `ifaceFlatDeg` of level.
//! That is where a PETG interface works -- a flat underside comes out like a top
//! surface -- and it costs a few layers of tool changes, not one every layer. On a
//! tilted underside the crest climbs with the wall top through every layer, and the
//! PETG, laid on a narrow tip, would not print on the PLA (a cube at 35deg, the first
//! print). 'all' crests every station anyway: the user's call per part. false: off.
//!
//! THE SPLIT. A crest station's wall is cut `crestH()` (`PROP.ifaceLayers` of the
//! print's layers) below its top: the BODY
//! below, the CREST above, both closed solids, the crest reaching CREST_OVERLAP down
//! into the body (never flush). Under a flat contact the crest keeps the wall's full
//! `th` (two lines): the tip only narrows so fused PLA snaps off, and here it would
//! only take away the contact and the bond. On a slope ('all') it keeps the tip's
//! taper, or its edge would rise into the part. The tines are interface too:
//! emitTines tags its whole comb, as sway.js does its braces'.
//!
//! THE TAG. Crest and tine triangles go into the same `out` as the wall, so every
//! range the engine and the UI keep over it (triRange(s), per-fin removal, Draw's
//! triStart/triEnd) still holds. What marks them is a 4th component on each vertex,
//! `v[3] === 1`: it rides through concatenation, slicing and the Worker's structured
//! clone, and everything that reads a vertex reads only [0..2]. Only crest and tine
//! solids are tagged, and their vertices are their own arrays, never shared with an
//! untagged solid. The export splits on it (splitInterface).
//!
//! **Rust representation of the tag:** a soup vertex is a [`V4`], `[x, y, z, tag]`, with the tag
//! `1.0` exactly where JS has `v[3] === 1` and `0.0` where JS has `undefined`.
//!
//! Off (the default) nothing here runs and every wall is what it was.
//!
//! Split out of prop.js, which re-exports the public names.

use std::sync::Arc;

use crate::config::Config;
use crate::jsnum;
use crate::prop::config::Iface;
use crate::prop::surface::surface_hits_at;
use crate::solids::ribbon;
use crate::tris::Tris;
use crate::types::V4;

type V3 = [f64; 3];

/// `CREST_OVERLAP` (web/prop/crest.js:44): how far the crest reaches down into the body, so the
/// two overlap: a slicer samples each layer at mid-height, so 0.01 mm of double material never
/// prints.
const CREST_OVERLAP: f64 = 0.01;

/// The module state `part` and `layerH` (web/prop/crest.js:50-51), set by `crestPart`. In Rust
/// a build owns one `Crest` and passes `&Crest` to every function that reads the state.
///
/// The seated part the 'flat' test reads the underside from, and the print's layer height: set
/// at each build's entry (buildFins, drawnWall), which hold the part in print space.
#[derive(Clone, Debug)]
pub struct Crest {
    /// `part`: `None` is JS's `null`.
    pub part: Option<Arc<Tris>>,
    /// `layerH`
    pub layer_h: f64,
}

impl Crest {
    /// Port of `crestPart(tris, layerHeight)` (web/prop/crest.js:51):
    /// `part = tris; layerH = layerHeight || PROP.tineH`. A falsy `layer_height` (0, NaN, or
    /// JS's `undefined`) takes `cfg.prop.tine_h`; `crestPart(null)` is `Crest::new(None, 0.0, cfg)`.
    pub fn new(tris: Option<Arc<Tris>>, layer_height: f64, cfg: &Config) -> Crest {
        Crest {
            part: tris,
            layer_h: if layer_height != 0.0 && !layer_height.is_nan() {
                layer_height
            } else {
                cfg.prop.tine_h
            },
        }
    }
}

/// Port of `crestOn` (web/prop/crest.js:47): is the crest on (either mode)?
pub fn crest_on(cfg: &Config) -> bool {
    cfg.prop.iface != Iface::Off && cfg.prop.iface_layers > 0.0
}

/// Port of `crestH` (web/prop/crest.js:54): the crest's height: `PROP.ifaceLayers` whole layers
/// of the print.
pub fn crest_h(crest: &Crest, cfg: &Config) -> f64 {
    cfg.prop.iface_layers * crest.layer_h
}

/// Port of `flatAt` (web/prop/crest.js:57-67): is the part's underside at contact point `p`
/// (x, y, surface z) flat enough?
fn flat_at(crest: &Crest, p: V3, cfg: &Config) -> bool {
    let Some(part) = &crest.part else {
        return false;
    };
    let mut best: Option<[f64; 2]> = None;
    for [z, nz] in surface_hits_at(part, p[0], p[1]) {
        // `!best || Math.abs(z - p[2]) < Math.abs(best[0] - p[2])`
        let closer = match best {
            Some(b) => jsnum::abs(z - p[2]) < jsnum::abs(b[0] - p[2]),
            None => true,
        };
        if closer {
            best = Some([z, nz]);
        }
    }
    // the face the wall stands under: down-facing, at the contact
    match best {
        Some(b) => {
            jsnum::abs(b[0] - p[2]) < 0.5
                && -b[1] >= jsnum::cos((cfg.prop.iface_flat_deg * std::f64::consts::PI) / 180.0)
        }
        None => false,
    }
}

/// What a station gets from `crestKinds`: `'flat'` or `'slope'` (`null` is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrestKind {
    Flat,
    Slope,
}

/// Port of `crestKinds` (web/prop/crest.js:73-77): what each station of a wall gets, from its
/// contact points (x, y, surface z): `Flat`, `Slope` (only in the 'all' mode) or `None`. A lone
/// crest station has no run to sweep a crest along, so it gets none. `None` (the whole result)
/// when the crest is off.
pub fn crest_kinds(crest: &Crest, pts: &[V3], cfg: &Config) -> Option<Vec<Option<CrestKind>>> {
    if !crest_on(cfg) {
        return None;
    }
    let k: Vec<Option<CrestKind>> = pts
        .iter()
        .map(|&p| {
            if flat_at(crest, p, cfg) {
                Some(CrestKind::Flat)
            } else if cfg.prop.iface == Iface::All {
                Some(CrestKind::Slope)
            } else {
                None
            }
        })
        .collect();
    Some(
        (0..k.len())
            .map(|i| {
                let before = i.checked_sub(1).and_then(|j| k[j]);
                let after = k.get(i + 1).copied().flatten();
                match k[i] {
                    Some(c) if before.is_some() || after.is_some() => Some(c),
                    _ => None,
                }
            })
            .collect(),
    )
}

/// Port of `markInterface` (web/prop/crest.js:80-82): tag `out[from..]` as interface. JS's
/// default `from = 0` is passed explicitly.
pub fn mark_interface(out: &mut [V4], from: usize) {
    for v in out.iter_mut().skip(from) {
        v[3] = 1.0;
    }
}

/// Port of `emitCrest` (web/prop/crest.js:89-98): a wall's crest sections, one per station
/// (`None` where it has none), as tagged closed solids: one per run of consecutive crest
/// stations.
pub fn emit_crest(sections: &[Option<Vec<V3>>], out: &mut Vec<V4>) {
    let from = out.len();
    let mut i = 0;
    while i < sections.len() {
        let mut j = i;
        while j < sections.len() && sections[j].is_some() {
            j += 1;
        }
        if j - i >= 2 {
            let run: Vec<Vec<V3>> = sections[i..j].iter().map(|s| s.clone().unwrap()).collect();
            ribbon(&run, out);
        }
        i = j + 1;
    }
    mark_interface(out, from);
}

/// Port of `dropCollapsed` (web/prop/crest.js:106-116): drop `out[from..]`'s triangles with two
/// corners on one point. A flat crest's body section repeats a corner (its straight side meets
/// the cut at full width), so its ribbon makes zero-area triangles there; dropping them leaves
/// the solid closed (a zero-length edge pairs off the two others), unlike a collinear corner's.
pub fn drop_collapsed(out: &mut Vec<V4>, from: usize) {
    let same = |a: V4, b: V4| a[0] == b[0] && a[1] == b[1] && a[2] == b[2];
    let mut w = from;
    let mut i = from;
    while i + 2 < out.len() {
        let (a, b, c) = (out[i], out[i + 1], out[i + 2]);
        i += 3;
        if same(a, b) || same(b, c) || same(a, c) {
            continue;
        }
        out[w] = a;
        out[w + 1] = b;
        out[w + 2] = c;
        w += 3;
    }
    out.truncate(w);
}

/// Port of `splitInterface` (web/prop/crest.js:119-125): split a triangle list into its body and
/// interface triangles (a triangle is interface when its first vertex is tagged).
pub fn split_interface(tris: &[V4]) -> (Vec<V4>, Vec<V4>) {
    let (mut body, mut iface) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i + 2 < tris.len() {
        let dst = if tris[i][3] == 1.0 {
            &mut iface
        } else {
            &mut body
        };
        dst.extend_from_slice(&tris[i..i + 3]);
        i += 3;
    }
    (body, iface)
}

/// The `ring` a `crestCut` result holds: `(P) => the crest section`, as data (web/prop/crest.js
/// :143-163).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ring {
    /// `[P(+th/2, cb), P(+th/2, top), P(-th/2, top), P(-th/2, cb)]`
    Flat { half: f64, cb: f64, top: f64 },
    /// `[P(+wb, cb), P(+tip/2, top), P(-tip/2, top), P(-wb, cb)]`
    Slope {
        wb: f64,
        tip: f64,
        cb: f64,
        top: f64,
    },
}

/// What `crestCut` returns: `{ cut, zt, wc, ring }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrestCut {
    /// the body's new top
    pub cut: f64,
    /// the body's taper start
    pub zt: f64,
    /// the body's half-width at `cut`
    pub wc: f64,
    ring: Ring,
}

impl CrestCut {
    /// `ring(P)`: the crest section, 4 corners from CREST_OVERLAP under `cut` to `top`, mirrored
    /// about the wall's centre. `p(offset, z)` places a point across the wall at height z.
    pub fn ring(&self, p: impl Fn(f64, f64) -> V3) -> Vec<V3> {
        match self.ring {
            Ring::Flat { half, cb, top } => {
                vec![p(half, cb), p(half, top), p(-half, top), p(-half, cb)]
            }
            Ring::Slope { wb, tip, cb, top } => {
                vec![p(wb, cb), p(tip / 2.0, top), p(-tip / 2.0, top), p(-wb, cb)]
            }
        }
    }
}

/// Port of `crestCut` (web/prop/crest.js:143-163): where a tapered wall top (th wide up to
/// `ztip`, then necking to `tip` at `top`) splits into body and crest, for a station of
/// crestKinds' `kind`. `floor` is the highest bottom vertex of the wall's section: a wall
/// shorter than twice the band keeps its lower half as body. Returns `None` for no crest, else:
///   cut    the body's new top
///   zt     the body's taper start
///   wc     the body's half-width at `cut`
///   ring   (P) => the crest section, 4 corners from CREST_OVERLAP under `cut` to
///          `top`, mirrored about the wall's centre.
/// 'flat': no taper at all -- the body runs full width up to `cut` (zt = cut, wc =
///   th/2: a repeated corner, see dropCollapsed) and the crest is a th-wide block.
/// 'slope': the body keeps its taper up to `cut` (zt is ztip, or just under `cut`
///   when the band reaches below the taper, so the cut lands ON it) and the crest is
///   the trapezoid on the same taper line -- no corner repeated or collinear, which
///   made zero-area triangles in each ribbon end cap.
///
/// JS defaults `th = PROP.th`, `tip = PROP.tip`: callers pass them.
#[allow(clippy::too_many_arguments)]
pub fn crest_cut(
    crest: &Crest,
    top: f64,
    ztip: f64,
    floor: f64,
    kind: Option<CrestKind>,
    th: f64,
    tip: f64,
    cfg: &Config,
) -> Option<CrestCut> {
    let kind = kind?;
    let cut = jsnum::max(top - crest_h(crest, cfg), (floor + top) / 2.0);
    let cb = cut - CREST_OVERLAP;
    if kind == CrestKind::Flat {
        return Some(CrestCut {
            cut,
            zt: cut,
            wc: th / 2.0,
            ring: Ring::Flat {
                half: th / 2.0,
                cb,
                top,
            },
        });
    }
    let zt = jsnum::min(ztip, cut - jsnum::min(0.05, (cut - floor) / 2.0));
    let half = |z: f64| {
        th / 2.0 - ((th - tip) / 2.0) * (jsnum::max(0.0, z - zt) / jsnum::max(1e-6, top - zt))
    };
    let wb = half(cb);
    Some(CrestCut {
        cut,
        zt,
        wc: half(cut),
        ring: Ring::Slope { wb, tip, cb, top },
    })
}

/// Port of `flatCut` (web/prop/crest.js:171): a flat-topped wall's crest (a wedge blade's):
/// `flatCut` is where the body stops (the same rule as crestCut, the bed as floor).
pub fn flat_cut(crest: &Crest, top: f64, cfg: &Config) -> f64 {
    jsnum::max(top - crest_h(crest, cfg), top / 2.0)
}

/// Port of `flatCrestRing` (web/prop/crest.js:172-175): the band from CREST_OVERLAP under
/// `flat_cut` to `top`, `half` either side. `p(offset, z)` places a point across the wall.
pub fn flat_crest_ring(
    crest: &Crest,
    p: impl Fn(f64, f64) -> V3,
    top: f64,
    half: f64,
    cfg: &Config,
) -> Vec<V3> {
    let cb = flat_cut(crest, top, cfg) - CREST_OVERLAP;
    vec![p(half, cb), p(half, top), p(-half, top), p(-half, cb)]
}
