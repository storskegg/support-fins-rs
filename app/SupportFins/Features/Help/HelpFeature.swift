import AppKit
import SwiftUI

/// The Help menu's links (`index.html:174-179`, the top bar's GitHub and Ko-fi buttons) and Report a problem. "Support Fins Help"
/// itself is the system's item: it opens the Help Book (`Help/SupportFins.help`, built from the web's pages by
/// `tools/helpbook`) that `CFBundleHelpBookFolder` registers. Calibration Prints and the guide are `CalibrationFeature`'s.
@MainActor
final class HelpFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    /// The page's GitHub link, Ko-fi link and the issue form for a bug (`.github/ISSUE_TEMPLATE/bug-report.yml`).
    enum Link {
        static let github = URL(string: "https://github.com/gittrahan/support-fins")!
        static let kofi = URL(string: "https://ko-fi.com/matthewtrahan")!
        static let report = URL(string: "https://github.com/gittrahan/support-fins/issues/new?template=bug-report.yml")!
    }

    static func menus(_ f: HelpFeature?) -> [MenuSection] {
        [MenuSection(menu: .help, order: 20, entries: [
            .divider,
            .item(JS.t("Support Fins on GitHub"), help: JS.t("Source code on GitHub. Runs entirely in your browser; no file ever leaves your machine.")) {
                NSWorkspace.shared.open(Link.github)
            },
            .item(JS.t("Support the project"), help: JS.t("Support Fins is free and open source — buy me a coffee on Ko-fi.")) {
                NSWorkspace.shared.open(Link.kofi)
            },
            .item(JS.t("Report a problem")) { NSWorkspace.shared.open(Link.report) },
        ])]
    }
}
