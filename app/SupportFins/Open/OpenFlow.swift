import SwiftUI

/// The open flow's window plumbing: the picker sheet and the drop target (`#picker`, `#drop`).
@MainActor
enum OpenFlow {
    /// "Import a model" (`#empty-import`, `ui/io.js:473`): the Open panel; the file loads into this empty window.
    static func importModel(into model: ViewerModel) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = PartDocument.readableContentTypes
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        NSDocumentController.shared.noteNewRecentDocumentURL(url)
        Task { await model.load(url: url) }
    }
}

private struct OpenFlowModifier: ViewModifier {
    @Bindable var model: ViewerModel
    @Environment(\.openDocument) private var openDocument

    func body(content: Content) -> some View {
        content
            .sheet(item: $model.picker) { ObjectPickerSheet(model: $0) }
            .dropDestination(for: URL.self) { urls, _ in dropped(urls) }
    }

    /// A drop (`#drop`): an STL, OBJ or 3MF opens. An empty window takes it; a window with a part opens it in a
    /// new window, as File > Open does.
    private func dropped(_ urls: [URL]) -> Bool {
        guard let url = urls.first(where: OpenRules.accepts) else {
            model.status = OpenRules.Unsupported().localizedDescription
            return false
        }
        if model.hasPart {
            Task { _ = try? await openDocument(at: url) }
        } else {
            Task { await model.load(url: url) }
        }
        return true
    }
}

extension View {
    func openFlow(_ model: ViewerModel) -> some View { modifier(OpenFlowModifier(model: model)) }
}
