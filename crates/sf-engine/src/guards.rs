//! Engine-internal guards (fix round 2D): defence in depth for values that pass the loader, such as a
//! "load anyway" part metres across, or one stray vertex at 1e16.
//!
//! * [`JsSort::js_sort_by`], a stable sort that cannot panic whatever the comparator says. The JS
//!   comparators here return 0 for a NaN difference, as V8's `sort` is happy with; that is not a total
//!   order, and Rust's `sort_by` (1.81+) may panic on one ("user-provided comparison function does not
//!   correctly implement a total order"). For any consistent comparator the result is the same as
//!   `slice::sort_by`'s (stable sorts agree), including ties such as `-0.0` / `+0.0` that the comparator
//!   calls equal, so parity holds for every finite input. With a NaN the order is some permutation.
//! * [`MAX_STEPS`] / [`too_many_steps`], the bound on a loop that steps a float across an extent.
//!   `v += 0.25` stops changing `v` past ~1e15, and even before that a 1e12 mm extent is 4e12 rows.

use std::cmp::Ordering;

/// The most steps any extent-driven float loop takes. A metre at 0.25 mm is 4,000; this is 25 m at that step.
pub const MAX_STEPS: f64 = 100_000.0;

/// How far from the origin (mm) a point can be and still be where a support wall is built: 100 m.
pub const REACH_MM: f64 = 1e5;

/// Is the point finite and within [`REACH_MM`] of the origin on every axis?
#[inline]
pub fn in_reach(p: &[f64; 3]) -> bool {
    p.iter().all(|c| c.abs() <= REACH_MM)
}

/// Would stepping `step` across `extent` take more than [`MAX_STEPS`] steps (or an unknown number: NaN)?
#[inline]
pub fn too_many_steps(extent: f64, step: f64) -> bool {
    let n = extent / step;
    n.is_nan() || n > MAX_STEPS
}

/// A total preorder on `f64` that agrees with the JS comparator `a - b` (negative first, positive last, zero equal)
/// on every non-NaN pair, `-0.0` and `+0.0` and equal infinities included (they tie, as `partial_cmp` has them;
/// `total_cmp` would split the zeros), and puts a NaN after everything, equal to itself. Unlike `a - b` read as
/// "NaN is equal" it IS a total order, so `slice::sort_by` (which panics on one that is not) can use it directly:
/// the fast path for a big slice, where [`JsSort`] is 3x slower.
#[inline]
pub fn cmp_f64(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b)
        .unwrap_or_else(|| a.is_nan().cmp(&b.is_nan()))
}

/// A stable sort that never panics (see the module docs).
pub trait JsSort<T> {
    /// Any `T`: sorts a permutation, then applies it.
    fn js_sort_by(&mut self, cmp: impl FnMut(&T, &T) -> Ordering);
}

