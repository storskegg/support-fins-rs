//! `support-fins`: port of `plugins/cli` and the parts of `plugins/shared/engine` it uses. Read
//! `docs/AGENTS.md` before changing anything here.

pub mod cli;
pub mod entry;
pub mod options;

/// `VERSION` (plugins/cli/version.js): run from a checkout it is the unstamped base.
pub const VERSION: &str = "0.7.0";

/// JS `String.prototype.trim` (the `\s` set).
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| {
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
    })
}
