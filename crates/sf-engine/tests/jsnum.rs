//! Direct tests of `sf_engine::jsnum`'s documented semantics (the recorded-V8 replay is in
//! `parity_jsnum.rs`). Every branch of every function has at least one assertion here.
//! Values that are -0 are checked with `to_bits`, NaN with `is_nan`.

// The literals below are exact doubles written out in full on purpose.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use sf_engine::jsnum::*;

const NAN: f64 = f64::NAN;
const INF: f64 = f64::INFINITY;
const NEG_INF: f64 = f64::NEG_INFINITY;

fn is_neg_zero(x: f64) -> bool {
    x.to_bits() == (-0.0f64).to_bits()
}
fn is_pos_zero(x: f64) -> bool {
    x.to_bits() == 0.0f64.to_bits()
}

// ---------------------------------------------------------------- rounding

#[test]
fn round_half_goes_toward_positive_infinity() {
    assert_eq!(round(2.5), 3.0);
    assert_eq!(round(1.5), 2.0);
    assert_eq!(round(0.5), 1.0);
    assert_eq!(round(-0.5 - 1.0), -1.0); // -1.5 -> -1
    assert_eq!(round(-2.5), -2.0);
    assert_eq!(round(-1.5), -1.0);
    assert_eq!(round(2.4), 2.0);
    assert_eq!(round(2.6), 3.0);
    assert_eq!(round(-2.6), -3.0);
}

#[test]
fn round_keeps_negative_zero() {
    assert!(is_neg_zero(round(-0.4)));
    assert!(is_neg_zero(round(-0.5)));
    assert!(is_neg_zero(round(-0.0)));
    assert!(is_pos_zero(round(0.0)));
    assert!(is_pos_zero(round(0.4)));
}

#[test]
fn round_special_and_large_values() {
    assert!(round(NAN).is_nan());
    assert_eq!(round(INF), INF);
    assert_eq!(round(NEG_INF), NEG_INF);
    // 0.49999999999999994 is the largest double below 0.5; floor(x + 0.5) would give 1
    assert!(is_pos_zero(round(0.49999999999999994)));
    assert_eq!(round(4503599627370497.0), 4503599627370497.0); // 2^52 + 1
    assert_eq!(round(-4503599627370497.0), -4503599627370497.0);
    assert_eq!(round(1e300), 1e300);
}

#[test]
fn trunc_floor_ceil_branches() {
    assert_eq!(trunc(2.7), 2.0);
    assert_eq!(trunc(-2.7), -2.0);
    assert!(is_neg_zero(trunc(-0.5)));
    assert!(is_neg_zero(trunc(-0.0)));
    assert!(trunc(NAN).is_nan());
    assert_eq!(trunc(INF), INF);
    assert_eq!(floor(2.7), 2.0);
    assert_eq!(floor(-2.2), -3.0);
    assert!(is_neg_zero(floor(-0.0)));
    assert!(is_pos_zero(floor(0.5)));
    assert!(floor(NAN).is_nan());
    assert_eq!(floor(NEG_INF), NEG_INF);
    assert_eq!(ceil(2.2), 3.0);
    assert_eq!(ceil(-2.7), -2.0);
    assert!(is_neg_zero(ceil(-0.5)));
    assert!(is_neg_zero(ceil(-0.0)));
    assert!(ceil(NAN).is_nan());
    assert_eq!(ceil(INF), INF);
}

#[test]
fn sign_branches() {
    assert_eq!(sign(3.5), 1.0);
    assert_eq!(sign(-3.5), -1.0);
    assert_eq!(sign(INF), 1.0);
    assert_eq!(sign(NEG_INF), -1.0);
    assert!(is_pos_zero(sign(0.0)));
    assert!(is_neg_zero(sign(-0.0)));
    assert!(sign(NAN).is_nan());
}

#[test]
fn abs_branches() {
    assert_eq!(abs(-3.5), 3.5);
    assert_eq!(abs(3.5), 3.5);
    assert!(is_pos_zero(abs(-0.0)));
    assert_eq!(abs(NEG_INF), INF);
    assert!(abs(NAN).is_nan());
    assert!(abs(-NAN).is_nan());
}

