//! Does a triangle come within a margin of one piece of a tie? A piece is a convex hexahedron (two
//! rectangular sections and the planar faces between them); the test is the separating-axis theorem
//! over the piece's face normals, the triangle's normal and the cross products of their edges, with the
//! margin added on every axis.

pub(crate) type P3 = [f64; 3];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: P3) -> Option<P3> {
    let l = dot(a, a).sqrt();
    (l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// One piece of a tie: section `a` and section `b`, each four corners in the same order.
pub(crate) struct Piece {
    pub v: [P3; 8],
    axes: Vec<P3>,
    edges: Vec<P3>,
    /// x, y, z box
    pub lo: P3,
    pub hi: P3,
}

impl Piece {
    pub fn new(a: [P3; 4], b: [P3; 4]) -> Piece {
        let v = [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]];
        let mut edges = Vec::new();
        for k in 0..4 {
            edges.push(sub(a[(k + 1) % 4], a[k]));
            edges.push(sub(b[(k + 1) % 4], b[k]));
            edges.push(sub(b[k], a[k]));
        }
        let edges: Vec<P3> = edges.into_iter().filter_map(norm).collect();
        let mut axes = Vec::new();
        // the two section planes and the four sides
        axes.extend(norm(cross(sub(a[1], a[0]), sub(a[3], a[0]))));
        axes.extend(norm(cross(sub(b[1], b[0]), sub(b[3], b[0]))));
        for k in 0..4 {
            let j = (k + 1) % 4;
            axes.extend(norm(cross(sub(a[j], a[k]), sub(b[k], a[k]))));
            axes.extend(norm(cross(sub(b[j], b[k]), sub(b[k], a[k]))));
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &v {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Piece {
            v,
            axes,
            edges,
            lo,
            hi,
        }
    }

    /// Is the triangle within `margin` of the piece (no separating axis with a gap over `margin`)?
    pub fn near(&self, t: &[P3; 3], margin: f64) -> bool {
        for k in 0..3 {
            let (tl, th) = (
                t[0][k].min(t[1][k]).min(t[2][k]),
                t[0][k].max(t[1][k]).max(t[2][k]),
            );
            if th + margin < self.lo[k] || tl - margin > self.hi[k] {
                return false;
            }
        }
        let separated = |axis: P3| {
            let (mut hl, mut hh) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in &self.v {
                let s = dot(*p, axis);
                hl = hl.min(s);
                hh = hh.max(s);
            }
            let (mut tl, mut th) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in t {
                let s = dot(*p, axis);
                tl = tl.min(s);
                th = th.max(s);
            }
            hh + margin < tl || th + margin < hl
        };
        if self.axes.iter().any(|&a| separated(a)) {
            return false;
        }
        let te = [sub(t[1], t[0]), sub(t[2], t[1]), sub(t[0], t[2])];
        if let Some(n) = norm(cross(te[0], te[1]))
            && separated(n)
        {
            return false;
        }
        for e in te {
            for &h in &self.edges {
                if let Some(a) = norm(cross(e, h))
                    && separated(a)
                {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> Piece {
        let ring = |x: f64| [[x, 0.0, 0.0], [x, 1.0, 0.0], [x, 1.0, 1.0], [x, 0.0, 1.0]];
        Piece::new(ring(0.0), ring(1.0))
    }

    #[test]
    fn triangle_inside_touching_and_apart() {
        let b = unit_box();
        let inside = [[0.2, 0.2, 0.5], [0.8, 0.2, 0.5], [0.5, 0.8, 0.5]];
        assert!(b.near(&inside, 0.0));
        let apart = [[2.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        assert!(!b.near(&apart, 0.5));
        assert!(b.near(&apart, 1.01));
        // a big triangle passing through the box with no vertex inside it
        let through = [[-5.0, -5.0, 0.5], [5.0, -5.0, 0.5], [0.0, 10.0, 0.5]];
        assert!(b.near(&through, 0.0));
        // a triangle across a box corner: the boxes overlap, its own normal separates them by 0.2/sqrt(3)
        let corner = [[1.6, 1.6, 0.0], [1.6, 0.0, 1.6], [0.0, 1.6, 1.6]];
        assert!(!b.near(&corner, 0.1));
        assert!(b.near(&corner, 0.2));
    }
}
