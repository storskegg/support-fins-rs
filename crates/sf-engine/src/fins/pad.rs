//! Port of `web/fins/pad.js`. Read `docs/AGENTS.md` before changing anything here.

use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

use crate::config::Config;

use crate::jsnum;
use crate::prop::surface::surface_z_at;
use crate::tris::Tris;
use crate::types::{Pt3, V4};
use rayon::prelude::*;

/// `PAD.style`: `'auto'`, `'light'`, `'sure'` or `'custom'` (web/fins/pad.js:54).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadStyle {
    Auto,
    Light,
    Sure,
    Custom,
}

/// `PAD.custom` (web/fins/pad.js:55): the user's numbers for the `'custom'` style.
#[derive(Clone, Debug, PartialEq)]
pub struct PadCustom {
    pub h: f64,
    pub gap: f64,
    pub grip: f64,
    pub margin: f64,
}

impl Default for PadCustom {
    fn default() -> Self {
        PadCustom {
            h: 0.5,
            gap: 0.0,
            grip: 0.05,
            margin: 4.0,
        }
    }
}

/// `PAD` (web/fins/pad.js:12). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct PadConfig {
    // mm; radial vertex spacing across the conforming oval disc
    pub cell: f64,
    // mm the pad rises PAST the part underside to bite in near the
    // contact, instead of standing off. A tilted part rests on a
    // knife edge; a pad held 0.2mm below it never touches (the "huge
    // gap"), so the part peeled while its own edge did all the
    // anchoring. The pad is a baked brim -- it has to CONNECT. But a
    // 0.15mm bite read as "too close"/welded, so this is a light TACK:
    // just enough to connect the part to the wide open-bed grip
    // (padMargin), which does the actual holding, while staying thin
    // enough to snap off clean. Capped at padH so the tack only lands
    // on the low near-edge strip; bounded by grab it can never
    // recreate the deep 0.46mm slab weld the old flat pad made.
    //
    // May be NEGATIVE: PETG welds to a support far harder than the
    // PLA this 0.05 tack was tuned for, so the PETG profile sets a
    // gap (-0.1) -- the pad stands a hair BELOW the part and snaps
    // off clean. `conform` floors every column at 0.05 so a gap pad
    // stays a valid watertight solid AND still kisses the part at
    // the resting edge (where the underside drops to the plate) to
    // hold it, while gapping off across the rest of the footprint.
    pub grab: f64,
    // BRIM-STYLE pad (experimental, off by default). The default pad is 0.5mm (2-3
    // layers) and tacks 0.05mm into the part, so the slicer merges pad + part into
    // ONE region on the first layers and runs solid infill straight across the
    // contact -- a weld. A slicer brim comes off easily for three reasons, and this
    // copies them: it is ONE layer (only the first layer grips the bed, so more
    // layers add stiffness and weld height, never adhesion); it stands a small gap
    // off the part's first-layer outline, so the slicer keeps two regions and runs
    // perimeters PARALLEL to the part (bead beside bead, not infill through); and
    // first-layer squish closes that gap just enough to hold. A tilted part's
    // flank still prints its second layer onto the pad's inner edge, but only a
    // strip ~layer/tan(tilt) wide, since the pad is only one layer tall.
    //
    // Pad styles, picked in the UI (Bed pad):
    //   'auto'  -- the default: 'light', except on a small foot (a first-layer
    //              outline under minGripOutline) where it builds 'sure' instead.
    //   'light' -- the brim-style pad above: one layer, brimGap off the part. It
    //              held a PETG cube on its edge and came off clean.
    //   'sure'  -- the original conforming pad: padH thick, tacked `grab` into the
    //              part. Holds harder, harder to remove.
    //   'custom'-- the brim-style mesh with every number the user's (PAD.custom).
    pub style: PadStyle,
    pub custom: PadCustom,
    /// JS key `brimGap`.
    // mm off the part's first-layer outline. Orca's brim-object gap
    // is 0.1, but a brim is generated from the slices; this pad is
    // geometry, and PrusaSlicer / Orca / Bambu close any slice gap
    // under 2 x slice_closing_radius (0.049) = 0.098mm. At 0.1 the
    // gap came through at exactly 45deg and was welded shut at 40deg
    // (0.089mm) -- so it needs the margin.
    pub brim_gap: f64,
    /// JS key `minGripOutline`.
    // mm of first-layer outline below which Light can't grip
    // (a point, a cone tip, a small nub): Light becomes Sure hold.
    // A judgement call, not a measurement -- the one print that
    // held had 80mm (a 40mm cube's edge, both sides).
    pub min_grip_outline: f64,
    /// JS key `brimCell`.
    // mm mesh spacing; the gap is only as true as the mesh that
    // samples it (the 1.2mm oval cells interpolate across it)
    pub brim_cell: f64,
}

impl Default for PadConfig {
    fn default() -> Self {
        PadConfig {
            cell: 1.2,
            grab: 0.05,
            style: PadStyle::Auto,
            custom: PadCustom::default(),
            brim_gap: 0.12,
            min_grip_outline: 20.0,
            brim_cell: 0.1,
        }
    }
}

/// What `buildPad` returns besides the triangles it pushes (web/fins/pad.js:184, 250, 520-521).
/// Track D's `fins::build::PadInfo` is this shape; `style` is `Sure`, `Light` or `Custom` (never
/// `Auto`). `auto_sure` is only set by the conforming pad, `small_foot` and `outline` by `tag`.
#[derive(Clone, Debug, PartialEq)]
pub struct PadInfo {
    pub r1: f64,
    pub r2: f64,
    pub cells: usize,
    pub height: f64,
    pub points: usize,
    pub oval: bool,
    pub style: PadStyle,
    pub auto_sure: Option<bool>,
    pub small_foot: Option<bool>,
    pub outline: Option<f64>,
}

