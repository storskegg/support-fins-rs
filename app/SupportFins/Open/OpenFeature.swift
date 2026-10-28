import SwiftUI

/// The open flow: the empty window with its Import button, the status message, the object picker sheet and the
/// drop target.
@MainActor
final class OpenFeature: Feature {
    unowned let host: ViewerModel

    init(host: ViewerModel) { self.host = host }

    var overlays: [WindowOverlay] {
        [
            WindowOverlay("empty-state", order: 20) {
                if self.host.showsEmptyState {
                    EmptyWindowView(compact: self.host.hasOpened) { OpenFlow.importModel(into: self.host) }
                }
            },
            WindowOverlay("status", order: 30) {
                if let status = self.host.status {
                    Text(status).padding(8).background(.regularMaterial, in: .rect(cornerRadius: 8)).padding()
                }
            },
        ]
    }

    var modifiers: [WindowModifier] {
        [WindowModifier("open-flow") { content in AnyView(content.openFlow(self.host)) }]
    }
}
