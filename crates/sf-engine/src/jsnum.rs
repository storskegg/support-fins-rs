//! JavaScript number semantics, bit for bit as V8 implements them.
//!
//! Every ported module does its arithmetic in plain `f64` (JS numbers are IEEE-754
//! doubles), but anywhere JS calls a `Math.*` function, an integer coercion or a
//! number-to-string conversion, the port calls the helper here instead of the Rust
//! standard library. The std versions differ in rounding direction, special values
//! or last-bit accuracy, and any one of those can flip a threshold the engine
//! decides on.
//!
//! The contract for every function: for every `f64` input (including -0, NaN, ±∞ and
//! subnormals), the output equals what V8 at Deno's shipped version returns, compared
//! by bits (`to_bits`), except that all NaNs are treated as equal. The tests in
//! `tests/jsnum.rs` check this against the values recorded from V8 in `parity/corpus/jsnum/`.

mod llvm;
mod text;
mod v8pow;
use text::{number_to_fixed, number_to_string, parse_float_impl, string_to_number_impl};
use v8pow::v8_pow;

// ---------------------------------------------------------------- rounding

/// `Math.round(x)`: rounds half toward +∞ (`Math.round(-2.5) == -2`), keeps -0
/// for inputs in [-0.5, -0], returns NaN/±∞ unchanged. NOT `f64::round`.
pub fn round(x: f64) -> f64 {
    // Float64Round (CodeStubAssembler): round up to the next integer, then step back down
    // one when that overshoots by more than half.
    let up = x.ceil();
    if up - 0.5 <= x { up } else { up - 1.0 }
}

/// `Math.trunc(x)`.
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

/// `Math.floor(x)`.
pub fn floor(x: f64) -> f64 {
    x.floor()
}

/// `Math.ceil(x)`.
pub fn ceil(x: f64) -> f64 {
    x.ceil()
}

/// `Math.sign(x)`: returns -0 for -0, NaN for NaN.
pub fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `Math.abs(x)`.
pub fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0x7fff_ffff_ffff_ffff)
}

// ------------------------------------------------------------- min / max

/// `Math.min(a, b)`: NaN if either is NaN, and -0 is less than +0.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        // -0 < +0: the OR of the sign bits is -0 if either is -0
        return f64::from_bits(a.to_bits() | b.to_bits());
    }
    if a < b { a } else { b }
}

/// `Math.max(a, b)`: NaN if either is NaN, and +0 is greater than -0.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        // +0 > -0: the AND of the sign bits is +0 if either is +0
        return f64::from_bits(a.to_bits() & b.to_bits());
    }
    if a > b { a } else { b }
}

/// `Math.min(...xs)`. `+∞` for an empty slice.
pub fn min_of(xs: &[f64]) -> f64 {
    let mut result = f64::INFINITY;
    for &v in xs {
        result = min(result, v);
    }
    result
}

/// `Math.max(...xs)`. `-∞` for an empty slice.
pub fn max_of(xs: &[f64]) -> f64 {
    let mut result = f64::NEG_INFINITY;
    for &v in xs {
        result = max(result, v);
    }
    result
}

// ------------------------------------------------------- transcendental

