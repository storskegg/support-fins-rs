//! Ports of LLVM libc `math::atan` and `math::atan2` (src/__support/math/{atan,atan2}.h) and
//! `atan_utils.h` (`atan_eval`), revision 9309c117, as V8 15.0.245 calls them from
//! `v8::base::ieee754::{atan,atan2}` (src/base/ieee754.cc:164-165). FMA, accurate pass.
//! Unlike the logarithms these have no 128-bit fallback.

use super::fputil::*;
use super::tables_trig::ATAN_I;

const IS_NEG: [f64; 2] = [1.0, -1.0];

fn atan_i(idx: usize) -> DD {
    DD::new(ATAN_I[idx][0], ATAN_I[idx][1])
}

/// `atan_eval(const DoubleDouble &x)`: approximates atan for |x| <= 2^-7.
fn atan_eval(x: DD) -> DD {
    let mut p = DD::default();
    p.hi = x.hi;
    let x_hi_sq = x.hi * x.hi;
    let c0 = fma(x_hi_sq, 0.2, -0.3333333333333333);
    let c1 = fma(x_hi_sq, 0.1111111111111111, -0.14285714285714285);
    let x_hi_3 = x_hi_sq * x.hi;
    let x_hi_4 = x_hi_sq * x_hi_sq;
    let d0 = fma(x_hi_4, c1, c0);
    let d1 = fma(x_hi_4 - x_hi_sq, x.lo, x.lo);
    p.lo = fma(x_hi_3, d0, d1);
    p
}

/// `atan(x)`
pub(crate) fn atan(x: f64) -> f64 {
    const PI_OVER_2: DD = DD::new(6.123233995736766e-17, 1.5707963267948966); // 0x1.1a62633145c07p-54, 0x1.921fb54442d18p0
    const MPI_OVER_2: DD = DD::new(-6.123233995736766e-17, -1.5707963267948966); // -0x1.1a62633145c07p-54, -0x1.921fb54442d18p0

    let x_sign = (x.to_bits() >> 63) != 0;
    let x_abs = x.to_bits() & 0x7fff_ffff_ffff_ffff;
    let x_exp: i32 = (x_abs >> 52) as i32 - 1023;

    if x_exp < 0 {
        if x_exp < -26 {
            if x == 0.0 {
                return x;
            }
            return fma(-5.551115123125783e-17, x, x);
        }

        let x_d = f64::from_bits(x_abs);
        let mut k = nearest_integer(64.0 * x_d);
        let idx = k as u32 as usize;
        k *= 0.015625;

        let num = DD {
            lo: 0.0,
            hi: x_d - k,
        };

        let den_hi = fma(x_d, k, 1.0);
        let prod = exact_mult(x_d, k);
        let den = DD {
            hi: den_hi,
            lo: ((1.0 - den_hi) + prod.hi) + prod.lo,
        };

        let x_r = dd_div(num, den);

        let p = atan_eval(x_r);

        let c0 = exact_add(atan_i(idx).hi, p.hi);
        let c1 = c0.lo + (atan_i(idx).lo + p.lo);
        return IS_NEG[x_sign as usize] * (c0.hi + c1);
    }

    if x_exp >= 53 {
        if x.is_nan() {
            return x;
        }
        return fma(
            IS_NEG[x_sign as usize],
            PI_OVER_2.hi,
            IS_NEG[x_sign as usize] * PI_OVER_2.lo,
        );
    }

    let x_d = f64::from_bits(x_abs);
    let y = 1.0 / x_d;

    let mut k = nearest_integer(64.0 * y);
    let idx = k as u32 as usize;
    k *= 0.015625;

    let den = exact_add(x_d, k);
    let num_hi = fma(-x_d, k, 1.0);
    let prod = exact_mult(x_d, k);
    let num = DD {
        hi: num_hi,
        lo: ((1.0 - num_hi) - prod.hi) - prod.lo,
    };

    let x_r = dd_div(num, den);

    let p = atan_eval(x_r);

    let c0 = exact_add(MPI_OVER_2.hi, atan_i(idx).hi);
    let c1 = exact_add(c0.hi, p.hi);
    let c2 = c1.lo + (c0.lo + p.lo) + (atan_i(idx).lo + MPI_OVER_2.lo);

    IS_NEG[(!x_sign) as usize] * (c1.hi + c2)
}

