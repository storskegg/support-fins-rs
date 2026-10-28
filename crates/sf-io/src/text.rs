//! JS text helpers shared by the readers: `TextDecoder`, `\s` and `String.prototype.trim`.

/// JS `\s` (no `u` flag), which is also the set `String.prototype.trim` strips:
/// `[\t\n\v\f\r    -     　﻿]`.
pub(crate) fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `s.trim()`.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// `new TextDecoder().decode(bytes)`: UTF-8 with replacement, and (ignoreBOM is false) one
/// leading U+FEFF byte-order mark is dropped.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// `jsnum::string_to_number(tok)`, through Rust's correctly rounded `f64` parse when the token is
/// a plain decimal literal (digits, sign, point, exponent), which both grammars read alike.
pub(crate) fn fast_number(tok: &str) -> f64 {
    if tok
        .bytes()
        .all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E'))
        && let Ok(v) = tok.parse::<f64>()
        && v.is_finite()
    {
        return v;
    }
    sf_engine::jsnum::string_to_number(tok)
}

/// `Number(tok)` for a token that is usually a plain non-negative integer (a triangle's vertex
/// reference); NaN when absent.
pub(crate) fn index_number(tok: Option<&str>) -> f64 {
    let Some(tok) = tok else { return f64::NAN };
    let b = tok.as_bytes();
    if !b.is_empty() && b.len() <= 15 && b.iter().all(u8::is_ascii_digit) {
        return b.iter().fold(0u64, |v, c| v * 10 + (c - b'0') as u64) as f64;
    }
    fast_number(tok)
}

/// `+x || 0`: the number, or 0 for NaN and zero (-0 included).
pub(crate) fn number_or0(tok: Option<&str>) -> f64 {
    let n = tok.map_or(f64::NAN, fast_number);
    if n == 0.0 || n.is_nan() { 0.0 } else { n }
}