// ------------------------------------------------------------- min / max

#[test]
fn min_max_branches() {
    assert_eq!(min(1.0, 2.0), 1.0);
    assert_eq!(min(2.0, 1.0), 1.0);
    assert_eq!(max(1.0, 2.0), 2.0);
    assert_eq!(max(2.0, 1.0), 2.0);
    assert!(min(NAN, 1.0).is_nan());
    assert!(min(1.0, NAN).is_nan());
    assert!(max(NAN, 1.0).is_nan());
    assert!(max(1.0, NAN).is_nan());
    assert!(is_neg_zero(min(0.0, -0.0)));
    assert!(is_neg_zero(min(-0.0, 0.0)));
    assert!(is_neg_zero(min(-0.0, -0.0)));
    assert!(is_pos_zero(min(0.0, 0.0)));
    assert!(is_pos_zero(max(0.0, -0.0)));
    assert!(is_pos_zero(max(-0.0, 0.0)));
    assert!(is_neg_zero(max(-0.0, -0.0)));
    assert!(is_pos_zero(max(0.0, 0.0)));
    assert_eq!(min(NEG_INF, 1.0), NEG_INF);
    assert_eq!(max(INF, 1.0), INF);
}

#[test]
fn min_of_max_of_branches() {
    assert_eq!(min_of(&[]), INF);
    assert_eq!(max_of(&[]), NEG_INF);
    assert_eq!(min_of(&[3.0]), 3.0);
    assert_eq!(max_of(&[3.0]), 3.0);
    assert_eq!(min_of(&[3.0, 1.0, 2.0]), 1.0);
    assert_eq!(max_of(&[3.0, 1.0, 2.0]), 3.0);
    assert!(min_of(&[1.0, NAN, 0.0]).is_nan());
    assert!(max_of(&[1.0, NAN, 0.0]).is_nan());
    assert!(is_neg_zero(min_of(&[0.0, -0.0, 0.0])));
    assert!(is_pos_zero(max_of(&[-0.0, 0.0, -0.0])));
    assert!(is_neg_zero(min_of(&[-0.0])));
    assert!(is_pos_zero(max_of(&[0.0])));
}

// ------------------------------------------------------- transcendental

#[test]
fn sqrt_branches() {
    assert_eq!(sqrt(4.0), 2.0);
    assert!(sqrt(-1.0).is_nan());
    assert!(is_neg_zero(sqrt(-0.0)));
    assert_eq!(sqrt(INF), INF);
    assert!(sqrt(NAN).is_nan());
}

#[test]
fn hypot_empty_and_single() {
    assert!(is_pos_zero(hypot(&[])));
    assert_eq!(hypot(&[-3.0]), 3.0);
    assert!(hypot(&[NAN]).is_nan());
    assert_eq!(hypot(&[NEG_INF]), INF);
}

#[test]
fn hypot_two_arguments() {
    assert_eq!(hypot(&[3.0, 4.0]), 5.0);
    assert_eq!(hypot(&[-3.0, 4.0]), 5.0);
    assert_eq!(hypot(&[NAN, INF]), INF); // infinity beats NaN
    assert_eq!(hypot(&[INF, NAN]), INF);
    assert_eq!(hypot(&[NEG_INF, 1.0]), INF);
    assert!(hypot(&[NAN, 1.0]).is_nan());
    assert!(hypot(&[1.0, NAN]).is_nan());
    assert!(is_pos_zero(hypot(&[0.0, -0.0])));
    // scaling by the largest value keeps these finite and non-zero
    assert!(hypot(&[1e200, 1e200]).is_finite());
    assert!(hypot(&[1e-200, 1e-200]) > 0.0);
    assert!(hypot(&[1e308, 1e308]).is_finite());
}

#[test]
fn hypot_three_arguments() {
    assert_eq!(hypot(&[2.0, 3.0, 6.0]), 7.0);
    assert_eq!(hypot(&[NAN, INF, 1.0]), INF);
    assert_eq!(hypot(&[1.0, 2.0, NEG_INF]), INF);
    assert!(hypot(&[1.0, NAN, 2.0]).is_nan());
    assert!(is_pos_zero(hypot(&[0.0, 0.0, -0.0])));
    assert!(hypot(&[1e308, 1e308, 1e308]).is_finite());
}