/// `atan2(y, x)`
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    const ZERO: DD = DD::new(0.0, 0.0);
    const MZERO: DD = DD::new(-0.0, -0.0);
    const PI: DD = DD::new(1.2246467991473532e-16, 3.141592653589793); // 0x1.1a62633145c07p-53, 0x1.921fb54442d18p+1
    const MPI: DD = DD::new(-1.2246467991473532e-16, -3.141592653589793); // -0x1.1a62633145c07p-53, -0x1.921fb54442d18p+1
    const PI_OVER_2: DD = DD::new(6.123233995736766e-17, 1.5707963267948966); // 0x1.1a62633145c07p-54, 0x1.921fb54442d18p0
    const MPI_OVER_2: DD = DD::new(-6.123233995736766e-17, -1.5707963267948966); // -0x1.1a62633145c07p-54, -0x1.921fb54442d18p0
    const PI_OVER_4: DD = DD::new(3.061616997868383e-17, 0.7853981633974483); // 0x1.1a62633145c07p-55, 0x1.921fb54442d18p-1
    const THREE_PI_OVER_4: DD = DD::new(9.184850993605148e-17, 2.356194490192345); // 0x1.a79394c9e8a0ap-54, 0x1.2d97c7f3321d2p+1
    // CONST_ADJ[x_sign][y_sign][recip]
    const CONST_ADJ: [[[DD; 2]; 2]; 2] = [
        [[ZERO, MPI_OVER_2], [MZERO, MPI_OVER_2]],
        [[MPI, PI_OVER_2], [MPI, PI_OVER_2]],
    ];

    let x_sign = (x.to_bits() >> 63) != 0;
    let y_sign = (y.to_bits() >> 63) != 0;
    let x_abs = x.to_bits() & 0x7fff_ffff_ffff_ffff;
    let y_abs = y.to_bits() & 0x7fff_ffff_ffff_ffff;
    let recip = x_abs < y_abs;
    let min_abs = if recip { x_abs } else { y_abs };
    let max_abs = if !recip { x_abs } else { y_abs };
    let mut min_exp = (min_abs >> 52) as u32;
    let mut max_exp = (max_abs >> 52) as u32;

    let mut num = f64::from_bits(min_abs);
    let mut den = f64::from_bits(max_abs);

    if max_exp > 0x7ffu32 - 128u32 || min_exp < 128u32 {
        if x.is_nan() || y.is_nan() {
            return f64::NAN;
        }
        let x_except: usize = if x == 0.0 {
            0
        } else if f64::from_bits(x_abs).is_infinite() {
            2
        } else {
            1
        };
        let y_except: usize = if y == 0.0 {
            0
        } else if f64::from_bits(y_abs).is_infinite() {
            2
        } else {
            1
        };

        const EXCEPTS: [[[DD; 2]; 3]; 3] = [
            [[ZERO, PI], [ZERO, PI], [ZERO, PI]],
            [[PI_OVER_2, PI_OVER_2], [ZERO, ZERO], [ZERO, PI]],
            [
                [PI_OVER_2, PI_OVER_2],
                [PI_OVER_2, PI_OVER_2],
                [PI_OVER_4, THREE_PI_OVER_4],
            ],
        ];

        if x_except != 1 || y_except != 1 {
            let r = EXCEPTS[y_except][x_except][x_sign as usize];
            return fma(
                IS_NEG[y_sign as usize],
                r.hi,
                IS_NEG[y_sign as usize] * r.lo,
            );
        }
        let scale_up = min_exp < 128u32;
        let scale_down = max_exp > 0x7ffu32 - 128u32;
        // Normalize the values: scale_up and scale_down cannot both hold here for finite inputs
        // unless the ratio is huge, in which case both are applied below as in the C++.
        if scale_up {
            num *= 1.8446744073709552e+19;
            if !scale_down {
                den *= 1.8446744073709552e+19;
            }
        } else if scale_down {
            den *= 5.421010862427522e-20;
            if !scale_up {
                num *= 5.421010862427522e-20;
            }
        }

        let min_abs = num.to_bits();
        let max_abs = den.to_bits();
        min_exp = (min_abs >> 52) as u32;
        max_exp = (max_abs >> 52) as u32;
    }

    let final_sign = IS_NEG[((x_sign != y_sign) != recip) as usize];
    let const_term = CONST_ADJ[x_sign as usize][y_sign as usize][recip as usize];
    let exp_diff = max_exp.wrapping_sub(min_exp);
    if exp_diff > 54 {
        return fma(
            final_sign,
            const_term.hi,
            final_sign * (const_term.lo + num / den),
        );
    }

    let mut k = nearest_integer(64.0 * num / den);
    let idx = k as u32 as usize;
    k *= 0.015625;

    let num_k = exact_mult(num, k);
    let den_k = exact_mult(den, k);

    let num_dd = exact_add(num - den_k.hi, -den_k.lo);
    let mut den_dd = exact_add(den, num_k.hi);
    den_dd.lo += num_k.lo;

    let q = dd_div(num_dd, den_dd);
    let p = atan_eval(q);

    let mut r = dd_add(const_term, dd_add(atan_i(idx), p));
    r.hi *= final_sign;
    r.lo *= final_sign;

    r.hi + r.lo
}
