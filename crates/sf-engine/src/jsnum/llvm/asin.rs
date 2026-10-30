//! Ports of LLVM libc `math::asin` and `math::acos` (src/__support/math/{asin,acos}.h) and
//! `asin_utils.h`, revision 9309c117, as V8 15.0.245 calls them from
//! `v8::base::ieee754::{asin,acos}` (src/base/ieee754.cc:122, 80). FMA, accurate pass.

use super::fputil::{
    DD, F128, exact_add, exact_mult, fma, nearest_integer, polyeval_f128, quick_add, quick_mul,
    quick_mult,
};
use super::tables_trig::{ASIN_COEFFS, ASIN_COEFFS_F128};

const PI: DD = DD::new(1.2246467991473532e-16, 3.141592653589793); // 0x1.1a62633145c07p-53, 0x1.921fb54442d18p1
const PI_OVER_TWO: DD = DD::new(6.123233995736766e-17, 1.5707963267948966); // 0x1.1a62633145c07p-54, 0x1.921fb54442d18p0

fn pi_over_two_f128() -> F128 {
    F128::new(false, -127, 0xc90fdaa2_2168c234_c4c6628b_80dc1cd1)
}

fn pi_f128() -> F128 {
    F128::new(false, -126, 0xc90fdaa2_2168c234_c4c6628b_80dc1cd1)
}

/// `asin_eval(const DoubleDouble &u, unsigned &idx, double &err)`
fn asin_eval(u: DD, idx: &mut usize, err: &mut f64) -> DD {
    let k = nearest_integer(u.hi * 32.0);
    *idx = k as u32 as usize;
    let c = &ASIN_COEFFS[*idx];
    let y_hi = fma(k, -0.03125, u.hi); // Exact
    let y = exact_add(y_hi, u.lo);
    let y2 = y.hi * y.hi;
    *err = fma(*err, y2, 1.9721522630525295e-31);
    let c0 = quick_mult(y, DD::new(c[3], c[2]));
    let c1 = fma(y.hi, c[5], c[4]);
    let c2 = fma(y.hi, c[7], c[6]);
    let c3 = fma(y.hi, c[9], c[8]);
    let c4 = fma(y.hi, c[11], c[10]);

    let y4 = y2 * y2;
    let d0 = fma(y2, c2, c1);
    let d1 = fma(y2, c4, c3);

    let mut r = exact_add(c[0], c0.hi);

    let e1 = fma(y4, d1, d0);

    r.lo = fma(y2, e1, c[1] + c0.lo + r.lo);

    r
}

/// `asin_eval(const Float128 &u, unsigned idx)`
fn asin_eval_f128(u: F128, idx: usize) -> F128 {
    polyeval_f128(u, &ASIN_COEFFS_F128[idx])
}

/// `asin(x)`
pub(crate) fn asin(x: f64) -> f64 {
    asin_impl::<0>(x)
}

/// `asin` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn asin_impl<const FORCE: u8>(x: f64) -> f64 {
    let bits = x.to_bits();
    let x_exp: i32 = ((bits >> 52) & 0x7ff) as i32;
    let x_is_neg = (bits >> 63) != 0;

    if x_exp < 1023 - 1 {
        if x_exp < 1023 - 26 {
            return fma(x, 5.551115123125783e-17, x);
        }

        let mut idx = 0usize;
        let x_sq = exact_mult(x, x);
        let mut err = f64::from_bits(bits & 0x7fff_ffff_ffff_ffff) * 4.440892098500626e-16;

        let p = asin_eval(x_sq, &mut idx, &mut err);
        let r0 = exact_mult(x, p.hi);
        let r_lo = fma(x, p.lo, r0.lo);

        let r_upper = r0.hi + (r_lo + err);
        let r_lower = r0.hi + (r_lo - err);

        if FORCE == 0 && r_upper == r_lower {
            return r_upper;
        }

        // Ziv's test failed, perform 128-bit calculation.
        idx = nearest_integer(x_sq.hi * 64.0) as u32 as usize;

        let x_f128 = F128::from_f64(x);

        let u_hi = F128::from_f64(fma(idx as f64, -0.015625, x_sq.hi));
        let u = quick_add(u_hi, F128::from_f64(x_sq.lo));

        let p_f128 = asin_eval_f128(u, idx);
        let r = quick_mul(x_f128, p_f128);

        return r.to_f64();
    }

    let x_abs = f64::from_bits(bits & 0x7fff_ffff_ffff_ffff);

    const SIGN: [f64; 2] = [1.0, -1.0];
    let x_sign = SIGN[x_is_neg as usize];

    if x_exp >= 1023 {
        if x_abs == 1.0 {
            return fma(x_sign, PI_OVER_TWO.hi, x_sign * PI_OVER_TWO.lo);
        }
        if x.is_nan() {
            // quiet NaN is returned as is; a signaling NaN becomes a quiet NaN
            return f64::NAN;
        }
        return f64::NAN;
    }

    // |x| < 1
    let u = fma(x_abs, -0.5, 0.5);
    let v_hi = u.sqrt();

    let h = fma(v_hi, -v_hi, u);

    let vh = v_hi * 2.0;
    let vl = h / v_hi;

    let mut idx = 0usize;
    let mut err = vh * 4.440892098500626e-16;

    let p = asin_eval(DD::new(0.0, u), &mut idx, &mut err);

    let r0 = quick_mult(DD::new(vl, vh), p);
    let r = exact_add(PI_OVER_TWO.hi, -r0.hi);

    let r_lo = PI_OVER_TWO.lo - r0.lo + r.lo;

    let r_upper = fma(r.hi, x_sign, fma(r_lo, x_sign, err));
    let r_lower = fma(r.hi, x_sign, fma(r_lo, x_sign, -err));

    if FORCE == 0 && r_upper == r_lower {
        return r_upper;
    }

    // Ziv's test failed, perform 128-bit calculation.
    idx = nearest_integer(u * 64.0) as u32 as usize;

    let vl_lo = fma(-v_hi, vl, h) / v_hi;
    let t = h * (-0.25) / u;
    let vll = fma(vl, t, vl_lo);
    // m_v = -(vh + vl + vll)
    let mut m_v = quick_add(
        F128::from_f64(vh),
        quick_add(F128::from_f64(vl), F128::from_f64(vll)),
    );
    m_v.neg = true;

    let y_f128 = F128::from_f64(fma(idx as f64, -0.015625, u));

    let p_f128 = asin_eval_f128(y_f128, idx);
    let r0_f128 = quick_mul(m_v, p_f128);
    let mut r_f128 = quick_add(pi_over_two_f128(), r0_f128);

    if x_is_neg {
        r_f128.neg = true;
    }

    r_f128.to_f64()
}

