//! Ports of LLVM libc `math::sin`, `math::cos` and `math::tan` (src/__support/math/{sin,cos,tan}.h,
//! revision 9309c117) with their shared range reduction (range_reduction_double_{common,fma}.h)
//! and `sincos_eval.h`. V8 15.0.245 reaches `sin` and `cos` through
//! `v8::base::ieee754::{fdlibm_sin,fdlibm_cos}` (src/base/ieee754.cc:236-238, 168-170) and `tan` through
//! `ieee754::tan` (ieee754.cc:243). FMA, accurate pass, full tables.

use super::fputil::{
    DD, F128, dd_div, exact_add, exact_add_slow, exact_mult, fma, nearest_integer, polyeval_f128,
    quick_add, quick_mul, quick_mult,
};
use super::tables_sincos::{
    ONE_TWENTY_EIGHT_OVER_PI, SIN_K_PI_OVER_128, SIN_K_PI_OVER_128_F128, TAN_COEFFS,
};

const FAST_PASS_EXPONENT: u16 = 16;
const EXP_BIAS: u16 = 1023;

fn pi_over_128_f128() -> F128 {
    F128::new(false, -133, 0xc90fdaa2_2168c234_c4c6628b_80dc1cd1)
}

fn sin_k_dd(i: u32) -> DD {
    let t = SIN_K_PI_OVER_128[(i & 255) as usize];
    DD::new(t[0], t[1])
}

/// `range_reduction_small`: returns k, with u = x - k * pi/128 in double-double.
fn range_reduction_small(x: f64, u: &mut DD) -> u32 {
    const MPI_OVER_128: [f64; 3] = [
        -0.024543692605220713,
        -9.495469541099947e-13,
        -3.159791013743673e-23,
    ]; // -0x1.921fb544p-6, -0x1.0b4611a6p-40, -0x1.3198a2e037073p-75
    const ONE_TWENTY_EIGHT_OVER_PI_D: f64 = 40.74366543152521; // 0x1.45f306dc9c883p5
    let prod_hi = x * ONE_TWENTY_EIGHT_OVER_PI_D;
    let kd = nearest_integer(prod_hi);

    let y_hi = fma(kd, MPI_OVER_128[0], x); // Exact
    u.hi = fma(kd, MPI_OVER_128[1], y_hi);
    let u0 = y_hi - u.hi; // Exact
    let u1 = fma(kd, MPI_OVER_128[1], u0); // Exact
    u.lo = fma(kd, MPI_OVER_128[2], u1);

    (kd as i64) as u32
}

/// `range_reduction_small_f128`
fn range_reduction_small_f128(x: f64) -> F128 {
    const ONE_TWENTY_EIGHT_OVER_PI_D: f64 = 40.74366543152521; // 0x1.45f306dc9c883p5
    let prod_hi = x * ONE_TWENTY_EIGHT_OVER_PI_D;
    let kd = nearest_integer(prod_hi);

    let mk_f128 = F128::from_f64(-kd);
    let x_f128 = F128::from_f64(x);
    let p_hi = quick_mul(x_f128, F128::from_f64(ONE_TWENTY_EIGHT_OVER_PI[3][0]));
    let p_mid = quick_mul(x_f128, F128::from_f64(ONE_TWENTY_EIGHT_OVER_PI[3][1]));
    let p_lo = quick_mul(x_f128, F128::from_f64(ONE_TWENTY_EIGHT_OVER_PI[3][2]));
    let s_hi = quick_add(p_hi, mk_f128);
    let s_lo = quick_add(p_mid, p_lo);
    let y = quick_add(s_hi, s_lo);

    quick_mul(y, pi_over_128_f128())
}

/// `LargeRangeReduction`
#[derive(Default)]
struct LargeRangeReduction {
    idx: usize,
    x_reduced: f64,
    y_hi: f64,
    y_lo: f64,
    y_mid: DD,
}