impl<T> JsSort<T> for [T] {
    fn js_sort_by(&mut self, mut cmp: impl FnMut(&T, &T) -> Ordering) {
        let n = self.len();
        if n < 2 {
            return;
        }
        // insertion sort of runs of RUN, then bottom-up merges, all on a permutation (no `T: Clone`)
        const RUN: usize = 16;
        let mut idx: Vec<usize> = (0..n).collect();
        let mut s = 0;
        while s < n {
            let e = (s + RUN).min(n);
            for i in s + 1..e {
                let mut j = i;
                while j > s && cmp(&self[idx[j]], &self[idx[j - 1]]) == Ordering::Less {
                    idx.swap(j, j - 1);
                    j -= 1;
                }
            }
            s = e;
        }
        let mut buf = idx.clone();
        let mut width = RUN;
        while width < n {
            let mut lo = 0;
            while lo < n {
                let mid = (lo + width).min(n);
                let hi = (lo + 2 * width).min(n);
                let (mut l, mut r, mut k) = (lo, mid, lo);
                while l < mid && r < hi {
                    // take from the right only when strictly less: stable
                    if cmp(&self[idx[r]], &self[idx[l]]) == Ordering::Less {
                        buf[k] = idx[r];
                        r += 1;
                    } else {
                        buf[k] = idx[l];
                        l += 1;
                    }
                    k += 1;
                }
                while l < mid {
                    buf[k] = idx[l];
                    l += 1;
                    k += 1;
                }
                while r < hi {
                    buf[k] = idx[r];
                    r += 1;
                    k += 1;
                }
                lo = hi;
            }
            std::mem::swap(&mut idx, &mut buf);
            width *= 2;
        }
        // apply the permutation in place: position i takes the element that was at idx[i]
        for i in 0..n {
            let mut j = idx[i];
            while j < i {
                j = idx[j];
            }
            self.swap(i, j);
            // (cycle-following form: after the swap, record where the displaced element went)
            idx[i] = j;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a JS `sort` comparator returning `diff` means: zero or NaN is equal (the six `cmp_diff` copies).
    fn cmp_diff(diff: f64) -> Ordering {
        if diff < 0.0 {
            Ordering::Less
        } else if diff > 0.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut s = seed;
        move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        }
    }

    /// Random keys: small integers (many ties), signed zeros, infinities and, when `nan`, NaNs (20 %).
    fn keys(r: &mut impl FnMut() -> u64, n: usize, nan: bool) -> Vec<f64> {
        (0..n)
            .map(|_| match r() % 10 {
                0 | 1 if nan => f64::NAN,
                2 => 0.0,
                3 => -0.0,
                4 => f64::INFINITY,
                5 => f64::NEG_INFINITY,
                _ => (r() % 7) as f64 - 3.0,
            })
            .collect()
    }

    #[test]
    fn same_as_the_std_stable_sort_for_a_consistent_comparator() {
        let mut r = rng(0x9E3779B97F4A7C15);
        for n in (0..70).chain([100, 255, 256, 257, 1000, 4097]) {
            for _ in 0..20 {
                // (key, original index) pairs: the index shows stability, -0.0 / +0.0 tie
                let ks = keys(&mut r, n, false);
                let mut a: Vec<(f64, usize)> = ks.iter().copied().zip(0..).collect();
                let mut b = a.clone();
                a.sort_by(|p, q| cmp_diff(p.0 - q.0));
                b.js_sort_by(|p, q| cmp_diff(p.0 - q.0));
                let bits = |v: &[(f64, usize)]| -> Vec<(u64, usize)> {
                    v.iter().map(|&(k, i)| (k.to_bits(), i)).collect()
                };
                // (infinite ties make inf - inf NaN: equal, as the comparator means; same in both)
                assert_eq!(bits(&a), bits(&b), "n = {n}");
            }
        }
    }

    #[test]
    fn cannot_panic_on_nan_laden_input_of_21_or_more_elements() {
        let mut r = rng(0xD1B54A32D192ED03);
        for n in (21..80).chain([200, 1000, 5000]) {
            for _ in 0..40 {
                let ks = keys(&mut r, n, true);
                let mut v: Vec<(f64, usize)> = ks.iter().copied().zip(0..).collect();
                v.js_sort_by(|p, q| cmp_diff(p.0 - q.0));
                // still a permutation of the input
                let mut seen: Vec<usize> = v.iter().map(|p| p.1).collect();
                seen.sort_unstable();
                assert_eq!(seen, (0..n).collect::<Vec<_>>());
                // and the same for the other comparator shapes in the engine
                let mut w = ks.clone();
                w.js_sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                w.js_sort_by(|a, b| cmp_diff(b - a));
                assert_eq!(w.len(), n);
            }
        }
    }

    #[test]
    fn cmp_f64_is_cmp_diff_on_non_nan_and_total() {
        let vals = [
            f64::NEG_INFINITY,
            -2.0,
            -0.0,
            0.0,
            1e-320,
            3.0,
            f64::INFINITY,
            f64::INFINITY,
        ];
        for &a in &vals {
            for &b in &vals {
                assert_eq!(cmp_f64(a, b), cmp_diff(a - b), "{a} {b}");
            }
        }
        let mut r = rng(7);
        for _ in 0..50 {
            let n = 21 + (r() % 300) as usize;
            let mut v = keys(&mut r, n, true);
            v.sort_by(|a, b| cmp_f64(*a, *b)); // std's sort_by: must not panic
            let nans = v.iter().filter(|x| x.is_nan()).count();
            assert!(v[n - nans..].iter().all(|x| x.is_nan()));
        }
    }

    #[test]
    fn steps_bound() {
        assert!(!too_many_steps(1000.0, 0.25));
        assert!(!too_many_steps(0.0, 0.25));
        assert!(too_many_steps(1e16, 0.25));
        assert!(too_many_steps(f64::INFINITY, 0.25));
        assert!(too_many_steps(f64::NAN, 0.25));
    }
}
