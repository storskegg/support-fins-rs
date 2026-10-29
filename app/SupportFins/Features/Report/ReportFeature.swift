import SwiftUI
import SupportFinsCore

/// The report links under the status card (`#report-row`, `ui/report.js`) and Edit > Copy Build Settings.
@MainActor
final class ReportFeature: Feature {
    weak let host: ViewerModel?
    let model: ReportModel

    init(host: ViewerModel) {
        self.host = host
        model = ReportModel(host: host)
    }

    func close() { model.close() }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("report", slot: .top, order: 10) { ReportRow(model: self.model) }]
    }

    static func menus(_ f: ReportFeature?) -> [MenuSection] {
        [MenuSection(menu: .edit, order: 20, entries: [
            .divider,
            .item(JS.t("Copy Build Settings"), help: JS.t("Copy this build’s settings as JSON: with the model, they rebuild exactly these supports"),
                  enabled: f?.model.isAvailable == true) { f?.model.copySettings() },
        ])]
    }
}

/// `<p class="report-row">`: two link-style buttons, shown once a build is on screen.
struct ReportRow: View {
    let model: ReportModel

    var body: some View {
        if model.isAvailable {
            HStack(spacing: 12) {
                Button("Something didn’t get supported?") { model.openIssue() }
                    .help("Opens a GitHub issue filled in with this build’s settings and numbers (not your model: attach it there if you can)")
                    .accessibilityIdentifier("report-link")
                Button { model.copySettings() } label: {
                    switch model.copyState {
                    case .idle: Text("Copy settings")
                    case .copied: Text("Copied")
                    case .failed: Text("Copy failed")
                    }
                }
                .help("Copy this build’s settings as JSON: with the model, they rebuild exactly these supports")
                .accessibilityIdentifier("report-copy")
                Spacer(minLength: 0)
            }
            .buttonStyle(.link)
            .font(.caption)
            .padding(.horizontal, 4)
        }
    }
}