impl LargeRangeReduction {
    /// `LargeRangeReduction::fast` (range_reduction_double_fma.h)
    fn fast(&mut self, x: f64, u: &mut DD) -> u32 {
        let bits = x.to_bits();
        let x_e_m62: i32 = ((bits >> 52) & 0x7ff) as i32 - (EXP_BIAS as i32 + 62);
        self.idx = ((x_e_m62 >> 4) + 3) as usize;
        let new_biased = ((x_e_m62 & 15) + EXP_BIAS as i32 + 62) as u64;
        self.x_reduced = f64::from_bits((bits & !(0x7ffu64 << 52)) | (new_biased << 52));
        let t = &ONE_TWENTY_EIGHT_OVER_PI[self.idx];
        let ph = exact_mult(self.x_reduced, t[0]);
        let pm = exact_mult(self.x_reduced, t[1]);
        let pl = exact_mult(self.x_reduced, t[2]);
        let sum_hi = ph.lo + pm.hi;
        let kd = nearest_integer(sum_hi);

        self.y_hi = (ph.lo - kd) + pm.hi; // Exact
        self.y_mid = exact_add(pm.lo, pl.hi);
        self.y_lo = pl.lo;

        let y_l = fma(self.x_reduced, t[3], self.y_lo);
        let mut y = exact_add(self.y_hi, self.y_mid.hi);
        y.lo += self.y_mid.lo + y_l;

        const PI_OVER_128_DD: DD = DD::new(9.567553118338697e-19, 0.02454369260617026); // 0x1.1a62633145c07p-60, 0x1.921fb54442d18p-6

        *u = quick_mult(y, PI_OVER_128_DD);

        (kd as i64) as u32
    }

    /// `LargeRangeReduction::accurate`
    fn accurate(&self) -> F128 {
        let y_lo_0 = F128::from_f64(self.x_reduced * ONE_TWENTY_EIGHT_OVER_PI[self.idx][3]);
        let y_lo_1 = quick_add(F128::from_f64(self.y_lo), y_lo_0);
        let y_mid_f128 = quick_add(F128::from_f64(self.y_mid.lo), y_lo_1);
        let y_hi_f128 = quick_add(F128::from_f64(self.y_hi), F128::from_f64(self.y_mid.hi));
        let y = quick_add(y_hi_f128, y_mid_f128);

        quick_mul(y, pi_over_128_f128())
    }
}

/// `sincos_eval(const DoubleDouble &u, DoubleDouble &sin_u, DoubleDouble &cos_u)`
fn sincos_eval(u: DD, sin_u: &mut DD, cos_u: &mut DD) -> f64 {
    let u_hi_sq = u.hi * u.hi; // Error < ulp(u_hi^2) < 2^(-6 - 52) = 2^-58.
    let p1 = fma(u_hi_sq, -0.0001984126984126984, 0.008333333333333333);
    let q1 = fma(u_hi_sq, 0.041666666666666664, -0.5);
    let u_hi_3 = u_hi_sq * u.hi;
    let p2 = fma(u_hi_sq, p1, -0.16666666666666666);
    let q2 = fma(u_hi_sq, q1, 1.0);
    let sin_lo = fma(u_hi_3, p2, u.lo * q2);

    let u_hi_neg_half = (-0.5) * u.hi;
    let mut v = DD::default();

    v.hi = fma(u.hi, u_hi_neg_half, 1.0);
    v.lo = 1.0 - v.hi; // Exact
    v.lo = fma(u.hi, u_hi_neg_half, v.lo);

    let r1 = fma(u_hi_sq, 2.48015873015873e-05, -0.001388888888888889);
    let s1 = fma(u_hi_sq, 0.16666666666666666, -1.0);
    let u_hi_4 = u_hi_sq * u_hi_sq;
    let u_hi_u_lo = u.hi * u.lo;
    let r2 = fma(u_hi_sq, r1, 0.041666666666666664);
    let s2 = fma(u_hi_u_lo, s1, v.lo);
    let cos_lo = fma(u_hi_4, r2, s2);

    *sin_u = exact_add(u.hi, sin_lo);
    *cos_u = exact_add(v.hi, cos_lo);

    fma(u_hi_3.abs(), 4.440892098500626e-16, 2.465190328815662e-32)
}