#[test]
fn hypot_four_or_more_arguments() {
    assert_eq!(hypot(&[1.0, 2.0, 2.0, 4.0]), 5.0);
    assert_eq!(hypot(&[NAN, INF, 1.0, 1.0]), INF);
    assert_eq!(hypot(&[1.0, 1.0, 1.0, NEG_INF]), INF);
    assert!(hypot(&[1.0, 1.0, NAN, 1.0]).is_nan());
    assert!(is_pos_zero(hypot(&[0.0, -0.0, 0.0, 0.0])));
    assert_eq!(hypot(&[1e308, 1e308, 1e308, 1e308]), INF); // 2e308 overflows
    assert!(hypot(&[1e308, 0.0, 0.0, 0.0]).is_finite());
    assert_eq!(hypot(&[-2.0, -2.0, -2.0, -2.0]), 4.0);
}

#[test]
fn trig_special_values() {
    assert!(is_pos_zero(sin(0.0)));
    assert!(is_neg_zero(sin(-0.0)));
    assert!(sin(INF).is_nan());
    assert!(sin(NEG_INF).is_nan());
    assert!(sin(NAN).is_nan());
    assert_eq!(sin(std::f64::consts::FRAC_PI_2), 1.0);
    assert_eq!(cos(0.0), 1.0);
    assert_eq!(cos(-0.0), 1.0);
    assert!(cos(INF).is_nan());
    assert!(cos(NAN).is_nan());
    assert_eq!(cos(std::f64::consts::PI), -1.0);
    assert!(is_pos_zero(tan(0.0)));
    assert!(is_neg_zero(tan(-0.0)));
    assert!(tan(INF).is_nan());
    assert!(tan(NAN).is_nan());
    // tiny arguments take the x_e < EXP_BIAS - 26 shortcut
    assert_eq!(sin(1e-300), 1e-300);
    assert_eq!(tan(1e-300), 1e-300);
    assert_eq!(cos(1e-300), 1.0);
    // 10^22 and the largest finite double go through the large-argument range reduction
    assert!(sin(1e22).abs() <= 1.0);
    assert!(sin(f64::MAX).abs() <= 1.0);
    assert_eq!(sin(1e22), -0.8522008497671888);
}

