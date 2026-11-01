import Foundation

/// The app's version and build number, as About Support Fins shows them: "2026.44.0 (1234)".
///
/// The version is YYYY.WW.PATCH (ISO week-year, ISO week, the week's release count; docs/AGENTS.md "Versioning and
/// releases"): `MARKETING_VERSION` in `app/Version.xcconfig`, which `tools/release/release.py` writes from Cargo.toml's
/// `[workspace.package] version`, the same version the engine reports (`engineVersion()`). The build number is the commit
/// count of HEAD, written into the built Info.plist by the app target's "Build number" script (app/project.yml). Both are
/// the app's own, not the web's: What's new lists the web's releases (`Changelog`).
enum AppVersion {
    /// `CFBundleShortVersionString`: "2026.44.0".
    static var version: String { string("CFBundleShortVersionString") }
    /// `CFBundleVersion`: "1234".
    static var build: String { string("CFBundleVersion") }
    /// "2026.44.0 (1234)": the crash reports' `appVersion`, the Help menu's line and the report link's site.
    static var display: String { "\(version) (\(build))" }

    private static func string(_ key: String) -> String {
        Bundle.main.object(forInfoDictionaryKey: key) as? String ?? "?"
    }
}
