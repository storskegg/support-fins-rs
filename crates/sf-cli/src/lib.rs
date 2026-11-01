//! `support-fins`: port of `plugins/cli` and the parts of `plugins/shared/engine` it uses. Read
//! `docs/AGENTS.md` before changing anything here.

pub mod calibration;
pub mod cli;
pub mod cut;
pub mod entry;
pub mod options;
pub mod plateties;
pub mod project;
pub mod project_analysis;
pub mod project_cache;
pub mod project_io;
pub mod project_nozzle;
pub mod project_report;
pub mod project_xml;
pub mod uuid;

/// `VERSION` (plugins/cli/version.js): run from a checkout it is the unstamped base. `--version` prints it, as the JS
/// command line does (cli-parity compares it); the app's own version is [`build_info`]'s.
pub const VERSION: &str = "0.7.0";

/// `--build-info` (not in the JS): the port's own version, YYYY.WW.PATCH from Cargo.toml's `[workspace.package]`
/// (docs/AGENTS.md "Versioning and releases"), the pinned upstream commit and the web command line it matches, e.g.
/// `support-fins 2026.44.0 (upstream 23039e2, web CLI 0.7.0)`.
pub fn build_info() -> String {
    let commit = include_str!("../../../UPSTREAM")
        .lines()
        .find_map(|l| l.trim().strip_prefix("commit="))
        .unwrap_or("unknown");
    format!(
        "support-fins {} (upstream {}, web CLI {VERSION})",
        env!("CARGO_PKG_VERSION"),
        &commit[..commit.len().min(7)]
    )
}

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
