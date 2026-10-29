import SwiftUI

/// The banner after a run that did not end normally, and Help > Show crash reports (`CrashReporter`, docs/FFI.md "Crash
/// reports"). The banner is non-modal: a strip over the top of the viewport.
@MainActor
final class CrashReportsFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    var overlays: [WindowOverlay] {
        [WindowOverlay("crash-banner", order: 80, alignment: .top) { CrashBanner(reporter: CrashReporter.shared, model: { self.host.openedURL }) }]
    }

    static func menus(_ f: CrashReportsFeature?) -> [MenuSection] {
        [MenuSection(menu: .help, order: 21, entries: [
            .item(JS.t("Show crash reports")) { CrashReporter.shared.revealFolder() },
        ])]
    }
}

struct CrashBanner: View {
    let reporter: CrashReporter
    let model: () -> URL?

    var body: some View {
        if reporter.showsBanner {
            HStack(spacing: 12) {
                Text("Support Fins closed unexpectedly last time.")
                    .accessibilityIdentifier("crash-banner-message")
                Button("Show Report") { reporter.revealReport() }
                    .accessibilityIdentifier("crash-banner-show")
                Button("Save crash report…") { reporter.saveReport(model: model()) }
                    .accessibilityIdentifier("crash-banner-save")
                Button("Dismiss") { reporter.dismiss() }
                    .accessibilityIdentifier("crash-banner-dismiss")
            }
            .font(.system(size: 13))
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.orange.opacity(0.7)))
            .shadow(radius: 12)
            .padding(.top, 8)
        }
    }
}
