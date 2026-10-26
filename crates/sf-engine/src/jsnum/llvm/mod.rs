//! V8 15.0.245's `Math.*` transcendental functions are LLVM libc's header-only `math::*`
//! implementations (revision 9309c117, vendored by V8 as third_party/llvm-libc), called from
//! `src/base/ieee754.cc`. Each submodule is a line-by-line port of one of them, built the way V8
//! builds them for arm64: FMA available, accurate (Ziv) pass enabled, round to nearest.

#![allow(
    clippy::approx_constant,
    clippy::excessive_precision,
    clippy::field_reassign_with_default,
    clippy::int_plus_one,
    clippy::unreadable_literal,
    clippy::unusual_byte_groupings
)]

mod asin;
mod atan;
mod cbrt;
mod exp;
mod fputil;
mod log;
mod log10;
mod log2;
mod log_rr;
mod sincos;
mod tables_common;
mod tables_exp;
mod tables_log;
mod tables_log10;
mod tables_log2;
mod tables_sincos;
mod tables_trig;

pub(crate) use asin::{acos, asin};
pub(crate) use atan::{atan, atan2};
pub(crate) use cbrt::cbrt;
pub(crate) use exp::exp;
pub(crate) use log::log;
pub(crate) use log2::log2;
pub(crate) use log10::log10;
pub(crate) use sincos::{cos, sin, tan};
