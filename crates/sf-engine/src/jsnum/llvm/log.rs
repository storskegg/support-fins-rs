//! Port of LLVM libc `math::log` (src/__support/math/log.h, revision 9309c117), as V8 15.0.245
//! calls it from `v8::base::ieee754::log` (src/base/ieee754.cc:226). FMA, accurate pass.

use super::fputil::*;
use super::log_rr::{LogRR, log_range_reduction};
use super::tables_common::{LOG_COEFFS, LOG_R_DD, RD};
use super::tables_log::{LOG_STEP_1, LOG_STEP_2, LOG_STEP_3, LOG_STEP_4};

const LOG_2_HI: f64 = 0.6931471805598903; // LSB = 2^-43
const LOG_2_LO: f64 = 5.497923018708371e-14; // LSB = 2^-97

/// A simple upper bound for the error of e_x * log(2) - log(r).
const HI_ERR: f64 = 2.5849394142282115e-26; // 0x1.0p-85
/// Extra errors from P is from using x^2 to reduce evaluation latency.
const P_ERR: f64 = 8.881784197001252e-16; // 0x1.0p-50

/// log(2) with 128-bit precision
fn log_2() -> F128 {
    F128::new(false, -128, 0xb17217f7_d1cf79ab_c9e3b398_03f2f6af)
}

fn big_coeffs() -> [F128; 3] {
    [
        F128::new(true, -129, 0x80000000_0006a710_b59c58e5_554d581c),
        F128::new(false, -129, 0xaaaaaaaa_aaaaaabd_de05c7c9_4ae9cbae),
        F128::new(true, -128, 0x80000000_00000000_00000000_00000000),
    ]
}

/// `log_accurate`: reuses the output of the fast pass range reduction.
fn log_accurate(e_x: i32, index: usize, m_x: f64) -> f64 {
    let big = big_coeffs();
    let table = LogRR {
        step_1: &LOG_STEP_1,
        step_2: &LOG_STEP_2,
        step_3: &LOG_STEP_3,
        step_4: &LOG_STEP_4,
    };
    let e_x_f128 = F128::from_f32(e_x as f32);
    let mut sum = quick_mul(log_2(), e_x_f128);
    sum = quick_add(sum, table.step_1[index]);

    let v_f128 = log_range_reduction(m_x, &table, &mut sum);
    sum = quick_add(sum, v_f128);

    // Polynomial approximation
    let mut p = quick_mul(v_f128, big[0]);
    p = quick_mul(v_f128, quick_add(p, big[1]));
    p = quick_mul(v_f128, quick_add(p, big[2]));
    p = quick_mul(v_f128, p);

    let r = quick_add(sum, p);
    r.to_f64()
}

/// `log(x)`
pub(crate) fn log(x: f64) -> f64 {
    log_impl::<0>(x)
}

/// `log` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn log_impl<const FORCE: u8>(x: f64) -> f64 {
    let mut x_u = x.to_bits();
    let mut x_e: i32 = -1023;

    if x_u == 0x3ff0_0000_0000_0000 {
        // log(1.0) = +0.0
        return 0.0;
    }
    if x_u < 0x0010_0000_0000_0000 || x_u > 0x7fef_ffff_ffff_ffff {
        if x == 0.0 {
            return f64::NEG_INFINITY;
        }
        if (x_u >> 63) != 0 && !x.is_nan() {
            return f64::NAN;
        }
        if x.is_infinite() || x.is_nan() {
            return x;
        }
        // Normalize denormal inputs.
        x_u = (x * 4503599627370496.0).to_bits();
        x_e -= 52;
    }

    // log(x) = log(2^x_e * x_m) = x_e * log(2) + log(x_m)
    // Range reduction for log(x_m): find r such that -2^-8 <= r * x_m - 1 < 2^-7
    let shifted = (x_u >> 45) as i32;
    let index = (shifted & 0x7F) as usize;
    let r = RD[index];

    // Add unbiased exponent. Add an extra 1 if the 8 leading fractional bits are all 1's.
    x_e += ((x_u + (1u64 << 45)) >> 52) as i32;
    let e_x = x_e as f64;

    // hi is exact
    let hi = fma(e_x, LOG_2_HI, LOG_R_DD[index][1]);
    let lo = fma(e_x, LOG_2_LO, LOG_R_DD[index][0]);

    // Set m = 1.mantissa.
    let x_m = (x_u & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000;
    let m = f64::from_bits(x_m);

    // Perform exact range reduction
    let u = fma(r, m, -1.0); // exact

    // Exact sum: r1.hi + r1.lo = e_x * log(2)_hi - log(r)_hi + u
    let r1 = exact_add(hi, u);

    let u_sq = u * u;
    // Degree-7 minimax polynomial
    let p0 = fma(u, LOG_COEFFS[1], LOG_COEFFS[0]);
    let p1 = fma(u, LOG_COEFFS[3], LOG_COEFFS[2]);
    let p2 = fma(u, LOG_COEFFS[5], LOG_COEFFS[4]);
    let p = polyeval(u_sq, &[lo + r1.lo, p0, p1, p2]);

    let err = fma(u_sq, P_ERR, HI_ERR);
    let left = r1.hi + (p - err);
    let right = r1.hi + (p + err);

    // Ziv's test if fast pass is accurate enough.
    if FORCE == 0 && left == right {
        return left;
    }
    log_accurate(x_e, index, u)
}
