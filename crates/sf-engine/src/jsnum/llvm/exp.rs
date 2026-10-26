//! Port of LLVM libc `math::exp` (src/__support/math/exp.h, revision 9309c117) together with
//! `exp_utils.h` (`ziv_test_denorm`), as V8 15.0.245 calls it from
//! `v8::base::ieee754::exp` (src/base/ieee754.cc:175). Built with FMA and the accurate pass.

use super::fputil::*;
use super::tables_exp::{EXP2_MID1, EXP2_MID2};

const LOG2_E: f64 = 1.4426950408889634; // 1.4426950408889634
const EXP_ERR_D: f64 = 1.6263032587282567e-19; // 1.6263032587282567e-19
const EXP_ERR_DD: f64 = 1.5777218104420236e-30; // 1.5777218104420236e-30
const MLOG_2_EXP2_M12_HI: f64 = -0.00016922538588914904; // -0.00016922538588914904
const MLOG_2_EXP2_M12_MID: f64 = 1.0256140314162804e-14; // 1.0256140314162804e-14
const MLOG_2_EXP2_M12_MID_30: f64 = 1.0256140311365355e-14; // 1.0256140311365355e-14
const MLOG_2_EXP2_M12_LO: f64 = 2.797449396191097e-24; // 2.797449396191097e-24

/// `poly_approx_d`
fn poly_approx_d(dx: f64) -> f64 {
    let dx2 = dx * dx;
    let c0 = fma(dx, 0.5, 1.0);
    let c1 = fma(dx, 0.041666666666666664, 0.16666666666666666);
    fma(dx2, c1, c0)
}

/// `poly_approx_dd`
fn poly_approx_dd(dx: DD) -> DD {
    const COEFFS: [DD; 7] = [
        DD::new(0.0, 1.0),
        DD::new(0.0, 1.0),
        DD::new(0.0, 0.5),
        DD::new(9.25185853854297e-18, 0.16666666666666666),
        DD::new(2.3129646346357427e-18, 0.041666666666666664),
        DD::new(1.1564823173178714e-19, 0.008333333333333333),
        DD::new(-5.300543954373577e-20, 0.001388888888888889),
    ];
    polyeval_dd(dx, &COEFFS)
}

/// `poly_approx_f128`
fn poly_approx_f128(dx: F128) -> F128 {
    let coeffs = [
        F128::new(false, -127, 0x80000000_00000000_00000000_00000000),
        F128::new(false, -127, 0x80000000_00000000_00000000_00000000),
        F128::new(false, -128, 0x80000000_00000000_00000000_00000000),
        F128::new(false, -130, 0xaaaaaaaa_aaaaaaaa_aaaaaaaa_aaaaaaab),
        F128::new(false, -132, 0xaaaaaaaa_aaaaaaaa_aaaaaaaa_aaaaaaab),
        F128::new(false, -134, 0x88888888_88888888_88888888_88888889),
        F128::new(false, -137, 0xb60b60b6_0b60b60b_60b60b60_b60b60b6),
        F128::new(false, -140, 0xd00d00d0_0d00d00d_00d00d00_d00d00d0),
    ];
    polyeval_f128(dx, &coeffs)
}

/// `exp_f128`
fn exp_f128(x: f64, kd: f64, idx1: usize, idx2: usize) -> F128 {
    let t1 = fma(kd, MLOG_2_EXP2_M12_HI, x);
    let t2 = kd * MLOG_2_EXP2_M12_MID_30;
    let t3 = kd * MLOG_2_EXP2_M12_LO;
    let dx = quick_add(
        F128::from_f64(t1),
        quick_add(F128::from_f64(t2), F128::from_f64(t3)),
    );
    let m1 = EXP2_MID1[idx1];
    let m2 = EXP2_MID2[idx2];
    // TripleDouble fields are listed {lo, mid, hi}
    let exp_mid1 = quick_add(
        F128::from_f64(m1[2]),
        quick_add(F128::from_f64(m1[1]), F128::from_f64(m1[0])),
    );
    let exp_mid2 = quick_add(
        F128::from_f64(m2[2]),
        quick_add(F128::from_f64(m2[1]), F128::from_f64(m2[0])),
    );
    let exp_mid = quick_mul(exp_mid1, exp_mid2);
    let p = poly_approx_f128(dx);
    let mut r = quick_mul(exp_mid, p);
    r.exponent += (kd as i32) >> 12;
    r
}

/// `exp_double_double`
fn exp_double_double(x: f64, kd: f64, exp_mid: DD) -> DD {
    let t1 = fma(kd, MLOG_2_EXP2_M12_HI, x);
    let t2 = kd * MLOG_2_EXP2_M12_MID_30;
    let t3 = kd * MLOG_2_EXP2_M12_LO;
    let mut dx = exact_add(t1, t2);
    dx.lo += t3;
    let p = poly_approx_dd(dx);
    quick_mult(exp_mid, p)
}

