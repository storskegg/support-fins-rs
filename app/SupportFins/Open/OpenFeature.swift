import SwiftUI

/// The open flow: the empty window with its Import button, the status message, the object picker sheet and the
/// drop target.
@MainActor
final class OpenFeature: Feature {
    weak let host: ViewerModel?

    init(host: ViewerModel) { self.host = host }

    var overlays: [WindowOverlay] {
        [
            WindowOverlay("empty-state", order: 20) {
                if let host = self.host, host.showsEmptyState {
                    EmptyWindowView(compact: host.hasOpened) { OpenFlow.importModel(into: host) }
                }
            },
            WindowOverlay("status", order: 30) {
                ZStack(alignment: .topLeading) {
                    // `parseStep`'s spinner (`ui/io.js:313-320`): the CAD kernel takes a moment on a big assembly
                    if let host = self.host, host.isLoading, let url = host.loadingURL, OpenRules.isStep(url) {
                        ReadingStepSpinner()
                    }
                    if let status = self.host?.status {
                        Text(status).accessibilityIdentifier("load-status")
                            .padding(8).background(.regularMaterial, in: .rect(cornerRadius: 8)).padding()
                    }
                }
            },
        ]
    }

    var modifiers: [WindowModifier] {
        [WindowModifier("open-flow") { content in self.host.map { AnyView(content.openFlow($0)) } ?? AnyView(content) }]
    }
}

/// "reading STEP…" (`t('reading STEP…')`, `ui/io.js:327`), the build spinner's look.
struct ReadingStepSpinner: View {
    var body: some View {
        VStack {
            Spacer()
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("reading STEP…").font(.callout)
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .background(.regularMaterial, in: .capsule)
            .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity)
        .allowsHitTesting(false)
        .accessibilityLabel("reading STEP…")
        .accessibilityIdentifier("reading-step")
    }
}
