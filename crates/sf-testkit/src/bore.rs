//! Port of `tests/_bore.js`.

use sf_engine::jsnum;
use std::f64::consts::PI;

/// Options of `circularBore` (tests/_bore.js:3); `Default` is the JS defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct CircularBore {
    pub diameter: f64,
    pub length: f64,
    pub segments: usize,
    pub half: f64,
    pub axial_segments: usize,
}

impl Default for CircularBore {
    fn default() -> Self {
        CircularBore {
            diameter: 5.0,
            length: 30.0,
            segments: 96,
            half: 10.0,
            axial_segments: 1,
        }
    }
}

/// Port of `circularBore` (tests/_bore.js:3-26): a procedural closed block with a circular
/// through-bore along X. Returns a `Float64Array` in JS, so nothing is rounded.
pub fn circular_bore(o: &CircularBore) -> Vec<f64> {
    let CircularBore {
        diameter,
        length,
        segments,
        half,
        axial_segments,
    } = *o;
    let seg = segments as f64;
    let axial = axial_segments as f64;
    let mut triangles: Vec<f64> = Vec::new();
    let mut quad = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| {
        for v in [a, b, c, a, c, d] {
            triangles.extend(v);
        }
    };
    let ring = |x: f64, outer: bool| -> Vec<[f64; 3]> {
        (0..segments)
            .map(|i| {
                let a = i as f64 * 2.0 * PI / seg;
                let c = jsnum::cos(a);
                let s = jsnum::sin(a);
                let r = if outer {
                    half / jsnum::max(jsnum::abs(c), jsnum::abs(s))
                } else {
                    diameter / 2.0
                };
                [x, r * c, half + r * s]
            })
            .collect()
    };
    let lo = ring(-length / 2.0, true);
    let li = ring(-length / 2.0, false);
    let ro = ring(length / 2.0, true);
    let ri = ring(length / 2.0, false);
    for i in 0..segments {
        let j = (i + 1) % segments;
        for k in 0..axial_segments {
            let x0 = -length / 2.0 + length * k as f64 / axial;
            let x1 = -length / 2.0 + length * (k + 1) as f64 / axial;
            let at = |v: [f64; 3], x: f64| [x, v[1], v[2]];
            quad(at(lo[i], x0), at(lo[j], x0), at(lo[j], x1), at(lo[i], x1));
            quad(at(li[i], x0), at(li[i], x1), at(li[j], x1), at(li[j], x0));
        }
        quad(li[i], li[j], lo[j], lo[i]);
        quad(ri[i], ro[i], ro[j], ri[j]);
    }
    triangles
}
