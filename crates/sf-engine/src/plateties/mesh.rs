//! A model's triangles on the plate, bucketed by height so a horizontal slice or a query for one
//! tie's height touches only the triangles that reach it.

use super::geom::P2;

pub(crate) type Tri = [[f64; 3]; 3];

/// A triangle soup with a height index.
pub(crate) struct Mesh {
    pub tris: Vec<Tri>,
    /// per triangle: its least and most z
    zr: Vec<[f64; 2]>,
    /// per triangle: its x, y box (min x, min y, max x, max y)
    xy: Vec<[f64; 4]>,
    /// triangles by bucket of `step` mm from `z0`
    buckets: Vec<Vec<u32>>,
    z0: f64,
    step: f64,
    pub bbox: [f64; 6],
}

/// At most this many height buckets per model.
const MAX_BUCKETS: f64 = 4096.0;

impl Mesh {
    /// The mesh of a flat soup, nine numbers per triangle. Non-finite triangles are left out.
    pub fn new(flat: &[f64]) -> Mesh {
        let tris: Vec<Tri> = flat
            .as_chunks::<9>()
            .0
            .iter()
            .filter(|t| t.iter().all(|v| v.is_finite()))
            .map(|t| [[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]])
            .collect();
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut zr = Vec::with_capacity(tris.len());
        let mut xy = Vec::with_capacity(tris.len());
        for t in &tris {
            let mut b = [
                f64::INFINITY,
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in t {
                for k in 0..3 {
                    b[k] = b[k].min(p[k]);
                    b[k + 3] = b[k + 3].max(p[k]);
                }
            }
            for k in 0..3 {
                bbox[k] = bbox[k].min(b[k]);
                bbox[k + 3] = bbox[k + 3].max(b[k + 3]);
            }
            zr.push([b[2], b[5]]);
            xy.push([b[0], b[1], b[3], b[4]]);
        }
        let (z0, step, n) = if tris.is_empty() {
            (0.0, 1.0, 0)
        } else {
            let h = bbox[5] - bbox[2];
            let step = (h / MAX_BUCKETS).max(0.5);
            (bbox[2], step, (h / step).floor() as usize + 1)
        };
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (i, r) in zr.iter().enumerate() {
            let (a, b) = (
                ((r[0] - z0) / step).floor() as usize,
                ((r[1] - z0) / step).floor() as usize,
            );
            for bk in a..=b.min(n.saturating_sub(1)) {
                buckets[bk].push(i as u32);
            }
        }
        Mesh {
            tris,
            zr,
            xy,
            buckets,
            z0,
            step,
            bbox,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    fn bucket(&self, z: f64) -> Option<usize> {
        if self.buckets.is_empty() || z < self.bbox[2] || z > self.bbox[5] {
            return None;
        }
        Some((((z - self.z0) / self.step).floor() as usize).min(self.buckets.len() - 1))
    }

    /// The cross-section at height `z`: one segment per triangle that spans it. A vertex exactly at `z`
    /// counts as above, so a closed mesh gives closed loops.
    pub fn slice(&self, z: f64) -> Vec<[P2; 2]> {
        let mut out = Vec::new();
        let Some(b) = self.bucket(z) else {
            return out;
        };
        for &i in &self.buckets[b] {
            let r = self.zr[i as usize];
            if z < r[0] || z > r[1] {
                continue;
            }
            let t = &self.tris[i as usize];
            let mut pts: Vec<P2> = Vec::with_capacity(2);
            for k in 0..3 {
                let (p, q) = (t[k], t[(k + 1) % 3]);
                let (pa, qa) = (p[2] >= z, q[2] >= z);
                if pa != qa {
                    let f = (z - p[2]) / (q[2] - p[2]);
                    pts.push([p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f]);
                }
            }
            if let [a, b] = pts[..] {
                out.push([a, b]);
            }
        }
        out
    }

    /// Every triangle whose height range meets [`lo`, `hi`] and whose x, y box meets `xy` (min x, min y,
    /// max x, max y), each once.
    pub fn each_near(
        &self,
        lo: f64,
        hi: f64,
        xy: [f64; 4],
        mut f: impl FnMut(&Tri) -> bool,
    ) -> bool {
        if self.buckets.is_empty() || hi < self.bbox[2] || lo > self.bbox[5] {
            return false;
        }
        if xy[2] < self.bbox[0]
            || xy[0] > self.bbox[3]
            || xy[3] < self.bbox[1]
            || xy[1] > self.bbox[4]
        {
            return false;
        }
        let first = (((lo.max(self.bbox[2]) - self.z0) / self.step).floor() as usize)
            .min(self.buckets.len() - 1);
        let last = (((hi.min(self.bbox[5]) - self.z0) / self.step).floor() as usize)
            .min(self.buckets.len() - 1);
        for b in first..=last {
            for &i in &self.buckets[b] {
                let r = self.zr[i as usize];
                // seen once: in the first bucket of the query it shares
                let own = (((r[0] - self.z0) / self.step).floor() as usize).max(first);
                if own != b || r[1] < lo || r[0] > hi {
                    continue;
                }
                let bx = self.xy[i as usize];
                if bx[2] < xy[0] || bx[0] > xy[2] || bx[3] < xy[1] || bx[1] > xy[3] {
                    continue;
                }
                if f(&self.tris[i as usize]) {
                    return true;
                }
            }
        }
        false
    }
}

/// Where the line `o + u d` (2D) crosses the cross-section's segments: the `u` of each crossing.
pub(crate) fn ray_hits(section: &[[P2; 2]], o: P2, d: P2) -> Vec<f64> {
    let mut out = Vec::new();
    for [a, b] in section {
        let e = [b[0] - a[0], b[1] - a[1]];
        let den = d[0] * e[1] - d[1] * e[0];
        if den == 0.0 {
            continue;
        }
        let w = [a[0] - o[0], a[1] - o[1]];
        // o + u d = a + v e
        let u = (w[0] * e[1] - w[1] * e[0]) / den;
        let v = (w[0] * d[1] - w[1] * d[0]) / den;
        if (0.0..=1.0).contains(&v) {
            out.push(u);
        }
    }
    out
}

/// Is `p` inside the cross-section (an odd number of its segments cross the ray from `p` along +x)?
pub(crate) fn inside_section(section: &[[P2; 2]], p: P2) -> bool {
    let mut odd = false;
    for [a, b] in section {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if x > p[0] {
                odd = !odd;
            }
        }
    }
    odd
}