/// `sincos_eval(const Float128 &u, Float128 &sin_u, Float128 &cos_u)`
fn sincos_eval_f128(u: F128, sin_u: &mut F128, cos_u: &mut F128) {
    let u_sq = quick_mul(u, u);

    let sin_coeffs = [
        F128::new(false, -127, 0x80000000_00000000_00000000_00000000), // 1
        F128::new(true, -130, 0xaaaaaaaa_aaaaaaaa_aaaaaaaa_aaaaaaab),  // -1/3!
        F128::new(false, -134, 0x88888888_88888888_88888888_88888889), // 1/5!
        F128::new(true, -140, 0xd00d00d0_0d00d00d_00d00d00_d00d00d0),  // -1/7!
        F128::new(false, -146, 0xb8ef1d2a_b6399c7d_560e4472_800b8ef2), // 1/9!
        F128::new(true, -153, 0xd7322b3f_aa271c7f_3a3f25c1_bee38f10),  // -1/11!
        F128::new(false, -160, 0xb092309d_43684be5_1c198e91_d7b4269e), // 1/13!
    ];
    let cos_coeffs = [
        F128::new(false, -127, 0x80000000_00000000_00000000_00000000), // 1.0
        F128::new(true, -128, 0x80000000_00000000_00000000_00000000),  // 1/2
        F128::new(false, -132, 0xaaaaaaaa_aaaaaaaa_aaaaaaaa_aaaaaaab), // 1/4!
        F128::new(true, -137, 0xb60b60b6_0b60b60b_60b60b60_b60b60b6),  // 1/6!
        F128::new(false, -143, 0xd00d00d0_0d00d00d_00d00d00_d00d00d0), // 1/8!
        F128::new(true, -149, 0x93f27dbb_c4fae397_780b69f5_333c725b),  // 1/10!
        F128::new(false, -156, 0x8f76c77f_c6c4bdaa_26d4c3d6_7f425f60), // 1/12!
    ];

    *sin_u = quick_mul(u, polyeval_f128(u_sq, &sin_coeffs));
    *cos_u = polyeval_f128(u_sq, &cos_coeffs);
}

/// `get_sin_k` lambda of sin/cos/tan's accurate pass.
fn get_sin_k(kk: u32) -> F128 {
    let idx = if kk & 64 != 0 {
        64 - (kk & 63)
    } else {
        kk & 63
    };
    let mut ans = SIN_K_PI_OVER_128_F128[idx as usize];
    if kk & 128 != 0 {
        ans.neg = true;
    }
    ans
}

fn biased_exponent(x: f64) -> u16 {
    ((x.to_bits() >> 52) & 0x7ff) as u16
}

/// Non-finite and huge inputs: `x_e > 2 * EXP_BIAS` (inf and NaN).
fn nan_result(x: f64) -> f64 {
    // quiet NaN either way; x + qNaN
    x + f64::NAN
}

/// `sin(x)`
pub(crate) fn sin(x: f64) -> f64 {
    sin_impl::<0>(x)
}

/// `sin` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn sin_impl<const FORCE: u8>(x: f64) -> f64 {
    let x_e = biased_exponent(x);

    let mut y = DD::default();
    let k: u32;
    let mut range_reduction_large = LargeRangeReduction::default();

    if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
        if x_e < EXP_BIAS - 7 {
            if x_e < EXP_BIAS - 26 {
                if x == 0.0 {
                    return x + x; // Make sure it works with FTZ/DAZ.
                }
                return fma(x, -5.551115123125783e-17, x);
            }
            k = 0;
            y.lo = 0.0;
            y.hi = x;
        } else {
            k = range_reduction_small(x, &mut y);
        }
    } else {
        if x_e > 2 * EXP_BIAS {
            return nan_result(x);
        }
        k = range_reduction_large.fast(x, &mut y);
    }

    let mut sin_y = DD::default();
    let mut cos_y = DD::default();
    let err = sincos_eval(y, &mut sin_y, &mut cos_y);

    let sin_k = sin_k_dd(k);
    let cos_k = sin_k_dd(k.wrapping_add(64));

    let sin_k_cos_y = quick_mult(cos_y, sin_k);
    let cos_k_sin_y = quick_mult(sin_y, cos_k);

    let mut rr = exact_add_slow(sin_k_cos_y.hi, cos_k_sin_y.hi);
    rr.lo += sin_k_cos_y.lo + cos_k_sin_y.lo;

    let rlp = rr.lo + err;
    let rlm = rr.lo - err;

    let r_upper = rr.hi + rlp; // (rr.lo + ERR);
    let r_lower = rr.hi + rlm; // (rr.lo - ERR);

    if FORCE == 0 && r_upper == r_lower {
        return r_upper;
    }

    let u_f128 = if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
        range_reduction_small_f128(x)
    } else {
        range_reduction_large.accurate()
    };

    let mut sin_u = F128::new(false, 0, 0);
    let mut cos_u = F128::new(false, 0, 0);
    sincos_eval_f128(u_f128, &mut sin_u, &mut cos_u);

    let sin_k_f128 = get_sin_k(k);
    let cos_k_f128 = get_sin_k(k.wrapping_add(64));

    let r = quick_add(quick_mul(sin_k_f128, cos_u), quick_mul(cos_k_f128, sin_u));

    r.to_f64()
}

