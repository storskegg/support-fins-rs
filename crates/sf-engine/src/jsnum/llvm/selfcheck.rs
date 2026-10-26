//! Self-checks for the 128-bit fallback passes. The recorded V8 cases almost never reach them
//! (Ziv's test passes first), so these tests force them and require the same answer as the
//! normal path: both are correctly rounded, so they must agree bit for bit.

use super::*;

/// xorshift64, fixed seed
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / 9007199254740992.0
    }
    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }
    fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        (lo.ln() + self.unit() * (hi.ln() - lo.ln())).exp()
    }
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Runs `normal` against both forced levels over `n` inputs from `gen`; returns the mismatches.
fn check(
    seed: u64,
    n: usize,
    gen_input: impl Fn(&mut Rng) -> f64,
    normal: impl Fn(f64) -> f64,
    forced: &[&dyn Fn(f64) -> f64],
) {
    let mut rng = Rng(seed);
    let mut bad = Vec::new();
    for _ in 0..n {
        let x = gen_input(&mut rng);
        let want = normal(x);
        for (level, f) in forced.iter().enumerate() {
            let got = f(x);
            if !same(got, want) {
                bad.push(format!(
                    "level {} x={x:?}: forced {got:?}, normal {want:?}",
                    level + 1
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches, first: {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

impl Rng {
    /// log-uniform magnitude in [lo, hi] with a random sign
    fn signed_log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let v = self.log_uniform(lo, hi);
        if self.next() & 1 == 0 { v } else { -v }
    }
}

#[test]
fn exp_accurate_passes_agree() {
    check(
        1,
        30_000,
        |r| r.uniform(-745.0, 709.0),
        exp,
        &[&exp_impl::<1>, &exp_impl::<2>],
    );
    check(
        2,
        10_000,
        |r| r.signed_log_uniform(1e-16, 1.0),
        exp,
        &[&exp_impl::<1>, &exp_impl::<2>],
    );
}

#[test]
fn log_accurate_passes_agree() {
    let gen_input = |r: &mut Rng| match r.next() % 3 {
        0 => r.log_uniform(1e-300, 1e300),
        1 => r.uniform(0.5, 2.0),
        _ => r.uniform(0.0, 100.0),
    };
    check(3, 30_000, gen_input, log, &[&log_impl::<1>]);
    check(4, 30_000, gen_input, log2, &[&log2_impl::<1>]);
    check(5, 30_000, gen_input, log10, &[&log10_impl::<1>]);
    // subnormals
    check(
        6,
        2_000,
        |r| f64::from_bits(r.next() & 0x000f_ffff_ffff_ffff),
        log,
        &[&log_impl::<1>],
    );
}

#[test]
fn trig_accurate_passes_agree() {
    let small = |r: &mut Rng| r.uniform(-10.0, 10.0);
    let large = |r: &mut Rng| r.signed_log_uniform(1e4, 1e300);
    check(7, 30_000, small, sin, &[&sin_impl::<1>]);
    check(8, 30_000, large, sin, &[&sin_impl::<1>]);
    check(9, 30_000, small, cos, &[&cos_impl::<1>]);
    check(10, 30_000, large, cos, &[&cos_impl::<1>]);
    check(11, 30_000, small, tan, &[&tan_impl::<1>]);
    check(12, 30_000, large, tan, &[&tan_impl::<1>]);
}

#[test]
fn inverse_trig_accurate_passes_agree() {
    let unit = |r: &mut Rng| r.uniform(-1.0, 1.0);
    let tiny = |r: &mut Rng| r.signed_log_uniform(1e-18, 0.5);
    check(13, 30_000, unit, asin, &[&asin_impl::<1>]);
    check(14, 30_000, tiny, asin, &[&asin_impl::<1>]);
    check(15, 30_000, unit, acos, &[&acos_impl::<1>]);
    check(16, 30_000, tiny, acos, &[&acos_impl::<1>]);
}

#[test]
fn cbrt_accurate_passes_agree() {
    check(
        17,
        30_000,
        |r| r.signed_log_uniform(1e-300, 1e300),
        cbrt,
        &[&cbrt_impl::<1>, &cbrt_impl::<2>],
    );
}