/// `Math.sqrt(x)` (IEEE correctly rounded, so `f64::sqrt` is acceptable underneath).
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// `Math.hypot(...xs)`. V8 does NOT compute `sqrt(sum of squares)` naively: it
/// scales by the largest magnitude and sums with compensation. Port V8's
/// algorithm exactly (see the brief), including ±∞ beating NaN and `hypot() == 0`.
pub fn hypot(xs: &[f64]) -> f64 {
    let length = xs.len();
    if length <= 3 {
        // FastMathHypot (math.tq): few arguments avoid the loop.
        if length == 0 {
            return 0.0;
        }
        let a = abs(xs[0]);
        if length == 1 {
            return a;
        }
        let b = abs(xs[1]);
        if length == 2 {
            if a == f64::INFINITY || b == f64::INFINITY {
                return f64::INFINITY;
            }
            let max = max(a, b);
            if max.is_nan() {
                return f64::NAN;
            }
            if max == 0.0 {
                return 0.0;
            }
            return ((a / max) * (a / max) + (b / max) * (b / max)).sqrt() * max;
        }
        let c = abs(xs[2]);
        if a == f64::INFINITY || b == f64::INFINITY || c == f64::INFINITY {
            return f64::INFINITY;
        }
        let max = max(max(a, b), c);
        if max.is_nan() {
            return f64::NAN;
        }
        if max == 0.0 {
            return 0.0;
        }
        let power_a = (a / max) * (a / max);
        let power_b = (b / max) * (b / max);
        let compensation = (power_a + power_b) - power_a - power_b;
        let power_c = (c / max) * (c / max) - compensation;
        return (power_a + power_b + power_c).sqrt() * max;
    }
    // MathHypot slow path
    let mut abs_values = vec![0.0f64; length];
    let mut one_arg_is_nan = false;
    let mut max = 0.0f64;
    for i in 0..length {
        let value = xs[i];
        if value.is_nan() {
            one_arg_is_nan = true;
        } else {
            let abs_value = abs(value);
            abs_values[i] = abs_value;
            if abs_value > max {
                max = abs_value;
            }
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    } else if one_arg_is_nan {
        return f64::NAN;
    } else if max == 0.0 {
        return 0.0;
    }
    // Kahan summation to avoid rounding errors.
    // Normalize the numbers to the largest one to avoid overflow.
    let mut sum = 0.0f64;
    let mut compensation = 0.0f64;
    for i in 0..length {
        let n = abs_values[i] / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `Math.sin(x)` (V8: fdlibm port, `src/base/ieee754.cc`).
pub fn sin(x: f64) -> f64 {
    llvm::sin(x)
}

/// `Math.cos(x)`.
pub fn cos(x: f64) -> f64 {
    llvm::cos(x)
}

/// `Math.tan(x)`.
pub fn tan(x: f64) -> f64 {
    llvm::tan(x)
}

/// `Math.asin(x)`.
pub fn asin(x: f64) -> f64 {
    llvm::asin(x)
}

/// `Math.acos(x)`.
pub fn acos(x: f64) -> f64 {
    llvm::acos(x)
}

/// `Math.atan(x)`.
pub fn atan(x: f64) -> f64 {
    llvm::atan(x)
}

/// `Math.atan2(y, x)`.
pub fn atan2(y: f64, x: f64) -> f64 {
    llvm::atan2(y, x)
}

/// `Math.exp(x)`.
pub fn exp(x: f64) -> f64 {
    llvm::exp(x)
}

/// `Math.log(x)`.
pub fn log(x: f64) -> f64 {
    llvm::log(x)
}

/// `Math.log2(x)`.
pub fn log2(x: f64) -> f64 {
    llvm::log2(x)
}

/// `Math.log10(x)`.
pub fn log10(x: f64) -> f64 {
    llvm::log10(x)
}

/// `Math.cbrt(x)`.
pub fn cbrt(x: f64) -> f64 {
    llvm::cbrt(x)
}

/// `Math.pow(x, y)` and `x ** y`. JS differs from C `pow` in special cases
/// (`pow(1, NaN)` is NaN in JS, 1 in C; `pow(-1, ±∞)` is NaN in JS).
pub fn pow(x: f64, y: f64) -> f64 {
    v8_pow(x, y)
}

// ------------------------------------------------------ integer coercion

/// ECMAScript ToInt32, i.e. `x | 0`: NaN/±∞ → 0, truncate, wrap modulo 2^32.
pub fn to_int32(x: f64) -> i32 {
    to_uint32(x) as i32
}

/// ECMAScript ToUint32, i.e. `x >>> 0`.
pub fn to_uint32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    // Truncate, then reduce modulo 2^32 (Rust's `%` on f64 is the exact C fmod).
    let m = x.trunc() % 4294967296.0;
    let m = if m < 0.0 { m + 4294967296.0 } else { m };
    m as u32
}

/// `Math.imul(a, b)` on numbers (each argument goes through ToInt32 first).
pub fn imul(a: f64, b: f64) -> i32 {
    to_int32(a).wrapping_mul(to_int32(b))
}

/// `a >>> b` on numbers: ToUint32(a) shifted right by `ToUint32(b) & 31`, as a number.
pub fn ushr(a: f64, b: f64) -> f64 {
    let shift = to_uint32(b) & 31;
    (to_uint32(a) >> shift) as f64
}

// ------------------------------------------------------- number → string

/// `String(x)` / `x.toString()` / template-literal `${x}` for a number: the shortest
/// round-trip digits, with JS's exponent rules (`1e21`, `1e-7`, `-0` → `"0"`, `NaN`, `Infinity`).
pub fn to_js_string(x: f64) -> String {
    number_to_string(x)
}

/// `x.toFixed(digits)` for `digits` in 0..=100. JS rounds the exact binary value
/// half up (`(1.005).toFixed(2) == "1.00"`), and `|x| >= 1e21` falls back to `to_js_string`.
pub fn to_fixed(x: f64, digits: u32) -> String {
    number_to_fixed(x, digits)
}

/// `Number(s)` on a string (ECMAScript StringToNumber): trims whitespace, the whole
/// string must be a numeric literal (`0x`/`0o`/`0b`, `Infinity`, empty → 0), else NaN.
/// `web/stl.js`'s ASCII reader uses this, not `parseFloat`.
pub fn string_to_number(s: &str) -> f64 {
    string_to_number_impl(s)
}

/// `parseFloat(s)` as JS parses it (leading whitespace, longest valid prefix, `Infinity`).
/// Returns NaN where JS does.
pub fn parse_float(s: &str) -> f64 {
    parse_float_impl(s)
}