/// `cos(x)`
pub(crate) fn cos(x: f64) -> f64 {
    cos_impl::<0>(x)
}

/// `cos` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn cos_impl<const FORCE: u8>(x: f64) -> f64 {
    let x_e = biased_exponent(x);

    let mut y = DD::default();
    let k: u32;
    let mut range_reduction_large = LargeRangeReduction::default();

    if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
        if x_e < EXP_BIAS - 7 {
            if x_e < EXP_BIAS - 27 {
                if x == 0.0 {
                    return 1.0;
                }
                // round_result_slightly_down(1.0) rounds to nearest: 1.0 - min_normal == 1.0
                return 1.0 - f64::MIN_POSITIVE;
            }
            k = 0;
            y.lo = 0.0;
            y.hi = x;
        } else {
            k = range_reduction_small(x, &mut y);
        }
    } else {
        if x_e > 2 * EXP_BIAS {
            return nan_result(x);
        }
        k = range_reduction_large.fast(x, &mut y);
    }

    let mut sin_y = DD::default();
    let mut cos_y = DD::default();
    let err = sincos_eval(y, &mut sin_y, &mut cos_y);

    let msin_k = sin_k_dd(k.wrapping_add(128));
    let cos_k = sin_k_dd(k.wrapping_add(64));

    let cos_k_cos_y = quick_mult(cos_y, cos_k);
    let msin_k_sin_y = quick_mult(sin_y, msin_k);

    let mut rr = exact_add_slow(cos_k_cos_y.hi, msin_k_sin_y.hi);
    rr.lo += msin_k_sin_y.lo + cos_k_cos_y.lo;

    let rlp = rr.lo + err;
    let rlm = rr.lo - err;

    let r_upper = rr.hi + rlp; // (rr.lo + ERR);
    let r_lower = rr.hi + rlm; // (rr.lo - ERR);

    if FORCE == 0 && r_upper == r_lower {
        return r_upper;
    }

    let u_f128 = if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
        range_reduction_small_f128(x)
    } else {
        range_reduction_large.accurate()
    };

    let mut sin_u = F128::new(false, 0, 0);
    let mut cos_u = F128::new(false, 0, 0);
    sincos_eval_f128(u_f128, &mut sin_u, &mut cos_u);

    let msin_k_f128 = get_sin_k(k.wrapping_add(128));
    let cos_k_f128 = get_sin_k(k.wrapping_add(64));

    let r = quick_add(quick_mul(cos_k_f128, cos_u), quick_mul(msin_k_f128, sin_u));

    r.to_f64()
}

/// `tan_internal::tan_eval(const DoubleDouble &u, DoubleDouble &result)`
fn tan_eval(u: DD, result: &mut DD) -> f64 {
    let u_hi_sq = u.hi * u.hi; // Error < ulp(u_hi^2) < 2^(-6 - 52) = 2^-58.
    let p1 = fma(u_hi_sq, 0.021869488536155203, 0.05396825396825397);
    let p2 = fma(u_hi_sq, 0.13333333333333333, 0.3333333333333333);
    let q1 = fma(u_hi_sq, 0.6666666666666666, 1.0);
    let u_hi_3 = u_hi_sq * u.hi;
    let u_hi_4 = u_hi_sq * u_hi_sq;
    let p3 = fma(u_hi_4, p1, p2);
    let q2 = fma(u_hi_sq, q1, 1.0);
    let tan_lo = fma(u_hi_3, p3, u.lo * q2);
    *result = exact_add(u.hi, tan_lo);
    fma(u_hi_3.abs(), 4.440892098500626e-16, 1.9721522630525295e-31)
}

