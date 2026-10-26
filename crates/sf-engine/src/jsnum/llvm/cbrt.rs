//! Port of LLVM libc `math::cbrt` (src/__support/math/cbrt.h, revision 9309c117), as V8 15.0.245
//! calls it from `v8::base::ieee754::cbrt` (src/base/ieee754.cc:233). FMA, accurate pass.

use super::fputil::*;

const REL_ERROR: f64 = 4.440892098500626e-16; // 0x1.0p-51

/// `cbrt_internal::initial_approximation`
fn initial_approximation(x: f64) -> f64 {
    const COEFFS: [f64; 8] = [
        3.471273287514779,
        -6.830810247982458,
        8.92351619384915,
        -7.471540905640615,
        4.0208001193431615,
        -1.3494228382831837,
        0.2575661358537974,
        -0.021382296276922758,
    ];

    let x_sq = x * x;

    let c0 = fma(x, COEFFS[1], COEFFS[0]);
    let c1 = fma(x, COEFFS[3], COEFFS[2]);
    let c2 = fma(x, COEFFS[5], COEFFS[4]);
    let c3 = fma(x, COEFFS[7], COEFFS[6]);

    let x_4 = x_sq * x_sq;
    let d0 = fma(x_sq, c1, c0);
    let d1 = fma(x_sq, c3, c2);

    fma(x_4, d1, d0)
}

/// `cbrt_internal::get_error` (FMA branch)
fn get_error(x_3: DD, a_sq: DD) -> f64 {
    fma(x_3.hi, a_sq.hi, -1.0) + fma(x_3.lo, a_sq.hi, x_3.hi * a_sq.lo)
}

/// `cbrt(x)`
pub(crate) fn cbrt(x: f64) -> f64 {
    cbrt_impl::<0>(x)
}

/// `cbrt` with the Ziv tests skipped when `FORCE` is not 0, so the tests can reach the
/// 128-bit accurate pass (a skipped test behaves as a failed one).
pub(crate) fn cbrt_impl<const FORCE: u8>(mut x: f64) -> f64 {
    let x_abs = x.to_bits() & 0x7fff_ffff_ffff_ffff;

    let mut exp_bias_correction: u32 = 682; // 1023 * 2/3

    if x_abs < 0x0010_0000_0000_0000 || x_abs >= 0x7ff0_0000_0000_0000 {
        if x == 0.0 || x_abs >= 0x7ff0_0000_0000_0000 {
            return x + x;
        }

        // Normalize denormal inputs.
        x *= 1.152921504606847e+18;
        exp_bias_correction -= 20;
    }

    let x_bits = x.to_bits();

    let x_e = ((x_bits >> 52) & 0x7ff) as u32;
    let out_e = x_e / 3 + exp_bias_correction;
    let shift_e = x_e % 3;

    let x_r = f64::from_bits((x_bits & ((1u64 << 52) - 1)) | (1023u64 << 52));

    let mut a_bits = x_bits & 0x800F_FFFF_FFFF_FFFF;
    a_bits |= ((shift_e + 1023) as u64) << 52;
    let a = f64::from_bits(a_bits);

    let p = initial_approximation(x_r);

    const EXP2_M2_OVER_3: [f64; 3] = [1.0, 0.6299605249474366, 0.3968502629920499]; // 0x1.428a2f98d728bp-1, 0x1.965fea53d6e3dp-2

    let x0 = EXP2_M2_OVER_3[shift_e as usize] * p;

    let a_sq = exact_mult(a, a);

    let x0_sq = exact_mult(x0, x0);
    let x0_3 = quick_mult_d(x0, x0_sq);

    let h0 = get_error(x0_3, a_sq);

    let err_coeffs: [f64; 3] = [
        -0.3333333333333333 - REL_ERROR, // -1/3 - relative_error
        0.2222222222222222,              // 2/9
        -0.1728395061728395,             // -14/81
    ];
    let e0 = polyeval(h0, &err_coeffs);
    let x0_h0 = x0 * h0;

    let x1_dd = DD::new(x0_h0 * e0, x0);

    let r1 = quick_mult_d(a, x1_dd);

    let update_exponent = |r: f64| -> f64 {
        let r_m = r.to_bits().wrapping_sub(0x3FF0_0000_0000_0000);
        let r_bits = r_m.wrapping_add((out_e as u64) << 52);
        f64::from_bits(r_bits)
    };

    let r1_lower = r1.hi + r1.lo;
    let r1_upper = r1.hi + fma(x0_h0, 2.0 * REL_ERROR * a, r1.lo);

    if FORCE == 0 && r1_upper == r1_lower {
        return update_exponent(r1_lower);
    }

    let x1 = x1_dd.hi + (e0 + REL_ERROR) * x0_h0;

    let x1_sq = exact_mult(x1, x1);
    let x1_3 = quick_mult_d(x1, x1_sq);
    let h1 = get_error(x1_3, a_sq);

    let e1 = h1 * (x1 * -0.3333333333333333);
    let x2 = exact_add(x1, e1);
    let r2 = quick_mult_d(a, x2);

    let r2_upper = r2.hi + fma(a, 1.9721522630525295e-31, r2.lo);
    let r2_lower = r2.hi + fma(a, -1.9721522630525295e-31, r2.lo);

    if FORCE < 2 && r2_upper == r2_lower {
        return update_exponent(r2_upper);
    }

    let x2_f128 = quick_add(F128::from_f64(x2.hi), F128::from_f64(x2.lo));
    let x2_3 = quick_mul(quick_mul(x2_f128, x2_f128), x2_f128);
    let a_sq_f128 = quick_mul(F128::from_f64(a), F128::from_f64(a));
    let x2_3_a_sq = quick_mul(x2_3, a_sq_f128);
    let h2_f128 = quick_add(x2_3_a_sq, F128::from_f64(-1.0));
    let h2 = h2_f128.to_f64();
    let t2 = quick_add(
        F128::from_f64(1.0),
        F128::from_f64(h2 * -0.3333333333333333),
    );
    let x3 = quick_mul(x2_f128, t2);
    let mut r3 = quick_mul(F128::from_f64(a), x3);

    let rounding_bits: u128 = r3.mantissa & 0x0000_0000_0000_03FF_FFFF_FFFF_FFFF_FFFF;

    let mut result = r3.to_f64();
    if rounding_bits < 0x0000_0000_0000_0000_0000_0000_0000_000F
        || rounding_bits >= 0x0000_0000_0000_03FF_FFFF_FFFF_FFFF_FFF0
    {
        r3.mantissa &= 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFF0;

        if rounding_bits >= 0x0000_0000_0000_03FF_FFFF_FFFF_FFFF_FFF0 {
            let tmp = F128::new(
                r3.neg,
                r3.exponent - 123,
                0x8000_0000_0000_0000_0000_0000_0000_0000,
            );
            let r4 = quick_add(r3, tmp);
            result = r4.to_f64();
        } else {
            result = r3.to_f64();
        }
    }

    update_exponent(result)
}