#[test]
fn inverse_trig_special_values() {
    assert!(is_pos_zero(asin(0.0)));
    assert!(is_neg_zero(asin(-0.0)));
    assert_eq!(asin(1.0), std::f64::consts::FRAC_PI_2);
    assert_eq!(asin(-1.0), -std::f64::consts::FRAC_PI_2);
    assert!(asin(1.5).is_nan());
    assert!(asin(NAN).is_nan());
    assert!(asin(INF).is_nan());
    assert_eq!(asin(0.5), 0.5235987755982989);
    assert_eq!(acos(1.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(acos(-1.0), std::f64::consts::PI);
    assert_eq!(acos(0.0), std::f64::consts::FRAC_PI_2);
    assert!(acos(-1.5).is_nan());
    assert!(acos(NAN).is_nan());
    assert_eq!(acos(0.5), 1.0471975511965979);
    assert_eq!(acos(-0.5), 2.0943951023931957);
    assert!(is_pos_zero(atan(0.0)));
    assert!(is_neg_zero(atan(-0.0)));
    assert_eq!(atan(INF), std::f64::consts::FRAC_PI_2);
    assert_eq!(atan(NEG_INF), -std::f64::consts::FRAC_PI_2);
    assert!(atan(NAN).is_nan());
    assert_eq!(atan(1.0), std::f64::consts::FRAC_PI_4);
    assert_eq!(atan(-1.0), -std::f64::consts::FRAC_PI_4);
    assert_eq!(atan(1e300), std::f64::consts::FRAC_PI_2);
}

#[test]
fn atan2_special_values() {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    assert!(is_pos_zero(atan2(0.0, 1.0)));
    assert!(is_neg_zero(atan2(-0.0, 1.0)));
    assert_eq!(atan2(0.0, -0.0), PI);
    assert_eq!(atan2(-0.0, -0.0), -PI);
    assert_eq!(atan2(0.0, -1.0), PI);
    assert_eq!(atan2(-0.0, -1.0), -PI);
    assert!(is_pos_zero(atan2(0.0, 0.0)));
    assert!(is_neg_zero(atan2(-0.0, 0.0)));
    assert_eq!(atan2(1.0, 0.0), FRAC_PI_2);
    assert_eq!(atan2(-1.0, 0.0), -FRAC_PI_2);
    assert_eq!(atan2(1.0, -0.0), FRAC_PI_2);
    assert_eq!(atan2(INF, INF), FRAC_PI_4);
    assert_eq!(atan2(INF, NEG_INF), 3.0 * FRAC_PI_4);
    assert_eq!(atan2(-INF, NEG_INF), -3.0 * FRAC_PI_4);
    assert_eq!(atan2(1.0, INF), 0.0);
    assert_eq!(atan2(1.0, NEG_INF), PI);
    assert_eq!(atan2(INF, 1.0), FRAC_PI_2);
    assert!(atan2(NAN, 1.0).is_nan());
    assert!(atan2(1.0, NAN).is_nan());
    assert_eq!(atan2(1.0, 1.0), FRAC_PI_4);
    assert_eq!(atan2(-1.0, -1.0), -3.0 * FRAC_PI_4);
    // very different magnitudes: the exp_diff > 54 shortcut and the scaling of subnormals
    assert_eq!(atan2(1.0, 1e300), 1e-300);
    assert_eq!(atan2(5e-324, 1.0), 5e-324);
    assert_eq!(atan2(5e-324, 5e-324), FRAC_PI_4);
    assert_eq!(atan2(1e300, 1e300), FRAC_PI_4);
    // V8 returns +0 here (not -0): the recorded behaviour of LLVM libc's atan2
    assert!(is_pos_zero(atan2(-5e-324, f64::MAX)));
}

#[test]
fn exp_branches() {
    assert_eq!(exp(0.0), 1.0);
    assert_eq!(exp(-0.0), 1.0);
    assert_eq!(exp(1.0), std::f64::consts::E);
    assert_eq!(exp(NEG_INF), 0.0);
    assert_eq!(exp(INF), INF);
    assert!(exp(NAN).is_nan());
    assert_eq!(exp(710.0), INF); // overflow
    assert_eq!(exp(-746.0), 0.0); // underflow
    assert_eq!(exp(-745.0), 5e-324); // denormal result
    assert_eq!(exp(1e-20), 1.0); // |x| <= 2^-53
    assert_eq!(exp(-1e-20), 1.0);
    assert_eq!(exp(709.782712893384), 1.7976931348622732e308);
}

#[test]
fn log_family_branches() {
    assert_eq!(log(1.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(log(0.0), NEG_INF);
    assert_eq!(log(-0.0), NEG_INF);
    assert!(log(-1.0).is_nan());
    assert!(log(NEG_INF).is_nan());
    assert_eq!(log(INF), INF);
    assert!(log(NAN).is_nan());
    assert_eq!(log(std::f64::consts::E), 1.0);
    assert_eq!(log(5e-324), -744.4400719213812); // subnormal input
    assert_eq!(log2(1.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(log2(0.0), NEG_INF);
    assert!(log2(-1.0).is_nan());
    assert_eq!(log2(INF), INF);
    assert!(log2(NAN).is_nan());
    assert_eq!(log2(8.0), 3.0);
    assert_eq!(log2(0.125), -3.0);
    assert_eq!(log2(5e-324), -1074.0);
    assert_eq!(log10(1.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(log10(0.0), NEG_INF);
    assert!(log10(-1.0).is_nan());
    assert_eq!(log10(INF), INF);
    assert!(log10(NAN).is_nan());
    assert_eq!(log10(1000.0), 3.0);
    assert_eq!(log10(1e22), 22.0);
    assert_eq!(log10(1e-5), -5.0);
    assert_eq!(log10(5e-324), -323.3062153431158);
}

#[test]
fn cbrt_branches() {
    assert_eq!(cbrt(27.0), 3.0);
    assert_eq!(cbrt(-8.0), -2.0);
    assert!(is_pos_zero(cbrt(0.0)));
    assert!(is_neg_zero(cbrt(-0.0)));
    assert_eq!(cbrt(INF), INF);
    assert_eq!(cbrt(NEG_INF), NEG_INF);
    assert!(cbrt(NAN).is_nan());
    assert_eq!(cbrt(5e-324), 1.7031839360032603e-108); // subnormal input
    assert_eq!(cbrt(f64::MAX), 5.643803094122362e102);
    assert_eq!(cbrt(1.0), 1.0);
}

#[test]
fn pow_javascript_special_cases() {
    assert!(pow(1.0, NAN).is_nan()); // 1 in C
    assert!(pow(-1.0, INF).is_nan());
    assert!(pow(1.0, INF).is_nan());
    assert!(pow(1.0, NEG_INF).is_nan());
    assert!(pow(-1.0, NEG_INF).is_nan());
    assert!(pow(NAN, NAN).is_nan());
    assert_eq!(pow(NAN, 0.0), 1.0);
    assert!(pow(NAN, 1.0).is_nan());
    assert_eq!(pow(0.0, 0.0), 1.0);
    assert_eq!(pow(3.0, 2.0), 9.0); // the x * x shortcut
    assert_eq!(pow(-3.0, 2.0), 9.0);
    assert_eq!(pow(INF, 2.0), INF);
    assert_eq!(pow(2.0, 0.5), std::f64::consts::SQRT_2);
    assert_eq!(pow(NEG_INF, 0.5), INF);
    assert_eq!(pow(INF, 0.5), INF);
    assert!(is_pos_zero(pow(-0.0, 0.5))); // sqrt(x + 0), not -0
    assert!(pow(-4.0, 0.5).is_nan());
    assert_eq!(pow(2.0, 10.0), 1024.0);
    assert_eq!(pow(2.0, -1.0), 0.5);
    assert_eq!(pow(10.0, 21.0), 1e21);
    assert_eq!(pow(0.0, -1.0), INF);
    assert_eq!(pow(-0.0, -1.0), NEG_INF);
    assert_eq!(pow(2.0, INF), INF);
    assert_eq!(pow(2.0, NEG_INF), 0.0);
    assert_eq!(pow(0.5, INF), 0.0);
    assert!(pow(-8.0, 1.0 / 3.0).is_nan());
    assert_eq!(pow(-2.0, 3.0), -8.0);
}

// ------------------------------------------------------ integer coercion

#[test]
fn to_int32_branches() {
    assert_eq!(to_int32(4294967296.5), 0);
    assert_eq!(to_int32(2147483648.0), -2147483648);
    assert_eq!(to_int32(2147483647.9), 2147483647);
    assert_eq!(to_int32(-2147483648.0), -2147483648);
    assert_eq!(to_int32(-2147483649.0), 2147483647);
    assert_eq!(to_int32(-1.9), -1);
    assert_eq!(to_int32(1.9), 1);
    assert_eq!(to_int32(NAN), 0);
    assert_eq!(to_int32(INF), 0);
    assert_eq!(to_int32(NEG_INF), 0);
    assert_eq!(to_int32(-0.0), 0);
    assert_eq!(to_int32(4294967295.0), -1);
    assert_eq!(to_int32(1e21), -559939584);
    assert_eq!(to_int32(9007199254740993.0), 0);
    assert_eq!(to_int32(f64::MAX), 0);
}

#[test]
fn to_uint32_branches() {
    assert_eq!(to_uint32(-1.0), 4294967295);
    assert_eq!(to_uint32(4294967296.0), 0);
    assert_eq!(to_uint32(4294967297.9), 1);
    assert_eq!(to_uint32(-0.5), 0);
    assert_eq!(to_uint32(NAN), 0);
    assert_eq!(to_uint32(INF), 0);
    assert_eq!(to_uint32(2147483648.0), 2147483648);
    assert_eq!(to_uint32(-2147483648.0), 2147483648);
}

#[test]
fn imul_branches() {
    assert_eq!(imul(4294967295.0, 5.0), -5);
    assert_eq!(imul(2.0, 3.0), 6);
    assert_eq!(imul(65536.0, 65536.0), 0);
    assert_eq!(imul(2147483647.0, 2.0), -2);
    assert_eq!(imul(NAN, 5.0), 0);
    assert_eq!(imul(5.0, INF), 0);
    assert_eq!(imul(-3.7, 2.2), -6);
    assert_eq!(imul(-2147483648.0, -1.0), -2147483648);
}

#[test]
fn ushr_branches() {
    assert_eq!(ushr(-1.0, 0.0), 4294967295.0);
    assert_eq!(ushr(-1.0, 1.0), 2147483647.0);
    assert_eq!(ushr(-1.0, 31.0), 1.0);
    assert_eq!(ushr(-1.0, 32.0), 4294967295.0); // shift count is masked with 31
    assert_eq!(ushr(-1.0, 33.0), 2147483647.0);
    assert_eq!(ushr(8.0, -1.0), 0.0); // -1 & 31 == 31
    assert_eq!(ushr(NAN, 3.0), 0.0);
    assert_eq!(ushr(4294967296.0 + 6.0, 1.0), 3.0);
    assert_eq!(ushr(16.0, NAN), 16.0);
}

// ------------------------------------------------------- number -> string

#[test]
fn to_js_string_special_values() {
    assert_eq!(to_js_string(NAN), "NaN");
    assert_eq!(to_js_string(INF), "Infinity");
    assert_eq!(to_js_string(NEG_INF), "-Infinity");
    assert_eq!(to_js_string(0.0), "0");
    assert_eq!(to_js_string(-0.0), "0");
}

#[test]
fn to_js_string_placement_rules() {
    assert_eq!(to_js_string(1.0), "1");
    assert_eq!(to_js_string(-1.5), "-1.5");
    assert_eq!(to_js_string(123.456), "123.456");
    assert_eq!(to_js_string(100.0), "100");
    assert_eq!(to_js_string(1e20), "100000000000000000000");
    assert_eq!(
        to_js_string(123456789012345680000.0),
        "123456789012345680000"
    );
    assert_eq!(to_js_string(1e21), "1e+21");
    assert_eq!(to_js_string(-1e21), "-1e+21");
    assert_eq!(to_js_string(1.5e21), "1.5e+21");
    assert_eq!(to_js_string(0.000001), "0.000001");
    assert_eq!(to_js_string(1e-7), "1e-7");
    assert_eq!(to_js_string(1.5e-7), "1.5e-7");
    assert_eq!(to_js_string(0.0000015), "0.0000015");
    assert_eq!(to_js_string(5e-324), "5e-324");
    assert_eq!(to_js_string(f64::MAX), "1.7976931348623157e+308");
    assert_eq!(to_js_string(0.1 + 0.2), "0.30000000000000004");
    assert_eq!(to_js_string(9007199254740992.0), "9007199254740992");
    assert_eq!(to_js_string(2.5e-7), "2.5e-7");
}

#[test]
fn to_js_string_even_digit_on_exact_ties() {
    // exactly halfway between two 17-digit decimals: JS picks the one with the even last digit
    assert_eq!(to_js_string(234419603297181.625), "234419603297181.62");
    assert_eq!(to_js_string(-234419603297181.625), "-234419603297181.62");
    assert_eq!(to_js_string(174612688907493.125), "174612688907493.12");
}

#[test]
fn to_fixed_basics() {
    assert_eq!(to_fixed(1.005, 2), "1.00"); // the double is below 1.005
    assert_eq!(to_fixed(1.255, 2), "1.25");
    assert_eq!(to_fixed(2.5, 0), "3");
    assert_eq!(to_fixed(1.5, 0), "2");
    assert_eq!(to_fixed(0.5, 0), "1");
    assert_eq!(to_fixed(-2.5, 0), "-3"); // ties round away from zero (up in magnitude)
    assert_eq!(to_fixed(123.456, 1), "123.5");
    assert_eq!(to_fixed(0.0, 2), "0.00");
    assert_eq!(to_fixed(-0.0, 2), "0.00");
    assert_eq!(to_fixed(10.0, 3), "10.000");
    assert_eq!(to_fixed(1.0 / 3.0, 20), "0.33333333333333331483");
    assert_eq!(to_fixed(0.1, 20), "0.10000000000000000555");
}

#[test]
fn to_fixed_signs_and_small_values() {
    assert_eq!(to_fixed(-0.0001, 2), "-0.00");
    assert_eq!(to_fixed(-1.45, 1), "-1.4");
    assert_eq!(to_fixed(0.000001, 5), "0.00000");
    assert_eq!(to_fixed(0.00000051, 6), "0.000001");
    assert_eq!(to_fixed(5e-324, 20), "0.00000000000000000000");
    assert_eq!(to_fixed(5e-324, 100).len(), 102);
}

#[test]
fn to_fixed_large_and_special_values() {
    assert_eq!(to_fixed(NAN, 2), "NaN");
    assert_eq!(to_fixed(INF, 2), "Infinity");
    assert_eq!(to_fixed(NEG_INF, 2), "-Infinity");
    assert_eq!(to_fixed(1e21, 2), "1e+21"); // |x| >= 1e21 falls back to String(x)
    assert_eq!(to_fixed(-1e21, 2), "-1e+21");
    assert_eq!(to_fixed(1e20, 2), "100000000000000000000.00");
    assert_eq!(
        to_fixed(123456789012345680000.0, 0),
        "123456789012345683968"
    );
    assert_eq!(to_fixed(2f64.powi(60), 3), "1152921504606846976.000");
    assert_eq!(to_fixed(4503599627370495.5, 0), "4503599627370496"); // exact tie rounds up
    assert_eq!(to_fixed(1e-10, 10), "0.0000000001");
}

// ------------------------------------------------------- string -> number

#[test]
fn string_to_number_trimming_and_empty() {
    assert_eq!(string_to_number(""), 0.0);
    assert_eq!(string_to_number("   "), 0.0);
    assert_eq!(
        string_to_number("\t\n\r\u{b}\u{c} 42 \u{a0}\u{feff}\u{2028}"),
        42.0
    );
    assert_eq!(string_to_number("  1.5e3 "), 1500.0);
    assert!(string_to_number("\u{85}1").is_nan()); // U+0085 is not JS whitespace
    assert!(string_to_number("\u{200b}1").is_nan());
}

#[test]
fn string_to_number_decimal_literals() {
    assert_eq!(string_to_number("42"), 42.0);
    assert_eq!(string_to_number("-42"), -42.0);
    assert_eq!(string_to_number("+42"), 42.0);
    assert!(is_neg_zero(string_to_number("-0")));
    assert!(is_pos_zero(string_to_number("+0")));
    assert_eq!(string_to_number(".5"), 0.5);
    assert_eq!(string_to_number("5."), 5.0);
    assert_eq!(string_to_number("5.e2"), 500.0);
    assert_eq!(string_to_number("-.5e1"), -5.0);
    assert_eq!(string_to_number("1e3"), 1000.0);
    assert_eq!(string_to_number("1E+3"), 1000.0);
    assert_eq!(string_to_number("1e-3"), 0.001);
    assert_eq!(string_to_number("00012"), 12.0);
    assert_eq!(string_to_number("1e1000"), INF);
    assert_eq!(string_to_number("-1e1000"), NEG_INF);
    assert!(is_pos_zero(string_to_number("1e-1000")));
    assert_eq!(string_to_number("0.1"), 0.1);
    assert_eq!(string_to_number("9007199254740993"), 9007199254740992.0);
    assert_eq!(string_to_number("4.9e-324"), 5e-324);
    assert_eq!(string_to_number("2e-324"), 0.0);
}

#[test]
fn string_to_number_rejects_what_js_rejects() {
    for s in [
        "1e",
        "1e+",
        ".",
        "+",
        "-",
        "1_000",
        "1 2",
        "1,5",
        "--1",
        "+-1",
        "abc",
        "12abc",
        "1.5.5",
        "NaN",
        "nan",
        "inf",
        "infinity",
        "Infinityx",
        "+ 1",
        "0x",
        "0xg",
        "-0x1F",
        "+0x1F",
        "0b2",
        "0o8",
        "0x1.8",
        ".e2",
        "e5",
        "1e5.5",
        "1\u{200b}",
    ] {
        assert!(string_to_number(s).is_nan(), "{s:?}");
    }
}

#[test]
fn string_to_number_infinity() {
    assert_eq!(string_to_number("Infinity"), INF);
    assert_eq!(string_to_number("+Infinity"), INF);
    assert_eq!(string_to_number("-Infinity"), NEG_INF);
    assert_eq!(string_to_number("  Infinity  "), INF);
}

#[test]
fn string_to_number_radix_prefixes() {
    assert_eq!(string_to_number("0x1F"), 31.0);
    assert_eq!(string_to_number("0X1f"), 31.0);
    assert_eq!(string_to_number("0o17"), 15.0);
    assert_eq!(string_to_number("0O17"), 15.0);
    assert_eq!(string_to_number("0b101"), 5.0);
    assert_eq!(string_to_number("0B101"), 5.0);
    assert!(is_pos_zero(string_to_number("0x0")));
    assert_eq!(string_to_number("0x00000000000000000000001"), 1.0); // leading zeros
    assert_eq!(
        string_to_number("0xFFFFFFFFFFFFF800"),
        18446744073709549568.0
    );
    // rounds to nearest even beyond 53 bits
    assert_eq!(
        string_to_number("0x20000000000001"),
        9007199254740992.0 + 0.0 + 0.0
    ); // 2^53 + 1 -> ties-to-even
    assert_eq!(string_to_number("0x20000000000003"), 9007199254740996.0);
    assert_eq!(
        string_to_number("0x20000000000001000000000000"),
        9007199254740992.0 * 2f64.powi(48)
    );
    assert_eq!(string_to_number(&format!("0x{}", "f".repeat(300))), INF);
    assert_eq!(
        string_to_number(&format!("0b{}", "1".repeat(53))),
        9007199254740991.0
    );
    assert_eq!(
        string_to_number(&format!("0b{}", "1".repeat(54))),
        18014398509481984.0
    );
}

#[test]
fn parse_float_prefixes() {
    assert_eq!(parse_float("3.14abc"), 3.14);
    assert_eq!(parse_float("  42  "), 42.0);
    assert_eq!(parse_float("42abc"), 42.0);
    assert_eq!(parse_float("1_000"), 1.0);
    assert_eq!(parse_float("1e"), 1.0);
    assert_eq!(parse_float("1e+"), 1.0);
    assert_eq!(parse_float("1.5e3x"), 1500.0);
    assert_eq!(parse_float(".5"), 0.5);
    assert_eq!(parse_float("5."), 5.0);
    assert_eq!(parse_float("-.5e1"), -5.0);
    assert_eq!(parse_float("1.5.5"), 1.5);
    assert_eq!(parse_float("1e5.5"), 100000.0);
    assert_eq!(parse_float("0x1F"), 0.0); // no hex in parseFloat
    assert_eq!(parse_float("Infinity"), INF);
    assert_eq!(parse_float("-Infinityx"), NEG_INF);
    assert_eq!(parse_float("+Infinity1"), INF);
    assert!(is_neg_zero(parse_float("-0")));
    assert!(is_neg_zero(parse_float("-0abc")));
}

#[test]
fn parse_float_rejects() {
    for s in [
        "",
        " ",
        "abc",
        ".",
        "+",
        "-",
        "e5",
        ".e2",
        "NaN",
        "infinity",
        "--1",
        "+-1",
        "\u{200b}1",
        "inf",
    ] {
        assert!(parse_float(s).is_nan(), "{s:?}");
    }
}

#[test]
fn number_string_round_trip() {
    for x in [
        0.1,
        1.0 / 3.0,
        123456.789,
        1e-7,
        1e21,
        5e-324,
        f64::MAX,
        4.35,
        8.345,
        -2.5e-9,
    ] {
        assert_eq!(string_to_number(&to_js_string(x)), x);
        assert_eq!(parse_float(&to_js_string(x)), x);
    }
}
