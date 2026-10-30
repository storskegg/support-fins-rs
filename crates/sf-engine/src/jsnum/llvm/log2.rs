//! Port of LLVM libc `math::log2` (src/__support/math/log2.h, revision 9309c117), as V8 15.0.245
//! calls it from `v8::base::ieee754::log2` (src/base/ieee754.cc:228). FMA, accurate pass.

use super::fputil::{DD, F128, exact_add, fma, polyeval, quick_add, quick_mul, quick_mult};
use super::log_rr::{LogRR, log_range_reduction};
use super::tables_common::{LOG_COEFFS, RD};
use super::tables_log2::{LOG_R1, LOG2_STEP_1, LOG2_STEP_2, LOG2_STEP_3, LOG2_STEP_4};

const LOG2_E: DD = DD::new(2.0355273740931033e-17, 1.4426950408889634); // 0x1.777d0ffda0d24p-56, 0x1.71547652b82fep0
/// Extra errors from P is from using x^2 to reduce evaluation latency.
const P_ERR: f64 = 1.7763568394002505e-15; // 0x1.0p-49

fn big_coeffs() -> [F128; 4] {
    [
        F128::new(true, -129, 0xb8aa3b29_5c2b21e3_3eccf694_0d66bbcc),
        F128::new(false, -129, 0xf6384ee1_d01febc9_ee39a6d6_49394bb1),
        F128::new(true, -128, 0xb8aa3b29_5c17f0bb_be87fed0_67ea2ad5),
        F128::new(false, -127, 0xb8aa3b29_5c17f0bb_be87fed0_691d3e3f),
    ]
}

/// `log2_accurate`
fn log2_accurate(e_x: i32, index: usize, m_x: f64) -> f64 {
    let big = big_coeffs();
    let table = LogRR {
        step_1: &LOG2_STEP_1,
        step_2: &LOG2_STEP_2,
        step_3: &LOG2_STEP_3,
        step_4: &LOG2_STEP_4,
    };
    let mut sum = F128::from_f32(e_x as f32);
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

/// `log2(x)`
pub(crate) fn log2(x: f64) -> f64 {
    log2_impl::<0>(x)
}

/// `log2` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn log2_impl<const FORCE: u8>(x: f64) -> f64 {
    let mut x_u = x.to_bits();
    let mut x_e: i32 = -1023;

    if x_u == 0x3ff0_0000_0000_0000 {
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

    let shifted = (x_u >> 45) as i32;
    let index = (shifted & 0x7F) as usize;
    let r = RD[index];

    x_e += ((x_u + (1u64 << 45)) >> 52) as i32;
    let e_x = x_e as f64;

    let x_m = (x_u & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000;
    let m = f64::from_bits(x_m);

    let u = fma(r, m, -1.0); // exact

    let mut r1 = exact_add(LOG_R1[index][1], u);

    let u_sq = u * u;
    let p0 = fma(u, LOG_COEFFS[1], LOG_COEFFS[0]);
    let p1 = fma(u, LOG_COEFFS[3], LOG_COEFFS[2]);
    let p2 = fma(u, LOG_COEFFS[5], LOG_COEFFS[4]);
    let p = polyeval(u_sq, &[LOG_R1[index][0], p0, p1, p2]);

    r1.lo += p;

    let r2 = quick_mult(r1, LOG2_E);
    let mut r3 = exact_add(e_x, r2.hi);
    r3.lo += r2.lo;

    let err = u_sq * P_ERR;
    let left = r3.hi + (r3.lo - err);
    let right = r3.hi + (r3.lo + err);

    if FORCE == 0 && left == right {
        return left;
    }
    log2_accurate(x_e, index, u)
}
