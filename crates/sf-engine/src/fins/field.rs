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
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(w > 0)` is true for NaN, as in JS
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fins::seating::tests::topo_of;
    use crate::overhangs::Analysis;
    use crate::types::{IDENTITY3, Xyz};

    fn res() -> Analysis {
        Analysis {
            over: vec![],
            kept: vec![],
            on_bed: vec![],
            regions: vec![],
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: 0.0,
            over_face_count: 0,
            offset: Xyz::default(),
            size: Xyz::default(),
        }
    }

    /// A flat rectangle `[x0,x1] x [y0,y1]` at z = 0 as `n x n` sub-rectangles (two faces each), so
    /// the plan grid has many cells.
    fn rect(x0: f64, x1: f64, y0: f64, y1: f64, n: usize) -> Vec<f64> {
        let mut v = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let (a, b) = (
                    x0 + (x1 - x0) * i as f64 / n as f64,
                    x0 + (x1 - x0) * (i + 1) as f64 / n as f64,
                );
                let (c, d) = (
                    y0 + (y1 - y0) * j as f64 / n as f64,
                    y0 + (y1 - y0) * (j + 1) as f64 / n as f64,
                );
                v.extend([
                    a, c, 0.0, b, c, 0.0, b, d, 0.0, a, c, 0.0, b, d, 0.0, a, d, 0.0,
                ]);
            }
        }
        v
    }

    #[test]
    fn a_long_part_has_a_heading_along_its_length() {
        let cfg = Config::default();
        let along_x = topo_of(rect(0.0, 60.0, 0.0, 10.0, 12));
        let f = heading_field(&along_x, &res(), &IDENTITY3, &cfg);
        assert_eq!(f(30.0, 5.0), Some(0.0)); // snapped to x
        let along_y = topo_of(rect(0.0, 10.0, 0.0, 60.0, 12));
        let f = heading_field(&along_y, &res(), &IDENTITY3, &cfg);
        let th = f(5.0, 30.0).unwrap();
        assert!(
            (th - std::f64::consts::FRAC_PI_2).abs() < 1e-12,
            "snapped to y: {th}"
        );
    }

    #[test]
    fn a_diagonal_part_keeps_its_heading_in_0_to_pi() {
        let cfg = Config::default();
        // 45 degrees: rotate the strip with the pose
        let strip = topo_of(rect(0.0, 60.0, 0.0, 10.0, 12));
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let rot = [c, c, 0.0, -c, c, 0.0, 0.0, 0.0, 1.0];
        let f = heading_field(&strip, &res(), &rot, &cfg);
        let th = f(21.0, 21.0).unwrap();
        assert!((th - std::f64::consts::FRAC_PI_4).abs() < 0.05, "{th}");
        assert!((0.0..std::f64::consts::PI).contains(&th));
    }

    #[test]
    fn a_squarish_or_distant_or_empty_part_has_no_heading() {
        let cfg = Config::default();
        let sq = topo_of(rect(0.0, 20.0, 0.0, 20.0, 10));
        assert_eq!(
            heading_field(&sq, &res(), &IDENTITY3, &cfg)(10.0, 10.0),
            None
        );
        let strip = topo_of(rect(0.0, 60.0, 0.0, 10.0, 12));
        let f = heading_field(&strip, &res(), &IDENTITY3, &cfg);
        assert_eq!(f(1e4, 1e4), None); // beyond 3 sigma: no weight
        let none = topo_of(vec![]);
        assert_eq!(
            heading_field(&none, &res(), &IDENTITY3, &cfg)(0.0, 0.0),
            None
        );
    }
}