/// `acos(x)`
pub(crate) fn acos(x: f64) -> f64 {
    acos_impl::<0>(x)
}

/// `acos` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn acos_impl<const FORCE: u8>(x: f64) -> f64 {
    let bits = x.to_bits();
    let x_exp: i32 = ((bits >> 52) & 0x7ff) as i32;
    let x_is_neg = (bits >> 63) != 0;

    if x_exp < 1023 - 1 {
        if x_exp < 1023 - 55 {
            return (f64::from_bits(bits & 0x7fff_ffff_ffff_ffff) + 6.842277657836021e-49)
                + PI_OVER_TWO.hi;
        }

        let mut idx = 0usize;
        let x_sq = exact_mult(x, x);
        let mut err = f64::from_bits(bits & 0x7fff_ffff_ffff_ffff) * 4.440892098500626e-16;
        let p = asin_eval(x_sq, &mut idx, &mut err);
        let r0 = exact_mult(x, p.hi);
        let r_hi = fma(-x, p.hi, PI_OVER_TWO.hi);
        let mut r_lo = ((PI_OVER_TWO.hi - r_hi) - r0.hi) - r0.lo;
        r_lo = fma(-x, p.lo, r_lo + PI_OVER_TWO.lo);

        let r_upper = r_hi + (r_lo + err);
        let r_lower = r_hi + (r_lo - err);

        if FORCE == 0 && r_upper == r_lower {
            return r_upper;
        }

        // Ziv's test failed, perform 128-bit calculation.
        idx = nearest_integer(x_sq.hi * 64.0) as u32 as usize;

        let mut x_f128 = F128::from_f64(x);

        let u_hi = F128::from_f64(fma(idx as f64, -0.015625, x_sq.hi));
        let u = quick_add(u_hi, F128::from_f64(x_sq.lo));

        let p_f128 = asin_eval_f128(u, idx);
        x_f128.neg = !x_f128.neg;
        let r = quick_add(pi_over_two_f128(), quick_mul(x_f128, p_f128));

        return r.to_f64();
    }

    let x_abs = f64::from_bits(bits & 0x7fff_ffff_ffff_ffff);

    const SIGN: [f64; 2] = [1.0, -1.0];
    let x_sign = SIGN[x_is_neg as usize];
    if x_exp >= 1023 {
        if x_abs == 1.0 {
            return if x == 1.0 {
                0.0
            } else {
                fma(-x_sign, PI.hi, PI.lo)
            };
        }
        if x.is_nan() {
            return f64::NAN;
        }
        return f64::NAN;
    }

    // |x| < 1
    let u = fma(x_abs, -0.5, 0.5);
    let v_hi = u.sqrt();

    let h = fma(v_hi, -v_hi, u);

    let vh = v_hi * 2.0;
    let vl = h / v_hi;

    let mut idx = 0usize;
    let mut err = vh * 4.440892098500626e-16;

    let p = asin_eval(DD::new(0.0, u), &mut idx, &mut err);

    let r0 = quick_mult(DD::new(vl, vh), p);

    let (r_hi, r_lo);
    if !x_is_neg {
        r_hi = r0.hi;
        r_lo = r0.lo;
    } else {
        let r = exact_add(PI.hi, -r0.hi);
        r_hi = r.hi;
        r_lo = (PI.lo - r0.lo) + r.lo;
    }

    let r_upper = r_hi + (r_lo + err);
    let r_lower = r_hi + (r_lo - err);

    if FORCE == 0 && r_upper == r_lower {
        return r_upper;
    }

    // Ziv's test failed, perform 128-bit calculation.
    idx = nearest_integer(u * 64.0) as u32 as usize;

    let vl_lo = fma(-v_hi, vl, h) / v_hi;
    let t = h * (-0.25) / u;
    let vll = fma(vl, t, vl_lo);
    let mut m_v = quick_add(
        F128::from_f64(vh),
        quick_add(F128::from_f64(vl), F128::from_f64(vll)),
    );
    m_v.neg = x_is_neg;

    let y_f128 = F128::from_f64(fma(idx as f64, -0.015625, u));

    let p_f128 = asin_eval_f128(y_f128, idx);
    let mut r_f128 = quick_mul(m_v, p_f128);

    if x_is_neg {
        r_f128 = quick_add(pi_f128(), r_f128);
    }

    r_f128.to_f64()
}
