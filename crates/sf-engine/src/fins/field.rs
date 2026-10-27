//! Port of `web/fins/field.js`. Read `docs/AGENTS.md` before changing anything here.

/// `FIELD` (web/fins/field.js:17). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldConfig {
    // mm: the plan grid the surface is binned into
    pub cell: f64,
    // mm: the Gaussian's sigma -- how far "the model here" reaches
    pub radius: f64,
    /// JS key `minRatio`.
    // long axis / short axis (std dev) under this: no heading
    pub min_ratio: f64,
    // deg: a heading this close to x or y is x or y
    pub snap: f64,
}

impl Default for FieldConfig {
    fn default() -> Self {
        FieldConfig {
            cell: 2.0,
            radius: 20.0,
            min_ratio: 1.25,
            snap: 10.0,
        }
    }
}

use indexmap::IndexMap;

use crate::config::Config;
use crate::jsnum;
use crate::overhangs::{Analysis, Topology};
use crate::types::Rot3;

/// Port of `headingField` (web/fins/field.js:29-68): the model's LENGTH in plan, the heading Full
/// coverage's Long walls run along (fins/rows.js, opts.longWalls).
///
/// The heading at (x, y) is the long axis of the part's surface within `FIELD.radius` in plan
/// (Gaussian-weighted second moments of the faces' plan centroids, by area). The fill pass asks
/// once, at the bare red's centroid, so the rows share one heading. Where the surface there is
/// about as long as it is wide (`FIELD.minRatio`) there is no length: `None` (JS `null`) -- a
/// square footprint's eigenvector is arbitrary (the old auto-placer's 45deg walls) -- and the
/// rows run along x. A heading within `FIELD.snap` of x or y is snapped to it.
///
/// Returns `(x, y) -> radians in [0, pi)` (or `None`), gridded once; each query sums the cells
/// within 3 sigma. The closure owns its grid, so it does not borrow the topology.
pub fn heading_field(
    topo: &Topology,
    result: &Analysis,
    rot: &Rot3,
    cfg: &Config,
) -> impl Fn(f64, f64) -> Option<f64> + use<> {
    let (pos, area) = (&topo.pos, &topo.area);
    let off = &result.offset;
    let c = cfg.field.cell;
    let mut cells: IndexMap<String, [f64; 6]> = IndexMap::new();
    for f in 0..topo.n_faces {
        let o = f * 9;
        let (mut x, mut y) = (0.0f64, 0.0f64);
        for v in 0..3 {
            let p = o + v * 3;
            x += rot[0] * pos[p] + rot[3] * pos[p + 1] + rot[6] * pos[p + 2];
            y += rot[1] * pos[p] + rot[4] * pos[p + 1] + rot[7] * pos[p + 2];
        }
        x = x / 3.0 + off.x;
        y = y / 3.0 + off.y;
        let a = area[f];
        let k = format!(
            "{},{}",
            jsnum::to_js_string(jsnum::floor(x / c)),
            jsnum::to_js_string(jsnum::floor(y / c))
        );
        if let Some(c) = cells.get_mut(&k) {
            c[0] += a;
            c[1] += a * x;
            c[2] += a * y;
            c[3] += a * x * x;
            c[4] += a * x * y;
            c[5] += a * y * y;
        } else {
            cells.insert(k, [a, a * x, a * y, a * x * x, a * x * y, a * y * y]);
        }
    }
    let list: Vec<([f64; 6], f64, f64)> = cells
        .values()
        .map(|c| (*c, c[1] / c[0], c[2] / c[0]))
        .collect();
    let s = cfg.field.radius;
    let cut = 9.0 * s * s;
    let snap = cfg.field.snap * std::f64::consts::PI / 180.0;
    let min_ratio = cfg.field.min_ratio;
    move |qx: f64, qy: f64| {
        let (mut w, mut mx, mut my, mut xx, mut xy, mut yy) =
            (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (c, x, y) in &list {
            let d2 = jsnum::pow(x - qx, 2.0) + jsnum::pow(y - qy, 2.0);
            if d2 > cut {
                continue;
            }
            let g = jsnum::exp(-d2 / (2.0 * s * s));
            w += g * c[0];
            mx += g * c[1];
            my += g * c[2];
            xx += g * c[3];
            xy += g * c[4];
            yy += g * c[5];
        }
        if !(w > 0.0) {
            return None;
        }
        mx /= w;
        my /= w;
        let sxx = xx / w - mx * mx;
        let sxy = xy / w - mx * my;
        let syy = yy / w - my * my;
        // eigenvalues of [[sxx, sxy], [sxy, syy]]
        let tr = (sxx + syy) / 2.0;
        let dd = jsnum::sqrt(jsnum::pow((sxx - syy) / 2.0, 2.0) + sxy * sxy);
        let l1 = tr + dd;
        let l2 = jsnum::max(tr - dd, 1e-9);
        if jsnum::sqrt(l1 / l2) < min_ratio {
            return None;
        }
        let pi = std::f64::consts::PI;
        let mut th = 0.5 * jsnum::atan2(2.0 * sxy, sxx - syy);
        th = ((th % pi) + pi) % pi;
        for axis in [0.0, pi / 2.0, pi] {
            if jsnum::abs(th - axis) <= snap {
                th = axis % pi;
            }
        }
        Some(th)
    }
}
