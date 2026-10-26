//! STL read/write, ported from `web/stl.js`.
//!
//! M0 needs only `read_stl` (the tests load their fixtures through it). The
//! `writeBinarySTL` port is M1.

/// Errors `readSTL` throws in JS, as a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StlError(pub String);

/// `readSTL(bytes)` (`web/stl.js:80`): binary or ASCII STL → flat triangle-soup positions,
/// 9 per face. JS returns a `Float32Array`: every coordinate is rounded to f32. This
/// returns those same f32 values widened to f64 (`v as f64`), so the engine sees exactly
/// the numbers JS sees.
pub fn read_stl(bytes: &[u8]) -> Result<Vec<f64>, StlError> {
    let _ = bytes;
    todo!("M0 track D")
}
