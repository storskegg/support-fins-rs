//! The parts of LLVM libc's `src/__support/FPUtil` that V8's `ieee754` functions rely on:
//! double-double arithmetic (`double_double.h`), `DyadicFloat<128>` (`dyadic_float.h`) and
//! `polyeval` (`PolyEval.h`).
//!
//! V8 15.0.245 builds these for arm64 with FMA available (`__ARM_FEATURE_FMA`), so
//! `fputil::multiply_add` is a fused multiply-add, and `exact_mult` takes the FMA branch.
//! `fma` below is `f64::mul_add`, which is exactly rounded on every target.

/// `fputil::multiply_add(x, y, z)` with FMA: `x * y + z` rounded once.
#[inline]
pub(crate) fn fma(x: f64, y: f64, z: f64) -> f64 {
    x.mul_add(y, z)
}

/// `fputil::DoubleDouble` (`NumberPair<double>`): `lo` is declared first in C++.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DD {
    pub lo: f64,
    pub hi: f64,
}

impl DD {
    pub(crate) const fn new(lo: f64, hi: f64) -> DD {
        DD { lo, hi }
    }
}

/// `exact_add<FAST2SUM = true>(a, b)`
pub(crate) fn exact_add(a: f64, b: f64) -> DD {
    let hi = a + b;
    let t = hi - a;
    let lo = b - t;
    DD { lo, hi }
}

/// `exact_add<false>(a, b)` (Knuth's 2Sum)
pub(crate) fn exact_add_slow(a: f64, b: f64) -> DD {
    let hi = a + b;
    let t1 = hi - a;
    let t2 = hi - t1;
    let t3 = b - t1;
    let t4 = a - t2;
    DD { lo: t3 + t4, hi }
}

/// `add(const NumberPair &a, const NumberPair &b)`
pub(crate) fn dd_add(a: DD, b: DD) -> DD {
    let r = exact_add(a.hi, b.hi);
    let lo = a.lo + b.lo;
    exact_add(r.hi, r.lo + lo)
}

/// `exact_mult(a, b)` (FMA branch)
pub(crate) fn exact_mult(a: f64, b: f64) -> DD {
    let hi = a * b;
    let lo = fma(a, b, -hi);
    DD { lo, hi }
}

/// `quick_mult(T a, const NumberPair &b)`
pub(crate) fn quick_mult_d(a: f64, b: DD) -> DD {
    let mut r = exact_mult(a, b.hi);
    r.lo = fma(a, b.lo, r.lo);
    r
}

/// `quick_mult(const DoubleDouble &a, const DoubleDouble &b)`
pub(crate) fn quick_mult(a: DD, b: DD) -> DD {
    let mut r = exact_mult(a.hi, b.hi);
    let t1 = fma(a.hi, b.lo, r.lo);
    let t2 = fma(a.lo, b.hi, t1);
    r.lo = t2;
    r
}

/// `div(const NumberPair &a, const NumberPair &b)` (FMA branch)
pub(crate) fn dd_div(a: DD, b: DD) -> DD {
    let q = 1.0 / b.hi;
    let hi = a.hi * q;
    let e_hi = fma(b.hi, -hi, a.hi);
    let e_lo = fma(b.lo, -hi, a.lo);
    let lo = q * (e_hi + e_lo);
    DD { lo, hi }
}

/// `multiply_add<DoubleDouble>(a, b, c)` = `add(c, quick_mult(a, b))`
pub(crate) fn dd_multiply_add(a: DD, b: DD, c: DD) -> DD {
    dd_add(c, quick_mult(a, b))
}

/// `polyeval(x, a0, a1, ...)` on doubles: Horner with fused multiply-adds.
pub(crate) fn polyeval(x: f64, coeffs: &[f64]) -> f64 {
    let (last, rest) = coeffs.split_last().expect("polyeval needs a coefficient");
    let mut acc = *last;
    for &c in rest.iter().rev() {
        acc = fma(x, acc, c);
    }
    acc
}

/// `polyeval(x, a0, a1, ...)` on double-doubles.
pub(crate) fn polyeval_dd(x: DD, coeffs: &[DD]) -> DD {
    let (last, rest) = coeffs.split_last().expect("polyeval needs a coefficient");
    let mut acc = *last;
    for &c in rest.iter().rev() {
        acc = dd_multiply_add(x, acc, c);
    }
    acc
}

/// `fputil::nearest_integer(double)`: AArch64 `frintn`, round to nearest, ties to even.
pub(crate) fn nearest_integer(x: f64) -> f64 {
    x.round_ties_even()
}

