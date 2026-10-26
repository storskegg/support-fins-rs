//! Port of `tests/_circular.js`.

use sf_engine::jsnum;
use std::f64::consts::PI;

/// Options of `recessedBody` (tests/_circular.js:3-4); `Default` is the JS defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct RecessedBody {
    pub segments: usize,
    pub height: f64,
    pub opening: f64,
    pub bore: f64,
    pub depth: f64,
    pub outer: f64,
    pub yaw: f64,
    pub translate: [f64; 3],
}

impl Default for RecessedBody {
    fn default() -> Self {
        RecessedBody {
            segments: 120,
            height: 40.0,
            opening: 25.0,
            bore: 15.0,
            depth: 10.0,
            outer: 35.0,
            yaw: 0.0,
            translate: [0.0, 0.0, 0.0],
        }
    }
}

/// Port of `recessedBody` (tests/_circular.js:3-17): a closed turned body with a conical
/// recess on its underside. Returns a `Float64Array` in JS, so nothing is rounded.
pub fn recessed_body(o: &RecessedBody) -> Vec<f64> {
    let RecessedBody {
        segments,
        height,
        opening,
        bore,
        depth,
        outer,
        yaw,
        translate,
    } = *o;
    let seg = segments as f64;
    let profile = [
        [outer, 0.0],
        [outer, height],
        [bore, height],
        [bore, depth],
        [opening, 0.0],
    ];
    let mut out: Vec<f64> = Vec::new();
    let point = |[r, z]: [f64; 2], i: usize| -> [f64; 3] {
        let a = i as f64 * 2.0 * PI / seg + yaw * PI / 180.0;
        [
            r * jsnum::cos(a) + translate[0],
            r * jsnum::sin(a) + translate[1],
            z + translate[2],
        ]
    };
    for i in 0..segments {
        for j in 0..profile.len() {
            let p = profile[j];
            let q = profile[(j + 1) % profile.len()];
            let a = point(p, i);
            let b = point(p, i + 1);
            let c = point(q, i + 1);
            let d = point(q, i);
            for v in [a, b, c, a, c, d] {
                out.extend(v);
            }
        }
    }
    out
}
