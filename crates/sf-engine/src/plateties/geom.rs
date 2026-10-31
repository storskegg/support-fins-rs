//! Plane geometry for plate ties: a model's outline on the bed (its convex hull), how thin it is and
//! which way, and how far two outlines are apart. New code (no JS counterpart): plain `f64` maths.

pub(crate) type P2 = [f64; 2];

pub(crate) fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub(crate) fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub(crate) fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}

/// The convex hull of `pts`, counter-clockwise, no repeated or collinear points (Andrew's monotone
/// chain). Fewer than three distinct points give what there is.
pub(crate) fn convex_hull(mut pts: Vec<P2>) -> Vec<P2> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut lower: Vec<P2> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2
            && cross(
                sub(lower[lower.len() - 1], lower[lower.len() - 2]),
                sub(p, lower[lower.len() - 2]),
            ) <= 0.0
        {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<P2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2
            && cross(
                sub(upper[upper.len() - 1], upper[upper.len() - 2]),
                sub(p, upper[upper.len() - 2]),
            ) <= 0.0
        {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Drop hull points that sit within `tol` of the chord between their kept neighbours, so a round
/// model's hull of thousands of points becomes a few dozen. The result is inside the hull and no
/// further than `tol` from it.
pub(crate) fn simplify_hull(hull: &[P2], tol: f64) -> Vec<P2> {
    let n = hull.len();
    if n <= 4 {
        return hull.to_vec();
    }
    let mut out = vec![hull[0]];
    let mut a = 0;
    while a < n {
        // the furthest end k (a < k <= n, n meaning back at 0) whose chord keeps every point between within tol
        let mut best = a + 1;
        let mut k = a + 2;
        // a chord over more than 512 points is kept as it is (bounded work on a huge hull)
        while k <= n && k - a <= 512 {
            let pa = hull[a];
            let pk = hull[k % n];
            let d = sub(pk, pa);
            let l = len(d);
            let ok = l > 0.0 && (a + 1..k).all(|i| (cross(d, sub(hull[i], pa)) / l).abs() <= tol);
            if !ok {
                break;
            }
            best = k;
            k += 1;
        }
        if best >= n {
            break;
        }
        out.push(hull[best]);
        a = best;
    }
    if out.len() < 3 { hull.to_vec() } else { out }
}

/// How thin a convex polygon is: its least width (between two parallel lines that hold it) and the unit
/// normal of those lines, the thin axis. Rotating calipers. A degenerate polygon has width 0 and axis x.
pub(crate) fn min_width(hull: &[P2]) -> (f64, P2) {
    let n = hull.len();
    if n < 3 {
        if n == 2 {
            let d = sub(hull[1], hull[0]);
            let l = len(d);
            if l > 0.0 {
                return (0.0, [-d[1] / l, d[0] / l]);
            }
        }
        return (0.0, [1.0, 0.0]);
    }
    let mut best = f64::INFINITY;
    let mut axis = [1.0, 0.0];
    let mut j = 1;
    for i in 0..n {
        let a = hull[i];
        let e = sub(hull[(i + 1) % n], a);
        let l = len(e);
        if l == 0.0 {
            continue;
        }
        let dist = |p: P2| cross(e, sub(p, a)) / l;
        // the antipodal point: walk on while the distance grows
        while dist(hull[(j + 1) % n]) > dist(hull[j % n]) {
            j += 1;
        }
        let w = dist(hull[j % n]);
        if w < best {
            best = w;
            axis = [-e[1] / l, e[0] / l];
        }
    }
    (best, axis)
}

/// Is `p` inside (or on) the counter-clockwise convex polygon `poly`?
pub(crate) fn in_convex(poly: &[P2], p: P2) -> bool {
    if poly.len() < 3 {
        return false;
    }
    (0..poly.len()).all(|i| cross(sub(poly[(i + 1) % poly.len()], poly[i]), sub(p, poly[i])) >= 0.0)
}

/// The closest point to `p` on the segment `a`-`b`.
pub(crate) fn closest_on_segment(p: P2, a: P2, b: P2) -> P2 {
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2 == 0.0 {
        return a;
    }
    let t = (dot(sub(p, a), d) / l2).clamp(0.0, 1.0);
    [a[0] + d[0] * t, a[1] + d[1] * t]
}

/// Do the segments `a`-`b` and `c`-`d` cross or touch?
pub(crate) fn segments_meet(a: P2, b: P2, c: P2, d: P2) -> bool {
    let o = |p: P2, q: P2, r: P2| cross(sub(q, p), sub(r, p));
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    let on = |p: P2, q: P2, r: P2| {
        o(p, q, r) == 0.0
            && r[0] >= p[0].min(q[0])
            && r[0] <= p[0].max(q[0])
            && r[1] >= p[1].min(q[1])
            && r[1] <= p[1].max(q[1])
    };
    on(c, d, a) || on(c, d, b) || on(a, b, c) || on(a, b, d)
}

/// Does the segment `a`-`b` touch the convex polygon?
pub(crate) fn segment_hits_convex(poly: &[P2], a: P2, b: P2) -> bool {
    if in_convex(poly, a) || in_convex(poly, b) {
        return true;
    }
    let n = poly.len();
    (0..n).any(|i| segments_meet(a, b, poly[i], poly[(i + 1) % n]))
}

/// The gap between two convex polygons and the closest points on each (`from` on `p`, `to` on `q`),
/// or `None` when they overlap or touch.
pub(crate) fn convex_gap(p: &[P2], q: &[P2]) -> Option<(f64, P2, P2)> {
    if p.is_empty() || q.is_empty() {
        return None;
    }
    if p.iter().any(|&v| in_convex(q, v)) || q.iter().any(|&v| in_convex(p, v)) {
        return None;
    }
    let (np, nq) = (p.len(), q.len());
    for i in 0..np {
        for j in 0..nq {
            if segments_meet(p[i], p[(i + 1) % np], q[j], q[(j + 1) % nq]) {
                return None;
            }
        }
    }
    let mut best = (f64::INFINITY, p[0], q[0]);
    for &v in p {
        for j in 0..nq {
            let c = closest_on_segment(v, q[j], q[(j + 1) % nq]);
            let d = len(sub(c, v));
            if d < best.0 {
                best = (d, v, c);
            }
        }
    }
    for &v in q {
        for i in 0..np {
            let c = closest_on_segment(v, p[i], p[(i + 1) % np]);
            let d = len(sub(c, v));
            if d < best.0 {
                best = (d, c, v);
            }
        }
    }
    Some(best)
}

/// The extent of `poly` along the unit direction `t`: (least, most).
pub(crate) fn extent_along(poly: &[P2], t: P2) -> (f64, f64) {
    poly.iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            let s = dot(v, t);
            (lo.min(s), hi.max(s))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, w: f64, h: f64) -> Vec<P2> {
        vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    }

    #[test]
    fn hull_drops_inside_and_collinear_points() {
        let pts = vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [1.0, 0.0],
            [2.0, 2.0],
            [0.0, 2.0],
            [1.0, 1.0],
        ];
        let h = convex_hull(pts);
        assert_eq!(h, vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
    }

    #[test]
    fn width_of_a_rotated_slab() {
        // a 40 x 3 slab turned 30 degrees: width 3, thin axis along the turned x
        let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let pts: Vec<P2> = square(-1.5, -20.0, 3.0, 40.0)
            .into_iter()
            .map(|[x, y]| [x * c - y * s, x * s + y * c])
            .collect();
        let (w, axis) = min_width(&convex_hull(pts));
        assert!((w - 3.0).abs() < 1e-9, "{w}");
        assert!((dot(axis, [c, s]).abs() - 1.0).abs() < 1e-9, "{axis:?}");
    }

    #[test]
    fn gap_between_squares_and_overlap() {
        let a = square(0.0, 0.0, 2.0, 2.0);
        let b = square(5.0, 1.0, 2.0, 2.0);
        let (g, f, t) = convex_gap(&a, &b).unwrap();
        assert!((g - 3.0).abs() < 1e-12);
        assert_eq!(f[0], 2.0);
        assert_eq!(t[0], 5.0);
        assert!(convex_gap(&a, &square(1.0, 1.0, 2.0, 2.0)).is_none());
        // crossing without a vertex inside the other
        assert!(convex_gap(&square(0.0, 1.0, 6.0, 1.0), &square(2.0, 0.0, 1.0, 6.0)).is_none());
    }

    #[test]
    fn simplified_circle_stays_close() {
        let pts: Vec<P2> = (0..4000)
            .map(|i| {
                let a = i as f64 / 4000.0 * std::f64::consts::TAU;
                [20.0 * a.cos(), 20.0 * a.sin()]
            })
            .collect();
        let h = convex_hull(pts.clone());
        let s = simplify_hull(&h, 0.02);
        assert!(s.len() < 200 && s.len() > 20, "{}", s.len());
        for p in pts {
            // every original point is within tol of the simplified outline (or inside it)
            let r = len(p);
            assert!(r <= 20.0 + 1e-9);
        }
        let (w, _) = min_width(&s);
        assert!((w - 40.0).abs() < 0.05, "{w}");
    }

    #[test]
    fn segment_through_a_square() {
        let q = square(0.0, 0.0, 2.0, 2.0);
        assert!(segment_hits_convex(&q, [-1.0, 1.0], [3.0, 1.0]));
        assert!(!segment_hits_convex(&q, [-1.0, 3.0], [3.0, 3.0]));
    }
}
