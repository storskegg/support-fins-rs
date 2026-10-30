//! Port of LLVM libc `log_range_reduction` (src/__support/math/log_range_reduction.h), the
//! 128-bit range reduction shared by the accurate passes of `log`, `log2` and `log10`.

use super::fputil::{F128, quick_add};
use super::tables_common::{S2, S3, S4};

/// `LogRR`: the four step tables of one logarithm.
pub(crate) struct LogRR {
    pub step_1: &'static [F128],
    pub step_2: &'static [F128],
    pub step_3: &'static [F128],
    pub step_4: &'static [F128],
}

/// `log_range_reduction(m_x, log_table, sum)`
pub(crate) fn log_range_reduction(m_x: f64, log_table: &LogRR, sum: &mut F128) -> F128 {
    let v: i64 = (m_x * 1.152921504606847e+18) as i64; // ulp = 2^-60

    let idx2 = ((v.wrapping_add(0x10_2000_0000_0000)) >> 46) as usize;
    *sum = quick_add(*sum, log_table.step_2[idx2]);

    let s2 = S2[idx2] as i64; // |s| <= 2^-7, ulp = 2^-16
    let sv2 = s2.wrapping_mul(v); // |s*v| < 2^-14, ulp = 2^(-60-16) = 2^-76
    let spv2 = (s2 << 44).wrapping_add(v); // |s + v| < 2^-14, ulp = 2^-60
    let vv2 = (spv2 << 16).wrapping_add(sv2); // |vv2| < 2^-14, ulp = 2^-76

    let idx3 = ((vv2.wrapping_add(0x2840_0000_0000_0000)) >> 55) as usize;
    *sum = quick_add(*sum, log_table.step_3[idx3]);

    let s3 = S3[idx3] as i64; // |s| < 2^-13, ulp = 2^-21
    let spv3 = (s3 << 55).wrapping_add(vv2); // |s + v| < 2^-21, ulp = 2^-76
    let sv3 = (s3 as i128).wrapping_mul(vv2 as i128);
    let vv3 = ((spv3 as i128) << 21).wrapping_add(sv3);

    let idx4 = (((vv3 >> 68) as i32 + 131) >> 1) as usize;
    *sum = quick_add(*sum, log_table.step_4[idx4]);

    let s4 = S4[idx4] as i128; // |s| < 2^-21, ulp = 2^-28
    let spv4 = (s4 << 69).wrapping_add(vv3);
    let sv4 = s4.wrapping_mul(vv3);
    let vv4 = (spv4 << 28).wrapping_add(sv4);

    if vv4 < 0 {
        F128::new(true, -125, vv4.wrapping_neg() as u128)
    } else {
        F128::new(false, -125, vv4 as u128)
    }
}
