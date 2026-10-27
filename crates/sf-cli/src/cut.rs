//! `cutNegatives` (web/cut.js): a slicer's negative parts, cut out of the part. The one place the
//! CLI calls the cutter from, so that `sf-cut` (#72, Manifold) lands here and nowhere else.
//!
//! A part without a negative part is returned as it is, exactly as cut.js does.

use sf_io::threemf::ReadExtra;

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

/// Port of `cutNegatives(obj)` (web/cut.js:151-198) for `{ positions, extras }`: the cut is
/// `sf-cut`'s (Manifold 3.5.4). The CLI does not use the part's colours.
pub fn cut_negatives(positions: &[f64], extras: &[ReadExtra]) -> CutResult {
    let c = sf_cut::cut_negatives(positions, None, extras);
    CutResult {
        positions: c.positions,
        cut: c.cut,
        uncut: c.uncut,
        why: c.why,
    }
}