/// `firstLayerOutline`'s result: a flat `[x0, y0, x1, y1, ...]` segment list and its length.
struct Outline {
    segs: Vec<f64>,
    length: f64,
}

/// Port of `firstLayerOutline` (web/fins/pad.js:74-91): the part's section at the layer's
/// mid-height (where a slicer cuts).
fn first_layer_outline(part_tris: &Tris, zc: f64) -> Outline {
    let pt = &part_tris.v;
    let mut segs = Vec::new();
    let mut length = 0.0;
    let mut i = 0;
    while i < pt.len() {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for a in 0..3 {
            let p = i + a * 3;
            let q = i + ((a + 1) % 3) * 3;
            let za = pt[p + 2] - zc;
            let zb = pt[q + 2] - zc;
            if (za < 0.0) == (zb < 0.0) {
                continue;
            }
            let t = za / (za - zb);
            pts.push([
                pt[p] + t * (pt[q] - pt[p]),
                pt[p + 1] + t * (pt[q + 1] - pt[p + 1]),
            ]);
        }
        if pts.len() != 2 {
            i += 9;
            continue;
        }
        segs.extend([pts[0][0], pts[0][1], pts[1][0], pts[1][1]]);
        length += jsnum::hypot(&[pts[1][0] - pts[0][0], pts[1][1] - pts[0][1]]);
        i += 9;
    }
    Outline { segs, length }
}

/// The oval's frame (`{ cx, cy, ax, ay, bx, by, r1, r2 }`, web/fins/pad.js:171).
struct Frame {
    cx: f64,
    cy: f64,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    r1: f64,
    r2: f64,
}

/// Port of `buildPad` (web/fins/pad.js:117-251): a breakaway pad under the part's bed contact,
/// its triangles' vertices pushed onto `out`. `None` is JS `null` (fewer than three contact points).
///
/// Not a nicety: a part tilted into a strong orientation rests on an EDGE, so its bed contact is
/// near zero and it peels off before the fins have anything to hold. The pad is a wide, thin
/// footprint -- thin enough to cut off, wide enough to stick. It is fed the part's lowest VERTICES
/// for its outline AND the seated part triangles, so its TOP can hold `gap` clear of the part
/// instead of welding to it. `layer_h` is JS's `layerH = FIN.tineH` default, passed explicitly.
pub fn build_pad(
    contact: &[[f64; 2]],
    part_tris: &Tris,
    out: &mut Vec<V4>,
    layer_h: f64,
    cfg: &Config,
) -> Option<PadInfo> {
    // the pad's vertices are untagged (tag 0.0 in a soup)
    let mut local: Vec<[f64; 3]> = Vec::new();
    let info = build_pad_inner(contact, part_tris, &mut local, layer_h, cfg);
    out.extend(local.iter().map(|v| v.v4()));
    info
}