/// `ziv_test_denorm<SKIP_ZIV_TEST = false>` (exp_utils.h)
pub(crate) fn ziv_test_denorm(hi: i32, mid: f64, lo: f64, mut err: f64) -> Option<f64> {
    let exp_hi: i64 = (hi as i64 + 1022) * (1i64 << 52);
    let add_exp = |v: f64| f64::from_bits(exp_hi.wrapping_add(v.to_bits() as i64) as u64);
    let mid_hi = add_exp(mid);
    let lo_scaled = if lo != 0.0 { add_exp(lo) } else { 0.0 };
    let mut extra_factor = 0.0;
    let mut scale_down: u64 = 0x3FE0_0000_0000_0000;
    if (1.0 - mid_hi) > lo_scaled {
        err += 2.220446049250313e-16;
        extra_factor = 1.0;
        scale_down = 0x3FF0_0000_0000_0000;
    }
    let err_scaled = add_exp(err);
    let lo_u = lo_scaled + err_scaled;
    let lo_l = lo_scaled - err_scaled;
    let upper = extra_factor + (mid_hi + lo_u);
    let lower = extra_factor + (mid_hi + lo_l);
    if upper == lower {
        return Some(f64::from_bits(upper.to_bits().wrapping_sub(scale_down)));
    }
    None
}

/// `exp_internal::set_exceptional` (round to nearest only)
fn set_exceptional(x: f64) -> f64 {
    let x_u = x.to_bits();
    let x_abs = x_u & 0x7fff_ffff_ffff_ffff;
    // |x| <= 2^-53: exp(x) ~ 1 + x
    if x_abs <= 0x3ca0_0000_0000_0000 {
        return 1.0 + x;
    }
    // x <= log(2^-1075) or -inf/nan
    if x_u >= 0xc087_4910_d52d_3052 {
        if x.is_infinite() {
            return 0.0;
        }
        if x.is_nan() {
            return x;
        }
        return 0.0;
    }
    // x >= round(log(MAX_NORMAL), D, RU) or +inf/nan: x + inf
    x + f64::INFINITY
}

/// `exp(x)`
pub(crate) fn exp(x: f64) -> f64 {
    exp_impl::<0>(x)
}

/// `exp` with the Ziv stages forced: `FORCE` = 1 skips the double pass, 2 skips the double-double
/// pass too (only the tests use anything but 0, to exercise the 128-bit code).
pub(crate) fn exp_impl<const FORCE: u8>(x: f64) -> f64 {
    let x_u = x.to_bits();
    if x_u >= 0xc0874910d52d3052
        || (x_u < 0xbca0000000000000 && x_u >= 0x40862e42fefa39f0)
        || x_u < 0x3ca0000000000000
    {
        return set_exceptional(x);
    }
    // Rounding errors <= 2^-31 + 2^-41.
    let tmp = fma(x, LOG2_E, 3145728.0001220703);
    let k = (tmp.to_bits() >> 19) as i32;
    let kd = k as f64;
    let idx1 = ((k >> 6) & 0x3f) as usize;
    let idx2 = (k & 0x3f) as usize;
    let hi = k >> 12;
    let denorm = hi <= -1022;
    let m1 = EXP2_MID1[idx1];
    let m2 = EXP2_MID2[idx2];
    let exp_mid1 = DD::new(m1[1], m1[2]);
    let exp_mid2 = DD::new(m2[1], m2[2]);
    let exp_mid = quick_mult(exp_mid1, exp_mid2);
    let lo_h = fma(kd, MLOG_2_EXP2_M12_HI, x);
    let dx = fma(kd, MLOG_2_EXP2_M12_MID, lo_h);
    let mid_lo = dx * exp_mid.hi;
    let p = poly_approx_d(dx);
    let lo = fma(p, mid_lo, exp_mid.lo);

    let scale = |bits_val: f64| -> f64 {
        let exp_hi: i64 = (hi as i64) * (1i64 << 52);
        f64::from_bits(exp_hi.wrapping_add(bits_val.to_bits() as i64) as u64)
    };
    if FORCE == 0 {
        if denorm {
            if let Some(r) = ziv_test_denorm(hi, exp_mid.hi, lo, EXP_ERR_D) {
                return r;
            }
        } else {
            let upper = exp_mid.hi + (lo + EXP_ERR_D);
            let lower = exp_mid.hi + (lo - EXP_ERR_D);
            if upper == lower {
                return scale(upper);
            }
        }
    }
    // Use double-double
    let r_dd = exp_double_double(x, kd, exp_mid);
    if FORCE < 2 {
        if denorm {
            if let Some(r) = ziv_test_denorm(hi, r_dd.hi, r_dd.lo, EXP_ERR_DD) {
                return r;
            }
        } else {
            let upper_dd = r_dd.hi + (r_dd.lo + EXP_ERR_DD);
            let lower_dd = r_dd.hi + (r_dd.lo - EXP_ERR_DD);
            if upper_dd == lower_dd {
                return scale(upper_dd);
            }
        }
    }
    // Use 128-bit precision
    exp_f128(x, kd, idx1, idx2).to_f64()
}
