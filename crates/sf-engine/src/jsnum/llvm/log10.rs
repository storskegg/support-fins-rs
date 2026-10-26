//! Port of LLVM libc `math::log10` (src/__support/math/log10.h, revision 9309c117), as V8 15.0.245
//! calls it from `v8::base::ieee754::log10` (src/base/ieee754.cc:229). FMA, accurate pass.

use super::fputil::*;
use super::log_rr::{LogRR, log_range_reduction};
use super::tables_common::{LOG_COEFFS, LOG_R_DD, RD};
use super::tables_log10::{LOG10_STEP_1, LOG10_STEP_2, LOG10_STEP_3, LOG10_STEP_4};

const LOG_2_HI: f64 = 0.6931471805598903; // LSB = 2^-43
const LOG_2_LO: f64 = 5.497923018708371e-14; // LSB = 2^-97
const LOG10_E: DD = DD::new(1.098319650216765e-17, 0.4342944819032518); // 0x1.95355baaafad3p-57, 0x1.bcb7b1526e50ep-2
/// A simple upper bound for the error of e_x * log(2) - log(r).
const HI_ERR: f64 = 2.5849394142282115e-26; // 0x1.0p-85
/// Extra errors from P is from using x^2 to reduce evaluation latency.
const P_ERR: f64 = 4.440892098500626e-16; // 0x1.0p-51

/// log10(2) with 128-bit precision
fn log10_2() -> F128 {
    F128::new(false, -129, 0x9a209a84_fbcff798_8f8959ac_0b7c9178)
}

fn big_coeffs() -> [F128; 4] {
    [
        F128::new(true, -131, 0xde5bd8a9_373f89a7_6903c4ce_1582517d),
        F128::new(false, -130, 0x943d3b1b_7a1af679_b8a21791_624e2e8a),
        F128::new(true, -130, 0xde5bd8a9_37287195_355baaaf_abc25990),
        F128::new(false, -129, 0xde5bd8a9_37287195_355baaaf_ad33dbd9),
    ]
}

/// `log10_accurate`
fn log10_accurate(e_x: i32, index: usize, m_x: f64) -> f64 {
    let big = big_coeffs();
    let table = LogRR {
        step_1: &LOG10_STEP_1,
        step_2: &LOG10_STEP_2,
        step_3: &LOG10_STEP_3,
        step_4: &LOG10_STEP_4,
    };
    let e_x_f128 = F128::from_f32(e_x as f32);
    let mut sum = quick_mul(log10_2(), e_x_f128);
    sum = quick_add(sum, table.step_1[index]);

    let v_f128 = log_range_reduction(m_x, &table, &mut sum);

    // Polynomial approximation
    let mut p = quick_mul(v_f128, big[0]);
    p = quick_mul(v_f128, quick_add(p, big[1]));
    p = quick_mul(v_f128, quick_add(p, big[2]));
    p = quick_mul(v_f128, quick_add(p, big[3]));

    let r = quick_add(sum, p);
    r.to_f64()
}

/// `log10(x)`
pub(crate) fn log10(x: f64) -> f64 {
    log10_impl::<0>(x)
}

/// `log10` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn log10_impl<const FORCE: u8>(x: f64) -> f64 {
    let mut x_u = x.to_bits();
    let mut x_e: i32 = -1023;

    if x_u == 0x3ff0_0000_0000_0000 {
        // log10(1.0) = +0.0
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

    // log10(x) = log10(2^x_e * x_m) = x_e * log10(2) + log10(x_m)
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

    // Error of u_sq = ulp(u^2);
    let u_sq = u * u;
    // Degree-7 minimax polynomial
    let p0 = fma(u, LOG_COEFFS[1], LOG_COEFFS[0]);
    let p1 = fma(u, LOG_COEFFS[3], LOG_COEFFS[2]);
    let p2 = fma(u, LOG_COEFFS[5], LOG_COEFFS[4]);
    let p = polyeval(u_sq, &[lo, p0, p1, p2]);

    // Exact sum: r1.hi + r1.lo = e_x * log(2)_hi - log(r)_hi + u
    let mut r1 = exact_add(hi, u);
    r1.lo += p;

    // Quick double-double multiplication: r2.hi + r2.lo ~ r1 * log10(e)
    let r2 = quick_mult(r1, LOG10_E);

    let err = fma(u_sq, P_ERR, HI_ERR);
    let left = r2.hi + (r2.lo - err);
    let right = r2.hi + (r2.lo + err);

    // Ziv's test if fast pass is accurate enough.
    if FORCE == 0 && left == right {
        return left;
    }

    // Exact cases:
    if (x_u & 0x3FFFFF) == 0 {
        match x_u {
            0x4024000000000000 => return 1.0,  // x = 10.0
            0x4059000000000000 => return 2.0,  // x = 10^2
            0x408f400000000000 => return 3.0,  // x = 10^3
            0x40c3880000000000 => return 4.0,  // x = 10^4
            0x40f86a0000000000 => return 5.0,  // x = 10^5
            0x412e848000000000 => return 6.0,  // x = 10^6
            0x416312d000000000 => return 7.0,  // x = 10^7
            0x4197d78400000000 => return 8.0,  // x = 10^8
            0x41cdcd6500000000 => return 9.0,  // x = 10^9
            0x4202a05f20000000 => return 10.0, // x = 10^10
            0x42374876e8000000 => return 11.0, // x = 10^11
            0x426d1a94a2000000 => return 12.0, // x = 10^12
            0x42a2309ce5400000 => return 13.0, // x = 10^13
            _ => {}
        }
    } else {
        match x_u {
            0x42d6bcc41e900000 => return 14.0, // x = 10^14
            0x430c6bf526340000 => return 15.0, // x = 10^15
            0x4341c37937e08000 => return 16.0, // x = 10^16
            0x4376345785d8a000 => return 17.0, // x = 10^17
            0x43abc16d674ec800 => return 18.0, // x = 10^18
            0x43e158e460913d00 => return 19.0, // x = 10^19
            0x4415af1d78b58c40 => return 20.0, // x = 10^20
            0x444b1ae4d6e2ef50 => return 21.0, // x = 10^21
            0x4480f0cf064dd592 => return 22.0, // x = 10^22
            _ => {}
        }
    }

    log10_accurate(x_e, index, u)
}