fn build_pad_inner(
    contact: &[[f64; 2]],
    part_tris: &Tris,
    out: &mut Vec<[f64; 3]>,
    layer_h: f64,
    cfg: &Config,
) -> Option<PadInfo> {
    let fin = &cfg.fin;
    let pad = &cfg.pad;
    if contact.len() < 3 {
        return None;
    }

    let mut cx = 0.0;
    let mut cy = 0.0;
    for p in contact {
        cx += p[0];
        cy += p[1];
    }
    cx /= contact.len() as f64;
    cy /= contact.len() as f64;

    // principal axis of the contact, so a long thin edge gets a long thin pad
    // instead of a circle sized to its length
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in contact {
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let tr = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let lam = tr / 2.0 + jsnum::sqrt(jsnum::max(0.0, (tr * tr) / 4.0 - det));
    let mut ax = sxy;
    let mut ay = lam - sxx;
    if jsnum::hypot(&[ax, ay]) < 1e-9 {
        ax = 1.0;
        ay = 0.0;
    }
    let an = jsnum::hypot(&[ax, ay]);
    ax /= an;
    ay /= an;
    let bx = -ay;
    let by = ax;

    let (mut e1, mut e2) = (0.0, 0.0);
    for p in contact {
        cfg.cancel.check();
        let dx = p[0] - cx;
        let dy = p[1] - cy;
        e1 = jsnum::max(e1, jsnum::abs(dx * ax + dy * ay));
        e2 = jsnum::max(e2, jsnum::abs(dx * bx + dy * by));
    }
    let margin = if pad.style == PadStyle::Custom {
        pad.custom.margin
    } else {
        fin.pad_margin
    };
    let r1 = e1 + margin;
    let r2 = e2 + margin;

    // A CONFORMING ELLIPTICAL DISC, not a boxy grid. Matthew wanted the pad to read
    // as a clean oval, but it still has to duck under a tilted part's flank the way
    // the old heightfield did. So build a radial mesh of the ellipse -- rings of
    // vertices from the centre out to (r1, r2) -- and give each vertex its own
    // conformed top height, exactly as a grid cell used to. The result is a smooth
    // oval outline whose TOP follows the part surface. It replaces both the boxy
    // grid AND the flat-ellipse fast path (which only fired on open-bed footprints,
    // so a tilted cube -- the case Matthew was looking at -- never became an oval).
    //
    // Height rule per vertex, unchanged from the grid: OUTBOARD (no part overhead)
    // rises to the full padH for bed grip; UNDER the part it caps at part_low + grab
    // to bite in a hair rather than stand off or weld deep. low+grab is always >= grab
    // (0.05mm) > 0, so every column has positive height and the mesh stays a valid,
    // watertight solid -- no cells to drop, no holes in the disc.
    let mut sure_grab = pad.grab; // raised to >= 0 below when Light swaps to Sure hold
    let frame = Frame {
        cx,
        cy,
        ax,
        ay,
        bx,
        by,
        r1,
        r2,
    };
    let l1 = if layer_h.is_finite() && layer_h > 0.05 {
        layer_h
    } else {
        fin.tine_h
    };
    // The Light pad holds by first-layer squish along the part's first-layer
    // OUTLINE, 0.12mm off. Along a cube's edge that is 80mm of contact and it held a
    // PETG print; around a part balanced on a point or a small nub (a sphere, a cone
    // tip, the shelter hub's ball foot) the outline is a few mm and there is next
    // to nothing to squish against. Sure hold tacks under the whole low footprint,
    // and it is what the hubs printed with. So below PAD.minGripOutline of outline,
    // Auto builds Sure hold and says so (pad.autoSure); otherwise Light. An explicit
    // Light or Custom is the user's call and is left alone; `smallFoot` rides along
    // so the UI can warn when that pad has a gap.
    let outline = first_layer_outline(part_tris, l1 / 2.0);
    let small_foot = outline.length < pad.min_grip_outline;
    let tag = |mut info: PadInfo| {
        info.small_foot = Some(small_foot);
        info.outline = Some(outline.length);
        Some(info)
    };
    let light = pad.style == PadStyle::Light || (pad.style == PadStyle::Auto && !small_foot);
    if light {
        return tag(brim_pad(
            part_tris,
            contact,
            &frame,
            l1,
            pad.brim_gap,
            0.0,
            l1,
            &outline.segs,
            out,
            cfg,
        ));
    }
    if pad.style == PadStyle::Custom {
        let c = &pad.custom;
        return tag(brim_pad(
            part_tris,
            contact,
            &frame,
            c.h,
            c.gap,
            c.grip,
            l1,
            &outline.segs,
            out,
            cfg,
        ));
    }
    let auto_sure = pad.style == PadStyle::Auto;
    // The swap exists to HOLD a part that barely touches the plate, so the pad must
    // meet it. PETG's Sure hold stands a gap (grab -0.1) under the part to snap off
    // clean; on a small foot that gap moved the pad ~0.7mm off a sphere's first-layer
    // dot -- further than Light's gap -- so the swap changed the label and nothing
    // else (Matthew, PETG sphere). Here the pad at least touches: flush or the
    // material's tack, never a gap.
    if auto_sure {
        sure_grab = jsnum::max(pad.grab, 0.0);
    }
    let conform = |x: f64, y: f64| -> f64 {
        match surface_z_at(part_tris, x, y) {
            None => fin.pad_h,
            // grab may be negative (a PETG gap). Floor at 0.05 so every column stays
            // positive -- the disc watertight, no dropped cells -- and so a gap pad still
            // kisses the part where its underside drops to the plate (the resting edge) to
            // hold it, while gapping off across the rest of the footprint.
            Some(low) => jsnum::max(0.05, jsnum::min(fin.pad_h, low + sure_grab)),
        }
    };
    let n_theta = fin.pad_segs as usize;
    let n_ring = jsnum::max(2.0, jsnum::ceil(jsnum::max(r1, r2) / pad.cell)) as usize;

    // Ring/segment vertex in world XY, at radial fraction `fr` and angle index `j`.
    let v_at = |fr: f64, j: usize| -> [f64; 2] {
        let a = (2.0 * std::f64::consts::PI * j as f64) / n_theta as f64;
        let s = r1 * fr * jsnum::cos(a);
        let t = r2 * fr * jsnum::sin(a);
        [cx + ax * s + bx * t, cy + ay * s + by * t]
    };
    // Precompute the vertex ring positions + their conformed tops once (reused by the
    // top cap, the side wall, and the flat bottom), so every shared edge is keyed
    // from bit-identical coordinates and the soup stays edge-manifold.
    let mut v: Vec<Vec<[f64; 3]>> = Vec::new(); // v[i][j] = [x, y, topZ], i in 0..=nRing
    for i in 0..=n_ring {
        let fr = i as f64 / n_ring as f64;
        let mut row = Vec::new();
        for j in 0..n_theta {
            let [x, y] = v_at(fr, j);
            row.push([x, y, conform(x, y)]);
        }
        v.push(row);
    }
    let centre_top = v[0][0]; // ring 0 collapses to the centre (fr = 0)
    let mut max_top = 0.0;
    for row in &v {
        for p in row {
            if p[2] > max_top {
                max_top = p[2];
            }
        }
    }

    for j in 0..n_theta {
        let jn = (j + 1) % n_theta;
        // TOP surface (normal up). Inner fan from the centre, then quad strips outward.
        out.extend([centre_top, v[1][j], v[1][jn]]);
        for i in 1..n_ring {
            out.extend([v[i][j], v[i + 1][j], v[i + 1][jn]]);
            out.extend([v[i][j], v[i + 1][jn], v[i][jn]]);
        }
        // BOTTOM surface at z=0 (normal down: reverse the top winding).
        let b0 = [cx, cy, 0.0];
        let bi = |i: usize, k: usize| [v[i][k][0], v[i][k][1], 0.0];
        out.extend([b0, bi(1, jn), bi(1, j)]);
        for i in 1..n_ring {
            out.extend([bi(i, j), bi(i + 1, jn), bi(i + 1, j)]);
            out.extend([bi(i, j), bi(i, jn), bi(i + 1, jn)]);
        }
        // SIDE wall around the outer ring, top down to the plate (normal outward).
        let o_r = n_ring;
        let tj = v[o_r][j];
        let tjn = v[o_r][jn];
        let bj = [tj[0], tj[1], 0.0];
        let bjn = [tjn[0], tjn[1], 0.0];
        out.extend([tj, bj, bjn]);
        out.extend([tj, bjn, tjn]);
    }

    tag(PadInfo {
        r1,
        r2,
        cells: n_theta * n_ring,
        height: max_top,
        points: contact.len(),
        oval: true,
        style: PadStyle::Sure,
        auto_sure: Some(auto_sure),
        small_foot: None,
        outline: None,
    })
}

/// Port of `brimPad` (web/fins/pad.js:274-522): the BRIM-STYLE pad (PAD.style 'light', and
/// 'custom'): the same oval, `h` tall (one layer for 'light'), standing `g` off the part's
/// first-layer outline -- see PAD.style for why. `grab` raises (or, negative, lowers) its top
/// against the part's underside the way PAD.grab does for the 'sure' pad; 'light' uses 0.
///
/// The gap has to come through the SLICER, which merges any slice gap under ~0.098mm, so it is
/// held exactly on the first layer: the part's first-layer outline is its section at that
/// layer's mid-height (what the slicer prints), and the pad's top there ramps LINEARLY with the
/// true distance from that outline, crossing mid-height exactly at `g`. A linear ramp survives
/// the mesh's linear interpolation; the earlier min-of-underside field stepped at vertical faces
/// and let the sliced edge drift to 0.036mm off a cube's end face. The pad is also held below
/// the lowest underside within `g` (plus `grab`), which keeps it clear of the part on any layer
/// above the first (Custom thickness).
///
/// The mesh is columns across the oval's long axis, each running rim to rim with the same number
/// of rows, at `brimCell` spacing, so the outline stays the smooth oval. The two tips close with
/// a fan; the bottom is one flat fan (the ellipse is convex).
///
/// Vertices are shared by identity in JS (`node`, `tipL`, a block's hub): they are kept in an
/// arena here and the triangle lists hold indices, so `a !== b` degeneracy checks and `folds`
/// compare indices exactly as JS compares objects.
#[allow(clippy::too_many_arguments)]
fn brim_pad(
    part_tris: &Tris,
    contact: &[[f64; 2]],
    e: &Frame,
    h: f64,
    g: f64,
    grab: f64,
    layer_h: f64,
    outline: &[f64],
    out: &mut Vec<[f64; 3]>,
    cfg: &Config,
) -> PadInfo {
    let fin = &cfg.fin;
    let pad = &cfg.pad;
    let Frame {
        cx,
        cy,
        ax,
        ay,
        bx,
        by,
        r1,
        r2,
    } = *e;
    let big_h = if h.is_finite() && h > 0.05 {
        h
    } else {
        fin.tine_h
    };
    let cell = pad.brim_cell;
    let zc = layer_h / 2.0; // first layer's mid-height: the slicer's cut
    let mut ring: Vec<[f64; 2]> = Vec::new();
    if g > 0.0 {
        for k in 0..16 {
            let k = k as f64;
            ring.push([
                g * jsnum::cos((k * std::f64::consts::PI) / 8.0),
                g * jsnum::sin((k * std::f64::consts::PI) / 8.0),
            ]);
        }
    }

    // The part's first-layer outline (firstLayerOutline, cut at zc), bucketed.
    let b = 0.5;
    let reach = jsnum::max(g, 0.0) + 2.0 * cell;
    let mut bucket: IndexMap<i32, Vec<f64>> = IndexMap::new();
    // `i * 73856093 ^ j * 19349663`: both products are doubles; `^` takes ToInt32 of each
    let key = |i: f64, j: f64| -> i32 {
        jsnum::to_int32(i * 73856093.0) ^ jsnum::to_int32(j * 19349663.0)
    };
    let mut i = 0;
    while i < outline.len() {
        cfg.cancel.check();
        let (x0, y0, x1, y1) = (outline[i], outline[i + 1], outline[i + 2], outline[i + 3]);
        let mut bi = jsnum::floor((jsnum::min(x0, x1) - reach) / b);
        while bi <= jsnum::floor((jsnum::max(x0, x1) + reach) / b) {
            let mut bj = jsnum::floor((jsnum::min(y0, y1) - reach) / b);
            while bj <= jsnum::floor((jsnum::max(y0, y1) + reach) / b) {
                let k = key(bi, bj);
                bucket.entry(k).or_default().extend([x0, y0, x1, y1]);
                bj += 1.0;
            }
            bi += 1.0;
        }
        i += 4;
    }
    // Signed distance to the outline, capped at `reach` (negative = inside it).
    let dist = |x: f64, y: f64| -> f64 {
        let arr = bucket.get(&key(jsnum::floor(x / b), jsnum::floor(y / b)));
        let mut d2 = reach * reach;
        if let Some(arr) = arr {
            let mut k = 0;
            while k < arr.len() {
                let x0 = arr[k];
                let y0 = arr[k + 1];
                let dx = arr[k + 2] - x0;
                let dy = arr[k + 3] - y0;
                let l2 = dx * dx + dy * dy;
                let t = if l2 > 0.0 {
                    jsnum::max(0.0, jsnum::min(1.0, ((x - x0) * dx + (y - y0) * dy) / l2))
                } else {
                    0.0
                };
                let ex = x0 + t * dx - x;
                let ey = y0 + t * dy - y;
                d2 = jsnum::min(d2, ex * ex + ey * ey);
                k += 4;
            }
        }
        let d = jsnum::sqrt(d2);
        match surface_z_at(part_tris, x, y) {
            Some(low) if low < zc => -d,
            _ => d,
        }
    };
    // Ramp slope: one cell below the crossing is still above the 0.05 floor, so the
    // two vertices around the crossing are never clamped and interpolate exactly.
    let k = jsnum::max(0.01, zc - 0.05) / cell;
    let top = |x: f64, y: f64| -> f64 {
        let mut low = surface_z_at(part_tris, x, y).unwrap_or(f64::INFINITY);
        for [dx, dy] in &ring {
            low = jsnum::min(
                low,
                surface_z_at(part_tris, x + dx, y + dy).unwrap_or(f64::INFINITY),
            );
        }
        // With no gap asked for (Custom gap 0) grip alone decides -- a bite is a bite.
        let ramp = if g > 0.0 {
            zc + k * (dist(x, y) - g)
        } else {
            f64::INFINITY
        };
        jsnum::max(0.05, jsnum::min(jsnum::min(big_h, low + grab), ramp))
    };
    let at = |s: f64, t: f64| [cx + ax * s + bx * t, cy + ay * s + by * t];
    // every vertex lives in `verts`; an index is the JS object's identity
    let mut verts: Vec<[f64; 3]> = Vec::new();

    // The lattice: columns across the long axis at `brimCell`, rim to rim, and rows
    // as FRACTIONS of each column's half-width (so the outline stays the smooth
    // oval), `brimCell` apart at the widest column. It is grouped into 1.2mm blocks
    // (BLOCK x BLOCK cells), and only a block the part comes near is meshed at the
    // full lattice -- the rest of the top is flat. Every column used to share one
    // set of rows, so a single dip near the rim made rows fine across the whole
    // oval: 2M cells on a 128mm figure in Suggest, where ~16 blocks were not flat,
    // and a RangeError at 4x. A fine block has the same nodes and diagonals as
    // that full grid, so the first-layer gap it slices to is unchanged.
    const BLOCK: usize = 12;
    let n_s = jsnum::max(3.0, jsnum::ceil((2.0 * r1) / cell) + 1.0) as usize;
    let half = |s: f64| r2 * jsnum::sqrt(jsnum::max(0.0, 1.0 - jsnum::pow(s / r1, 2.0)));
    let s_at = |i: usize| -r1 + (2.0 * r1 * i as f64) / (n_s - 1) as f64;
    let fine = cell / r2;
    // `new Set([-1, 1])`: insertion order, SameValueZero (so +0 and -0 are one value)
    let mut fr: Vec<f64> = vec![-1.0, 1.0];
    let mut seen: HashSet<u64> = HashSet::new();
    let zero_key = |v: f64| {
        if v == 0.0 {
            0.0f64.to_bits()
        } else {
            v.to_bits()
        }
    };
    seen.insert(zero_key(-1.0));
    seen.insert(zero_key(1.0));
    let mut add = |v: f64, fr: &mut Vec<f64>| {
        if seen.insert(zero_key(v)) {
            fr.push(v);
        }
    };
    let mut f = 0.0;
    while f <= 1.0 + 1e-12 {
        add(
            jsnum::parse_float(&jsnum::to_fixed(jsnum::min(f, 1.0), 9)),
            &mut fr,
        );
        add(
            -jsnum::min(jsnum::parse_float(&jsnum::to_fixed(f, 9)), 1.0),
            &mut fr,
        );
        f += fine;
    }
    let mut rows = fr;
    rows.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n_r = rows.len();
    let mid = rows
        .iter()
        .position(|&v| v == 0.0)
        .map(|m| m as f64)
        .unwrap_or(-1.0);
    // block boundaries: columns every BLOCK from the left tip, rows every BLOCK out
    // from the long axis, each list ending on its last index
    let mut cb: Vec<usize> = Vec::new();
    let mut i = 0;
    while i + 1 < n_s {
        cb.push(i);
        i += BLOCK;
    }
    cb.push(n_s - 1);
    let mut rb: Vec<usize> = vec![0, n_r - 1];
    let mut r = mid as i64;
    while r > 0 {
        rb.push(r as usize);
        r -= BLOCK as i64;
    }
    let mut r = mid as i64 + BLOCK as i64;
    while r < n_r as i64 - 1 {
        rb.push(r as usize);
        r += BLOCK as i64;
    }
    rb.sort();
    let n_cb = cb.len() - 1;
    let n_rb = rb.len() - 1;
    let mut row_block = vec![0usize; n_r];
    for bl in 0..n_rb {
        cfg.cancel.check();
        for rr in rb[bl]..=rb[bl + 1] {
            row_block[rr] = bl.min(n_rb - 1);
        }
    }

    // Fine blocks: every block within reach of part surface that dips below the
    // pad's flat top -- each such triangle clipped to that height, its XY box grown
    // by the gap ring, the ramp out to `reach`, and a cell. A part triangle is
    // what moves the top (the underside, the gap ring, the first-layer outline the
    // ramp follows), so a block clear of all of them is flat.
    let mut fine_blk = vec![0u8; n_cb * n_rb];
    // (the ramp follows the outline cut at zc, which a Custom pad thinner than half a
    // layer stands below)
    let z_lim = jsnum::max(big_h + jsnum::max(0.0, -grab), zc) + 1e-6;
    let m_ = reach + jsnum::max(g, 0.0) + cell;
    let ds = (2.0 * r1) / (n_s - 1) as f64;
    let pt = &part_tris.v;
    let mut clip: Vec<f64> = Vec::new();
    let mut n = 0;
    while n < pt.len() {
        if jsnum::min(jsnum::min(pt[n + 2], pt[n + 5]), pt[n + 8]) > z_lim {
            n += 9;
            continue;
        }
        clip.clear();
        for a in 0..3 {
            let p = n + a * 3;
            let q = n + ((a + 1) % 3) * 3;
            let zp = pt[p + 2];
            let zq = pt[q + 2];
            if zp <= z_lim {
                clip.extend([pt[p], pt[p + 1]]);
            }
            if (zp <= z_lim) != (zq <= z_lim) {
                let t = (z_lim - zp) / (zq - zp);
                clip.extend([
                    pt[p] + t * (pt[q] - pt[p]),
                    pt[p + 1] + t * (pt[q + 1] - pt[p + 1]),
                ]);
            }
        }
        let (mut s0, mut s1, mut t0, mut t1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        let mut m = 0;
        while m < clip.len() {
            let dx = clip[m] - cx;
            let dy = clip[m + 1] - cy;
            let s = dx * ax + dy * ay;
            let t = dx * bx + dy * by;
            s0 = jsnum::min(s0, s);
            s1 = jsnum::max(s1, s);
            t0 = jsnum::min(t0, t);
            t1 = jsnum::max(t1, t);
            m += 2;
        }
        s0 -= m_;
        s1 += m_;
        t0 -= m_;
        t1 += m_;
        n += 9;
        if s1 < -r1 || s0 > r1 || t1 < -r2 || t0 > r2 {
            continue;
        }
        let blk = BLOCK as f64;
        let hi = (n_cb - 1) as f64;
        let a_lo = jsnum::max(
            0.0,
            jsnum::min(hi, jsnum::floor(jsnum::floor((s0 + r1) / ds) / blk)),
        );
        let a_hi = jsnum::max(
            0.0,
            jsnum::min(hi, jsnum::floor(jsnum::ceil((s1 + r1) / ds) / blk)),
        );
        let mut a = a_lo;
        while a <= a_hi {
            let ai = a as usize;
            // f = t / half(s): half is smallest at one of the block's ends (the ellipse is
            // concave) and at most r2, so t over those bounds the f range (a block
            // straddling s = 0 peaks inside, but there the row padding below covers it)
            let h_a = half(s_at(cb[ai]));
            let h_b = half(s_at(cb[ai + 1]));
            let h_min = jsnum::min(h_a, h_b);
            let h_max = jsnum::max(h_a, h_b);
            let (mut f0, mut f1) = (-1.0, 1.0);
            if h_min > 1e-9 {
                f0 = jsnum::max(-1.0, jsnum::min(t0 / h_min, t0 / h_max));
                f1 = jsnum::min(1.0, jsnum::max(t1 / h_min, t1 / h_max));
            }
            if f0 > f1 {
                a += 1.0;
                continue;
            }
            let r_lo = jsnum::max(0.0, jsnum::floor(mid + f0 / fine) - 1.0);
            let r_hi = jsnum::min((n_r - 1) as f64, jsnum::ceil(mid + f1 / fine) + 1.0);
            if !(r_lo.is_nan() || r_hi.is_nan()) {
                // `for (let b = rowBlock[rLo]; b <= rowBlock[rHi]; b++)`
                let mut bl = row_block[r_lo as usize];
                while bl <= row_block[r_hi as usize] {
                    fine_blk[ai * n_rb + bl] = 1;
                    bl += 1;
                }
            }
            a += 1.0;
        }
    }

    // `top` is a pure function of (x, y) and ~17 surface lookups: the lattice's nodes are known
    // before they are made, so the tops of every node of a fine block and every coarse block's
    // corners and hub are computed up front, in parallel, and `vert` / the hubs read them back.
    // (A block the checks below promote to fine, or a node missed here, is computed on demand: the
    // same function of the same point, so the pad is what it was.)
    let memo: HashMap<[u64; 2], f64> = {
        let node_xy = |i: usize, r: usize| -> [f64; 2] {
            if i == 0 {
                at(-r1, 0.0)
            } else if i == n_s - 1 {
                at(r1, 0.0)
            } else {
                let s = s_at(i);
                at(s, half(s) * rows[r])
            }
        };
        let mut seen_xy: HashSet<[u64; 2]> = HashSet::new();
        let mut pts: Vec<[f64; 2]> = Vec::new();
        let mut want = |p: [f64; 2]| {
            if seen_xy.insert([p[0].to_bits(), p[1].to_bits()]) {
                pts.push(p);
            }
        };
        want(at(-r1, 0.0));
        want(at(r1, 0.0));
        for a in 0..n_cb {
            cfg.cancel.check();
            for bl in 0..n_rb {
                if fine_blk[a * n_rb + bl] != 0 {
                    for i in cb[a]..=cb[a + 1] {
                        for r in rb[bl]..=rb[bl + 1] {
                            want(node_xy(i, r));
                        }
                    }
                } else {
                    let (mut x, mut y) = (0.0, 0.0);
                    for (i, r) in [
                        (cb[a], rb[bl]),
                        (cb[a + 1], rb[bl]),
                        (cb[a], rb[bl + 1]),
                        (cb[a + 1], rb[bl + 1]),
                    ] {
                        let p = node_xy(i, r);
                        want(p);
                        x += p[0] / 4.0;
                        y += p[1] / 4.0;
                    }
                    want([x, y]);
                }
            }
        }
        let zs: Vec<f64> = pts
            .par_iter()
            .map(|p| {
                if cfg.cancel.is_cancelled() {
                    return 0.0; // unused: the poll below unwinds
                }
                top(p[0], p[1])
            })
            .collect();
        cfg.cancel.check();
        pts.iter()
            .zip(zs)
            .map(|(p, z)| ([p[0].to_bits(), p[1].to_bits()], z))
            .collect()
    };
    let top = |x: f64, y: f64| -> f64 {
        memo.get(&[x.to_bits(), y.to_bits()])
            .copied()
            .unwrap_or_else(|| top(x, y))
    };
    let vert = |s: f64, t: f64, verts: &mut Vec<[f64; 3]>| -> usize {
        let [x, y] = at(s, t);
        verts.push([x, y, top(x, y)]);
        verts.len() - 1
    };
    let tip_l = vert(-r1, 0.0, &mut verts);
    let tip_r = vert(r1, 0.0, &mut verts);
    let mut vv: Vec<Vec<Option<usize>>> = vec![Vec::new(); n_s]; // V[i][r]: lattice vertex, made once
    let node = |i: usize,
                r: usize,
                verts: &mut Vec<[f64; 3]>,
                vv: &mut Vec<Vec<Option<usize>>>|
     -> usize {
        if i == 0 {
            return tip_l;
        }
        if i == n_s - 1 {
            return tip_r;
        }
        let col = &mut vv[i];
        if col.len() <= r {
            col.resize(r + 1, None);
        }
        if col[r].is_none() {
            let s = s_at(i);
            let id = vert(s, half(s) * rows[r], verts);
            col[r] = Some(id);
        }
        col[r].unwrap()
    };
    // A coarse block's hub: its four corners' mean. The chords between its nodes
    // make a trapezoid (the columns are straight), and the mean of its corners is
    // inside it, where a point on the curved ellipse can fall outside near a tip.
    let mut centres: IndexMap<usize, usize> = IndexMap::new();
    // Backstop: a coarse block whose corners and centre do not share one top height
    // is not flat after all -- mesh it fine.
    for a in 0..n_cb {
        cfg.cancel.check();
        for bl in 0..n_rb {
            if fine_blk[a * n_rb + bl] != 0 {
                continue;
            }
            let (mut x, mut y) = (0.0, 0.0);
            for (i, r) in [
                (cb[a], rb[bl]),
                (cb[a + 1], rb[bl]),
                (cb[a], rb[bl + 1]),
                (cb[a + 1], rb[bl + 1]),
            ] {
                let nd = node(i, r, &mut verts, &mut vv);
                x += verts[nd][0] / 4.0;
                y += verts[nd][1] / 4.0;
            }
            verts.push([x, y, top(x, y)]);
            let c = verts.len() - 1;
            centres.insert(a * n_rb + bl, c);
            let z = verts[c][2];
            for (i, r) in [
                (cb[a], rb[bl]),
                (cb[a + 1], rb[bl]),
                (cb[a], rb[bl + 1]),
                (cb[a + 1], rb[bl + 1]),
            ] {
                let nd = node(i, r, &mut verts, &mut vv);
                if verts[nd][2] != z {
                    fine_blk[a * n_rb + bl] = 1;
                    break;
                }
            }
        }
    }
    let is_fine = |fb: &Vec<u8>, a: i64, bl: i64| {
        a >= 0
            && a < n_cb as i64
            && bl >= 0
            && bl < n_rb as i64
            && fb[a as usize * n_rb + bl as usize] == 1
    };

    // A coarse block's top triangles: two, or -- next to a fine block, or on the rim
    // -- a fan from its hub round its edge, through every lattice node on a side it
    // shares with a fine block (so no fine vertex sits on a coarse edge: a T-junction
    // is a crack) and every rim node (so the pad's outline is the full lattice's).
    let coarse = |a: usize,
                  bl: usize,
                  fb: &Vec<u8>,
                  verts: &mut Vec<[f64; 3]>,
                  vv: &mut Vec<Vec<Option<usize>>>|
     -> Vec<usize> {
        let (i0, i1, r0, r1_) = (cb[a], cb[a + 1], rb[bl], rb[bl + 1]);
        let (ai, bi) = (a as i64, bl as i64);
        let sub = [
            is_fine(fb, ai - 1, bi),
            is_fine(fb, ai, bi + 1) || bl == n_rb - 1,
            is_fine(fb, ai + 1, bi),
            is_fine(fb, ai, bi - 1) || bl == 0,
        ];
        if !sub.iter().any(|&x| x) {
            return vec![
                node(i0, r0, verts, vv),
                node(i1, r1_, verts, vv),
                node(i1, r0, verts, vv),
                node(i0, r0, verts, vv),
                node(i0, r1_, verts, vv),
                node(i1, r1_, verts, vv),
            ];
        }
        let mut lp: Vec<usize> = Vec::new();
        let add = |v: usize, lp: &mut Vec<usize>| {
            if lp.last() != Some(&v) {
                lp.push(v);
            }
        };
        for r in r0..r1_ {
            if r == r0 || sub[0] {
                let nd = node(i0, r, verts, vv);
                add(nd, &mut lp);
            }
        }
        for i in i0..i1 {
            if i == i0 || sub[1] {
                let nd = node(i, r1_, verts, vv);
                add(nd, &mut lp);
            }
        }
        let mut r = r1_;
        while r > r0 {
            if r == r1_ || sub[2] {
                let nd = node(i1, r, verts, vv);
                add(nd, &mut lp);
            }
            r -= 1;
        }
        let mut i = i1;
        while i > i0 {
            if i == i1 || sub[3] {
                let nd = node(i, r0, verts, vv);
                add(nd, &mut lp);
            }
            i -= 1;
        }
        if lp[0] == *lp.last().unwrap() {
            lp.pop();
        }
        let c = centres[&(a * n_rb + bl)];
        let mut o = Vec::new();
        for m in 0..lp.len() {
            o.extend([c, lp[m], lp[(m + 1) % lp.len()]]);
        }
        o
    };
    // Every top triangle turns the same way as the cells (clockwise in s, t: negative
    // area in XY). Near a tip a side curves enough that a fan can fold back over its
    // neighbours -- reversed facets lying on the top, a broken pad -- so a block whose
    // triangles do not all turn that way is meshed fine instead, until none fold.
    let folds = |t: &[usize], verts: &[[f64; 3]]| -> bool {
        let mut m = 0;
        while m < t.len() {
            let (pi, qi, ri) = (t[m], t[m + 1], t[m + 2]);
            m += 3;
            if pi == qi || qi == ri || ri == pi {
                continue;
            }
            let (p, q, r) = (verts[pi], verts[qi], verts[ri]);
            if (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]) > -1e-12 {
                return true;
            }
        }
        false
    };
    let mut changed = true;
    while changed {
        changed = false;
        for a in 0..n_cb {
            cfg.cancel.check();
            for bl in 0..n_rb {
                if fine_blk[a * n_rb + bl] != 1 {
                    let t = coarse(a, bl, &fine_blk, &mut verts, &mut vv);
                    if folds(&t, &verts) {
                        fine_blk[a * n_rb + bl] = 1;
                        changed = true;
                    }
                }
            }
        }
    }

    let mut tris: Vec<usize> = Vec::new();
    let tri = |a: usize, b: usize, d: usize, tris: &mut Vec<usize>| {
        if a != b && b != d && d != a {
            tris.extend([a, b, d]);
        }
    };
    let mut quads = 0usize;
    for a in 0..n_cb {
        cfg.cancel.check();
        for bl in 0..n_rb {
            if is_fine(&fine_blk, a as i64, bl as i64) {
                // wound like the tip fans, side wall and bottom (it once ran the other way:
                // the top was inside-out against the rest, and Bambu, reading a broken pad,
                // refused the separate-object 3MF over a G-code path conflict with the part)
                for i in cb[a]..cb[a + 1] {
                    for r in rb[bl]..rb[bl + 1] {
                        let (n00, n11, n10, n01) = (
                            node(i, r, &mut verts, &mut vv),
                            node(i + 1, r + 1, &mut verts, &mut vv),
                            node(i + 1, r, &mut verts, &mut vv),
                            node(i, r + 1, &mut verts, &mut vv),
                        );
                        tri(n00, n11, n10, &mut tris);
                        tri(n00, n01, n11, &mut tris);
                        quads += 1;
                    }
                }
                continue;
            }
            quads += 1;
            let t = coarse(a, bl, &fine_blk, &mut verts, &mut vv);
            let mut m = 0;
            while m < t.len() {
                tri(t[m], t[m + 1], t[m + 2], &mut tris);
                m += 3;
            }
        }
    }
    // boundary ring (tipL, the +t rim left to right, tipR, the -t rim right to
    // left) -- every rim node is in the top, coarse blocks included -- then its side
    // wall and the flat bottom fan
    let mut rim = vec![tip_l];
    for i in 1..n_s - 1 {
        rim.push(node(i, n_r - 1, &mut verts, &mut vv));
    }
    rim.push(tip_r);
    let mut i = n_s as i64 - 2;
    while i >= 1 {
        rim.push(node(i as usize, 0, &mut verts, &mut vv));
        i -= 1;
    }
    let mut flat = Vec::new();
    for &rv in &rim {
        verts.push([verts[rv][0], verts[rv][1], 0.0]);
        flat.push(verts.len() - 1);
    }
    verts.push([cx, cy, 0.0]);
    let b0 = verts.len() - 1;
    for m in 0..rim.len() {
        let nx = (m + 1) % rim.len();
        tri(rim[m], flat[m], flat[nx], &mut tris);
        tri(rim[m], flat[nx], rim[nx], &mut tris);
        tri(b0, flat[nx], flat[m], &mut tris);
    }
    // One winding was picked above; flip the lot if it came out inside-out.
    let mut vol = 0.0;
    let mut i = 0;
    while i < tris.len() {
        let (p, q, r) = (verts[tris[i]], verts[tris[i + 1]], verts[tris[i + 2]]);
        vol += p[0] * (q[1] * r[2] - q[2] * r[1]) - p[1] * (q[0] * r[2] - q[2] * r[0])
            + p[2] * (q[0] * r[1] - q[1] * r[0]);
        i += 3;
    }
    let mut i = 0;
    while i < tris.len() {
        if vol < 0.0 {
            out.extend([verts[tris[i]], verts[tris[i + 2]], verts[tris[i + 1]]]);
        } else {
            out.extend([verts[tris[i]], verts[tris[i + 1]], verts[tris[i + 2]]]);
        }
        i += 3;
    }
    PadInfo {
        r1,
        r2,
        cells: quads,
        height: big_h,
        points: contact.len(),
        oval: true,
        style: if pad.style == PadStyle::Custom {
            PadStyle::Custom
        } else {
            PadStyle::Light
        },
        auto_sure: None,
        small_foot: None,
        outline: None,
    }
}
