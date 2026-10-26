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
