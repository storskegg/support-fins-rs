import AppKit
import SwiftUI

/// Content whose clicks work on the first click, in a window that is not key: AppKit sends that click to the view
/// under the pointer only when the view `acceptsFirstMouse`, and the window's SwiftUI view does not, so a plain button
/// there only activates the window. For a strip of buttons a user expects to work at once (the crash banner).
/// The content is hosted in a view of its own and gets the surrounding environment.
struct FirstMouse<Content: View>: NSViewRepresentable {
    @ViewBuilder var content: () -> Content

    func makeNSView(context: Context) -> FirstMouseHostingView {
        let view = FirstMouseHostingView(rootView: AnyView(content().environment(\.self, context.environment)))
        view.sizingOptions = [.intrinsicContentSize]
        return view
    }

    func updateNSView(_ view: FirstMouseHostingView, context: Context) {
        view.rootView = AnyView(content().environment(\.self, context.environment))
    }

    /// The content's own size, not the space offered (a banner keeps to its strip and its alignment).
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: FirstMouseHostingView, context: Context) -> CGSize? {
        nsView.fittingSize
    }

    @MainActor
    final class FirstMouseHostingView: NSHostingView<AnyView> {
        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    }
}