// ------------------------------------------------------------ DyadicFloat<128>

const EXP_BIAS: i32 = 1023;

/// `fputil::DyadicFloat<128>` (`Float128`): `(-1)^neg * mantissa * 2^exponent`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct F128 {
    pub neg: bool,
    pub exponent: i32,
    pub mantissa: u128,
}

impl F128 {
    /// `DyadicFloat(Sign, int exponent, mantissa)`: normalizes.
    pub(crate) const fn new(neg: bool, exponent: i32, mantissa: u128) -> F128 {
        let mut f = F128 {
            neg,
            exponent,
            mantissa,
        };
        if mantissa != 0 {
            let shift = mantissa.leading_zeros();
            f.exponent -= shift as i32;
            f.mantissa <<= shift;
        }
        f
    }

    /// `DyadicFloat(T x)` for a double.
    pub(crate) fn from_f64(x: f64) -> F128 {
        let bits = x.to_bits();
        let neg = (bits >> 63) != 0;
        let biased = ((bits >> 52) & 0x7ff) as i32;
        let frac = bits & ((1u64 << 52) - 1);
        let (explicit_exp, explicit_man) = if biased == 0 {
            (1 - EXP_BIAS, frac)
        } else {
            (biased - EXP_BIAS, frac | (1u64 << 52))
        };
        F128::new(neg, explicit_exp - 52, explicit_man as u128)
    }

    /// `DyadicFloat(float x)`, from a value that is exactly a `float`.
    pub(crate) fn from_f32(x: f32) -> F128 {
        let bits = x.to_bits();
        let neg = (bits >> 31) != 0;
        let biased = ((bits >> 23) & 0xff) as i32;
        let frac = bits & ((1u32 << 23) - 1);
        let (explicit_exp, explicit_man) = if biased == 0 {
            (1 - 127, frac)
        } else {
            (biased - 127, frac | (1u32 << 23))
        };
        F128::new(neg, explicit_exp - 23, explicit_man as u128)
    }

    fn normalize(mut self) -> F128 {
        if self.mantissa != 0 {
            let shift = self.mantissa.leading_zeros();
            self.exponent -= shift as i32;
            self.mantissa <<= shift;
        }
        self
    }

    fn shift_right(&mut self, shift_length: u32) {
        if shift_length < 128 {
            self.exponent += shift_length as i32;
            self.mantissa >>= shift_length;
        } else {
            self.exponent = 0;
            self.mantissa = 0;
        }
    }

    fn shift_left(&mut self, shift_length: u32) {
        if shift_length < 128 {
            self.exponent -= shift_length as i32;
            self.mantissa <<= shift_length;
        } else {
            self.exponent = 0;
            self.mantissa = 0;
        }
    }

    /// `fast_as<double, false>()` (`static_cast<double>(Float128)`).
    pub(crate) fn to_f64(self) -> f64 {
        const SIG_MASK: u64 = (1u64 << 52) - 1;
        const IMPLICIT_MASK: u64 = 0; // SIG_MASK - FRACTION_MASK is 0 for binary64
        const PRECISION: u32 = 53;
        const BITS: u32 = 128;
        let sign_bit = (self.neg as u64) << 63;
        let create_value = |biased_exp: u64, mant: u64| -> f64 {
            f64::from_bits(sign_bit | (biased_exp << 52) | mant)
        };
        if self.mantissa == 0 {
            return f64::from_bits(sign_bit);
        }
        let mut exp_hi = self.exponent + (BITS as i32 - 1) + EXP_BIAS;
        if exp_hi > 2 * EXP_BIAS {
            let d_hi = create_value(2 * EXP_BIAS as u64, IMPLICIT_MASK);
            let two = 2.0f64;
            return two * d_hi;
        }
        let mut denorm = false;
        let mut shift: u32 = BITS - PRECISION;
        if exp_hi <= 0 {
            denorm = true;
            shift = (BITS - PRECISION) + (1 - exp_hi) as u32;
            exp_hi = EXP_BIAS;
        }
        let exp_lo = exp_hi - PRECISION as i32 - 1;
        let m_hi: u128 = if shift >= BITS {
            0
        } else {
            self.mantissa >> shift
        };
        let d_hi = create_value(exp_hi as u64, ((m_hi as u64) & SIG_MASK) | IMPLICIT_MASK);
        let round_mask: u128 = if shift - 1 >= BITS {
            0
        } else {
            1u128 << (shift - 1)
        };
        let sticky_mask: u128 = round_mask.wrapping_sub(1);
        let round_bit = (self.mantissa & round_mask) != 0;
        let sticky_bit = (self.mantissa & sticky_mask) != 0;
        let round_and_sticky = round_bit as i32 * 2 + sticky_bit as i32;
        if exp_lo <= 0 {
            let scale_up_exponent = 1 - exp_lo;
            let scale_up_factor = create_value_pos((EXP_BIAS + scale_up_exponent) as u64);
            let scale_down_factor = create_value_pos((EXP_BIAS - scale_up_exponent) as u64);
            let d_lo = create_value((exp_lo + scale_up_exponent) as u64, IMPLICIT_MASK);
            return fma(d_lo, round_and_sticky as f64, d_hi * scale_up_factor) * scale_down_factor;
        }
        let d_lo = create_value(exp_lo as u64, IMPLICIT_MASK);
        let r = fma(d_lo, round_and_sticky as f64, d_hi);
        if denorm {
            let clear_exp = (exp_hi as u64) << 52;
            let mut r_bits = r.to_bits().wrapping_sub(clear_exp);
            if r_bits & (0x7ffu64 << 52) == 0 {
                r_bits = r_bits.wrapping_sub(IMPLICIT_MASK);
            }
            return f64::from_bits(r_bits);
        }
        r
    }
}