/// `tan_internal::tan_eval(const Float128 &u)`
fn tan_eval_f128(u: F128) -> F128 {
    let u_sq = quick_mul(u, u);
    quick_mul(u, polyeval_f128(u_sq, &TAN_COEFFS))
}

/// `tan_internal::newton_raphson_div`
fn newton_raphson_div(a: F128, mut b: F128, q: f64) -> F128 {
    let q0 = F128::from_f64(q);
    let two = F128::from_f64(2.0);
    b.neg = !b.neg;
    let q1 = quick_mul(q0, quick_add(two, quick_mul(b, q0)));
    let q2 = quick_mul(q1, quick_add(two, quick_mul(b, q1)));
    quick_mul(a, q2)
}

/// `tan(x)`
pub(crate) fn tan(x: f64) -> f64 {
    tan_impl::<0>(x)
}

/// `tan` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn tan_impl<const FORCE: u8>(x: f64) -> f64 {
    let x_e = biased_exponent(x);

    let mut y = DD::default();
    let k: u32;
    let mut range_reduction_large = LargeRangeReduction::default();

    if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
        if x_e < EXP_BIAS - 7 {
            if x_e < EXP_BIAS - 27 {
                if x == 0.0 {
                    return x + x; // Make sure it works with FTZ/DAZ.
                }
                return fma(x, 5.551115123125783e-17, x);
            }
            k = 0;
            y.lo = 0.0;
            y.hi = x;
        } else {
            k = range_reduction_small(x, &mut y);
        }
    } else {
        if x_e > 2 * EXP_BIAS {
            return nan_result(x);
        }
        k = range_reduction_large.fast(x, &mut y);
    }

    let mut tan_y = DD::default();
    let err = tan_eval(y, &mut tan_y);

    let msin_k = sin_k_dd(k.wrapping_add(128));
    let cos_k = sin_k_dd(k.wrapping_add(64));

    let cos_k_tan_y = quick_mult(tan_y, cos_k);
    let msin_k_tan_y = quick_mult(tan_y, msin_k);

    let mut num_dd = exact_add_slow(cos_k_tan_y.hi, -msin_k.hi);
    let mut den_dd = exact_add_slow(msin_k_tan_y.hi, cos_k.hi);
    num_dd.lo += cos_k_tan_y.lo - msin_k.lo;
    den_dd.lo += msin_k_tan_y.lo + cos_k.lo;

    let tan_x = dd_div(num_dd, den_dd);

    let den_inv: u64 =
        ((EXP_BIAS as u64 + 1) << (52 + 1)) - (den_dd.hi.to_bits() & (0x7ffu64 << 52));

    let tan_err = err * fma(f64::from_bits(den_inv), tan_x.hi.abs(), 1.0);

    let err_higher = tan_x.lo + tan_err;
    let err_lower = tan_x.lo - tan_err;

    let tan_upper = tan_x.hi + err_higher;
    let tan_lower = tan_x.hi + err_lower;

    if FORCE == 0 && tan_upper == tan_lower {
        return tan_upper;
    }

    let u_f128 = if x_e < EXP_BIAS + FAST_PASS_EXPONENT {
        range_reduction_small_f128(x)
    } else {
        range_reduction_large.accurate()
    };

    let tan_u = tan_eval_f128(u_f128);

    let sin_k_f128 = get_sin_k(k);
    let cos_k_f128 = get_sin_k(k.wrapping_add(64));
    let msin_k_f128 = get_sin_k(k.wrapping_add(128));

    let num_f128 = quick_add(sin_k_f128, quick_mul(cos_k_f128, tan_u));
    let den_f128 = quick_add(cos_k_f128, quick_mul(msin_k_f128, tan_u));

    let result = newton_raphson_div(num_f128, den_f128, 1.0 / den_dd.hi);

    result.to_f64()
}
