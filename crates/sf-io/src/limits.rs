//! Hostile-input limits for the readers (audit X-02..X-07; docs/FFI.md "Load limits").
//!
//! Two kinds:
//! - **Always refused** (a plain error, no override): component nesting deeper than
//!   [`MAX_COMPONENT_DEPTH`], JSON nested deeper than [`MAX_JSON_DEPTH`], overlapping ZIP
//!   entries, non-finite coordinates.
//! - **Load anyway** ([`LimitExceeded`], overridden by [`LoadOptions::allow_large`]): more than
//!   [`MAX_TRIANGLES`] emitted triangles, more than [`MAX_DECOMPRESSED_BYTES`] unpacked in total,
//!   a largest dimension over [`MAX_DIMENSION_MM`]. The check runs before the expensive work, so
//!   `estimate` is what the file would cost, not what was allocated before giving up.

use std::fmt;

/// Emitted triangles (after component expansion) above which a load needs `allow_large`.
pub const MAX_TRIANGLES: u64 = 20_000_000;
/// Total decompressed ZIP bytes above which a load needs `allow_large` (2 GiB).
pub const MAX_DECOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Largest bounding-box dimension (mm) above which a load needs `allow_large` (2 m).
pub const MAX_DIMENSION_MM: f64 = 2000.0;
/// Component nesting deeper than this is refused (real files go 2-3 deep).
pub const MAX_COMPONENT_DEPTH: usize = 64;
/// A model part with more `<object>`s than this is refused: each costs about 2 KB parsed, so a
/// 1 MB archive of 200,000 one-line objects took 390 MB before a single triangle was read.
pub const MAX_OBJECTS_PER_PART: usize = 50_000;
/// JSON nesting deeper than this is refused (the app's sessions go under 10).
pub const MAX_JSON_DEPTH: usize = 128;
/// Stack size of the thread a load runs on (the Swift cooperative pool gives 512 KiB).
pub const LOAD_STACK_BYTES: usize = 8 * 1024 * 1024;

/// How a load is allowed to behave. `Default` is the safe one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadOptions {
    /// Skip the load-anyway limits (the user answered "Load anyway").
    pub allow_large: bool,
}

/// Which load-anyway limit was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// Emitted triangles; `estimate` and `limit` are counts.
    Triangles,
    /// Total decompressed bytes; `estimate` and `limit` are bytes.
    Bytes,
    /// Largest bounding-box dimension; `estimate` and `limit` are millimetres.
    Dimension,
}

/// A load-anyway limit that was exceeded: what, how much the file needs, and the limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitExceeded {
    pub kind: LimitKind,
    pub estimate: f64,
    pub limit: f64,
}

fn thousands(n: f64) -> String {
    let s = format!("{:.0}", n.min(1e18));
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn gigabytes(b: f64) -> String {
    format!("{:.1} GB", b / 1e9)
}

impl LimitExceeded {
    pub fn triangles(estimate: u64) -> Self {
        Self {
            kind: LimitKind::Triangles,
            estimate: estimate as f64,
            limit: MAX_TRIANGLES as f64,
        }
    }
    pub fn bytes(estimate: u64) -> Self {
        Self {
            kind: LimitKind::Bytes,
            estimate: estimate as f64,
            limit: MAX_DECOMPRESSED_BYTES as f64,
        }
    }
    pub fn dimension(mm: f64) -> Self {
        Self {
            kind: LimitKind::Dimension,
            estimate: mm,
            limit: MAX_DIMENSION_MM,
        }
    }
}

impl fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            LimitKind::Triangles => write!(
                f,
                "this file expands to about {} triangles (over {})",
                thousands(self.estimate),
                thousands(self.limit)
            ),
            LimitKind::Bytes => write!(
                f,
                "this file unpacks to about {} (over {})",
                gigabytes(self.estimate),
                gigabytes(self.limit)
            ),
            LimitKind::Dimension => write!(
                f,
                "this model is {} mm across (over {} mm)",
                if self.estimate.is_finite() && self.estimate < 1e15 {
                    thousands(self.estimate)
                } else {
                    format!("{:e}", self.estimate)
                },
                thousands(self.limit)
            ),
        }
    }
}

/// A reader's failure: a message (always refused), or a load-anyway limit.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadError {
    Msg(String),
    Limit(LimitExceeded),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Msg(m) => f.write_str(m),
            ReadError::Limit(l) => l.fmt(f),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<String> for ReadError {
    fn from(m: String) -> Self {
        ReadError::Msg(m)
    }
}

impl From<&str> for ReadError {
    fn from(m: &str) -> Self {
        ReadError::Msg(m.to_string())
    }
}

impl From<LimitExceeded> for ReadError {
    fn from(l: LimitExceeded) -> Self {
        ReadError::Limit(l)
    }
}

impl From<ReadError> for String {
    fn from(e: ReadError) -> String {
        e.to_string()
    }
}

/// The first non-finite coordinate in a flat position list, as an error.
pub fn check_finite(positions: &[f64], what: &str) -> Result<(), String> {
    use rayon::prelude::*;
    // a big soup is scanned in parallel; only the (rare) failure looks for where
    if positions
        .par_iter()
        .with_min_len(1 << 16)
        .all(|v| v.is_finite())
    {
        return Ok(());
    }
    match positions.iter().position(|v| !v.is_finite()) {
        None => Ok(()),
        Some(i) => Err(format!(
            "{what} has a coordinate that is not a number (triangle {}); the file is damaged",
            i / 9 + 1
        )),
    }
}

/// The largest side of the axis-aligned box of a flat position list (mm); 0 when empty.
/// Non-finite values are ignored here ([`check_finite`] refuses them).
pub fn largest_dimension(positions: &[f64]) -> f64 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in positions.as_chunks::<3>().0 {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (0..3)
        .map(|k| hi[k] - lo[k])
        .filter(|d| d.is_finite())
        .fold(0.0, f64::max)
}

/// Refuse a part larger than [`MAX_DIMENSION_MM`] unless `opts.allow_large`.
pub fn check_dimension(positions: &[f64], opts: &LoadOptions) -> Result<(), LimitExceeded> {
    if opts.allow_large {
        return Ok(());
    }
    let d = largest_dimension(positions);
    if d > MAX_DIMENSION_MM {
        Err(LimitExceeded::dimension(d))
    } else {
        Ok(())
    }
}
