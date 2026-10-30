import AppKit
import SwiftUI

/// Support Fins ▸ Acknowledgements…: the notices OpenCascade (LGPL-2.1, with its relinking note), uniffi (MPL-2.0), Manifold
/// (Apache-2.0) and every Rust crate that ships require. The text is `Resources/Acknowledgements.txt`, written by
/// `tools/licenses/gen.py` from Cargo.lock (the gate fails when it is stale); the licence texts stay English.
enum Acknowledgements {
    static let windowID = "acknowledgements"

    /// The file's text, or a one-line pointer when the bundle lacks it (a test host).
    static func text(in bundle: Bundle = .main) -> String {
        guard let url = bundle.url(forResource: "Acknowledgements", withExtension: "txt"),
              let text = try? String(contentsOf: url, encoding: .utf8) else {
            return "https://github.com/storskegg/support-fins-rs"
        }
        return text
    }
}

/// The one window of the notices (a `Window` scene: opening it again fronts it).
struct AcknowledgementsScene: Scene {
    var body: some Scene {
        Window(JS.t("Acknowledgements"), id: Acknowledgements.windowID) {
            AcknowledgementsView()
                .frame(minWidth: 480, minHeight: 320)
        }
        .defaultSize(width: 680, height: 600)
    }
}

struct AcknowledgementsView: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Support Fins is built on open-source software. Its licences ask for these notices.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding([.top, .horizontal], 12)
            NoticesTextView(text: Acknowledgements.text())
        }
    }
}

/// A scrolling, selectable text view: the file is half a megabyte, too much for a SwiftUI `Text`.
private struct NoticesTextView: NSViewRepresentable {
    let text: String

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        guard let view = scroll.documentView as? NSTextView else { return scroll }
        view.isEditable = false
        view.isSelectable = true
        view.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        view.textContainerInset = NSSize(width: 8, height: 8)
        view.string = text
        view.setAccessibilityLabel(JS.t("Acknowledgements"))
        // on the text view itself, not on a container (a container identifier hides what is inside it)
        view.setAccessibilityIdentifier("acknowledgements-text")
        return scroll
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {}
}

/// The menu item; a view of its own because `openWindow` is an environment value.
struct AcknowledgementsCommand: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button(JS.t("Acknowledgements…")) { openWindow(id: Acknowledgements.windowID) }
    }
}
