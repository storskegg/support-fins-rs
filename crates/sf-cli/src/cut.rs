//! `cutNegatives` (web/cut.js): a slicer's negative parts, cut out of the part. The one place the
//! CLI calls the cutter from, so that `sf-cut` (#72, Manifold) lands here and nowhere else.
//!
//! Until then a part that has a negative part comes back uncut with `why = "load"` ("the cutter
//! did not load"), the path cut.js itself has when its WASM cannot be fetched; a part without one is
//! returned as it is, exactly as cut.js does.

use sf_io::threemf::{PartKind, ReadExtra};

/// What `cutNegatives` returns (web/cut.js:158-162): `{ positions, cut, uncut, why }` (the part's
/// colours are not used by the CLI).
#[derive(Clone, Debug)]
pub struct CutResult {
    /// the cut part (the input's own positions when nothing was cut)
    pub positions: Vec<f64>,
    /// how many negative parts were cut
    pub cut: usize,
    /// how many were not
    pub uncut: usize,
    /// why NONE was, when that is the whole object's reason: `open`, `whole` or `load`
    pub why: Option<&'static str>,
}

/// Port of `cutNegatives(obj)` (web/cut.js:151-198) for `{ positions, extras }`.
pub fn cut_negatives(positions: &[f64], extras: &[ReadExtra]) -> CutResult {
    let negs: Vec<&ReadExtra> = extras
        .iter()
        .filter(|e| e.kind == PartKind::Negative)
        .collect();
    let same = CutResult {
        positions: positions.to_vec(),
        cut: 0,
        uncut: negs.len(),
        why: None,
    };
    if negs.is_empty() {
        return same;
    }
    // (the cutter cannot load: no Manifold in this build)
    CutResult {
        why: Some("load"),
        ..same
    }
}