fn create_value_pos(biased_exp: u64) -> f64 {
    f64::from_bits(biased_exp << 52)
}

/// `quick_add(a, b)`
pub(crate) fn quick_add(mut a: F128, mut b: F128) -> F128 {
    if a.mantissa == 0 {
        return b;
    }
    if b.mantissa == 0 {
        return a;
    }
    if a.exponent > b.exponent {
        b.shift_right((a.exponent - b.exponent) as u32);
    } else if b.exponent > a.exponent {
        a.shift_right((b.exponent - a.exponent) as u32);
    }
    let mut result = F128 {
        neg: false,
        exponent: 0,
        mantissa: 0,
    };
    if a.neg == b.neg {
        result.neg = a.neg;
        result.exponent = a.exponent;
        let (sum, overflow) = a.mantissa.overflowing_add(b.mantissa);
        result.mantissa = sum;
        if overflow {
            result.shift_right(1);
            result.mantissa |= 1u128 << 127;
        }
        return result;
    }
    if a.mantissa >= b.mantissa {
        result.neg = a.neg;
        result.exponent = a.exponent;
        result.mantissa = a.mantissa - b.mantissa;
    } else {
        result.neg = b.neg;
        result.exponent = b.exponent;
        result.mantissa = b.mantissa - a.mantissa;
    }
    result.normalize()
}

/// `quick_mul(a, b)`: `UInt<128>::quick_mul_hi` drops only the low x low word product.
pub(crate) fn quick_mul(a: F128, b: F128) -> F128 {
    let mut result = F128 {
        neg: a.neg != b.neg,
        exponent: a.exponent + b.exponent + 128,
        mantissa: 0,
    };
    if !(a.mantissa == 0 || b.mantissa == 0) {
        result.mantissa = quick_mul_hi(a.mantissa, b.mantissa);
        if result.mantissa >> 127 == 0 {
            result.shift_left(1);
        }
    }
    result
}

/// `multiword::quick_mul_hi` for two 64-bit words.
fn quick_mul_hi(lhs: u128, rhs: u128) -> u128 {
    const M: u128 = (1u128 << 64) - 1;
    let (l0, l1) = (lhs & M, lhs >> 64);
    let (r0, r1) = (rhs & M, rhs >> 64);
    // all the partial products except l0 * r0, truncated to the top 128 bits
    let a = l0 * r1;
    let b = l1 * r0;
    let lo = (a & M) + (b & M);
    let hi = (a >> 64) + (b >> 64) + (lo >> 64);
    (l1 * r1).wrapping_add(hi)
}

/// `multiply_add(a, b, c)` on `DyadicFloat`: `quick_add(c, quick_mul(a, b))`
pub(crate) fn f128_multiply_add(a: F128, b: F128, c: F128) -> F128 {
    quick_add(c, quick_mul(a, b))
}

/// `polyeval(x, a0, a1, ...)` on `Float128`.
pub(crate) fn polyeval_f128(x: F128, coeffs: &[F128]) -> F128 {
    let (last, rest) = coeffs.split_last().expect("polyeval needs a coefficient");
    let mut acc = *last;
    for &c in rest.iter().rev() {
        acc = f128_multiply_add(x, acc, c);
    }
    acc
}
