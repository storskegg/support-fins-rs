//! Port of `web/scale.js`: uniform scale for the part.

use sf_engine::jsnum;

/// Port of `scaleAbout` (web/scale.js:9-17): `to + s * (p - from)` for every xyz in `src`.
/// A new `Float32Array` in JS, so each value is rounded to f32 as stored. `to` defaults to
/// `from` in JS (`to = from`): callers pass it explicitly.
pub fn scale_about(src: &[f64], s: f64, from: [f64; 3], to: [f64; 3]) -> Vec<f64> {
    let mut out = vec![0.0; src.len()];
    let mut i = 0;
    while i < src.len() {
        // A source whose length is not a multiple of 3 reads `undefined` (NaN) past its end,
        // and the typed-array store past the end of `out` is ignored.
        for k in 0..3 {
            if i + k < out.len() {
                let v = src.get(i + k).copied().unwrap_or(f64::NAN);
                out[i + k] = ((to[k] + s * (v - from[k])) as f32) as f64;
            }
        }
        i += 3;
    }
    out
}

/// `MIN_PCT` (web/scale.js:21): below 10 % a fine mesh's vertices land closer than the weld's
/// 1 um grain (overhangs.js) and distinct vertices merge.
const MIN_PCT: f64 = 10.0;
/// `MAX_PCT` (web/scale.js:21).
const MAX_PCT: f64 = 1000.0;

/// Port of `scaleFromPercent` (web/scale.js:26-31): the percent field's value as a factor.
/// JS takes any value and applies `Number(v)`; this takes that number (use
/// `jsnum::string_to_number` for a string field). `None` is JS's `null`.
pub fn scale_from_percent(v: f64) -> Option<f64> {
    let n = v;
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    Some(jsnum::round(jsnum::min(MAX_PCT, jsnum::max(MIN_PCT, n)) * 100.0) / 10000.0)
}
