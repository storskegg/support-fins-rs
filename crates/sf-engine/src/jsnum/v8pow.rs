//! `Math.pow` as V8 15.0.245 computes it.
//!
//! Port of `v8::internal::math::pow` (src/numbers/ieee754.cc:15-47 at the V8 tag). The final
//! `std::pow` call is `--use-std-math-pow`, which defaults to true on every platform except AIX,
//! so V8 does not run its own fdlibm/LLVM implementation here: it calls the C library's `pow`.
//! On macOS (the only platform this project targets and the one the oracle corpus was recorded
//! on) that is Apple's libsystem_m `pow`, whose arm64 source is not published. Rust's
//! `f64::powf` lowers to that same libm call, so it reproduces V8 exactly on macOS and only there.

/// `v8::internal::math::pow(x, y)`
pub(super) fn v8_pow(mut x: f64, y: f64) -> f64 {
    if y.is_nan() {
        // 1. If exponent is NaN, return NaN.
        return f64::NAN;
    }
    if y.is_infinite() && (x == 1.0 || x == -1.0) {
        // 9. If exponent is +inf, then b. If abs(base) = 1, return NaN.
        // and 10. If exponent is -inf, then b. If abs(base) = 1, return NaN.
        return f64::NAN;
    }
    if x.is_nan() {
        // libm pow distinguishes between quiet and signaling NaN; JS doesn't.
        x = f64::NAN;
    }

    // The following special cases just exist to match the optimizing compilers' behavior,
    // which avoid calls to `pow` in those cases.
    if y == 2.0 {
        // x ** 2   ==>   x * x
        return x * x;
    } else if y == 0.5 {
        // x ** 0.5   ==>  sqrt(x), except if x is -Infinity
        if x.is_infinite() {
            return f64::INFINITY;
        } else {
            // Note the +0 so that we get +0 for -0**0.5 rather than -0.
            return (x + 0.0).sqrt();
        }
    }

    // v8_flags.use_std_math_pow (default true): std::pow, the platform C library's pow.
    x.powf(y)
}
