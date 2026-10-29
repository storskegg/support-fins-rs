import AppKit
import SwiftUI

/// A tip card (`web/ui/tips.js`, GH #351): a setting's tooltip under its 240 x 130 diagram (`web/ui/tippics.js`), shown
/// beside the inspector after the pointer rests on the setting for 350 ms (`HOVER_MS`), hidden when it leaves or on Esc
/// (the popover's own). A setting with no picture shows the text alone (`.plain`). It replaces the plain tooltip: the
/// system's would pop up over the card. The text is the setting's own tooltip, translated like the rest (`Tip`).
struct TipCard: View {
    /// A picture key of `TipPictures`, nil for a card of text only.
    let pic: String?
    let text: String

    /// The card's width with a picture: the 240-wide picture and the padding (the web's 260 px card, `.tip-card`).
    static let width: CGFloat = 264

    private var hasPicture: Bool { pic.map(TipPictures.has) ?? false }

    /// A card without a picture is only as wide as its text, up to a card's width (the web's `.tip-card.plain`,
    /// `width: max-content`).
    static func plainWidth(_ text: String) -> CGFloat {
        let line = NSAttributedString(string: text, attributes: [.font: NSFont.preferredFont(forTextStyle: .callout)]).size().width
        return min(width, ceil(line) + 2 + 24)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if hasPicture, let pic {
                // the pictures have no words and are drawn as the web draws them, left to right in every language
                TipPicture(key: pic).environment(\.layoutDirection, .leftToRight).frame(maxWidth: .infinity)
            }
            if !text.isEmpty {
                Text(verbatim: text).font(.callout).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .frame(width: hasPicture ? Self.width : Self.plainWidth(text), alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("tip-card")
    }
}

private struct TipCardModifier: ViewModifier {
    let pic: String?
    let text: String
    @State private var shown = false
    @State private var wait: Task<Void, Never>?

    func body(content: Content) -> some View {
        content
            .onHover { inside in
                wait?.cancel()
                if inside {
                    wait = Task {
                        try? await Task.sleep(for: .milliseconds(350))
                        if !Task.isCancelled { shown = true }
                    }
                } else {
                    shown = false
                }
            }
            // keyboard focus shows it too, as the web's `focusin` does when the last input was a key (`tips.js`)
            .background(TipFocusProbe { focused in
                wait?.cancel()
                shown = focused
            })
            .onDisappear { wait?.cancel(); shown = false }
            // `.leading` is the logical edge: the card sits beside the setting on the side the inspector's content starts,
            // which is the left in English and, mirrored with the inspector, the right in Arabic
            .popover(isPresented: $shown, arrowEdge: .leading) { TipCard(pic: pic, text: text) }
            .accessibilityHint(text)
    }
}

extension View {
    /// The setting's tip card instead of a plain tooltip: `pic` is its picture's key in `TipPictures` (`data-tip-pic`).
    func tipCard(pic: String? = nil, _ text: String) -> some View {
        modifier(TipCardModifier(pic: pic, text: text))
    }
}

/// Reports when keyboard focus moves into or out of the view it sits behind: the window's first responder lies within the
/// view's frame, and focus got there by a key (Tab), not a click (`byKey` in `web/ui/tips.js`: a click in a field also
/// counts as `:focus-visible` there, and must not raise the card).
struct TipFocusProbe: NSViewRepresentable {
    let onChange: @MainActor (Bool) -> Void

    func makeNSView(context: Context) -> ProbeView {
        let v = ProbeView()
        v.onChange = onChange
        return v
    }

    func updateNSView(_ v: ProbeView, context: Context) { v.onChange = onChange }

    /// Whether focus that just moved got there by a key. A test stands in for the key event it cannot post.
    @MainActor static var keyDriven: @MainActor () -> Bool = { NSApp.currentEvent?.type == .keyDown }

    final class ProbeView: NSView {
        var onChange: (@MainActor (Bool) -> Void)?
        private var observation: NSKeyValueObservation?
        private var inside = false

        override func viewDidMoveToWindow() {
            observation = window?.observe(\.firstResponder, options: [.new]) { [weak self] window, _ in
                MainActor.assumeIsolated { self?.responderChanged(window) }
            }
        }

        private func responderChanged(_ window: NSWindow) {
            guard let r = window.firstResponder as? NSView, r !== self else { set(false); return }
            let mine = convert(bounds, to: nil)
            let theirs = r.convert(r.bounds, to: nil)
            let within = mine.contains(CGPoint(x: theirs.midX, y: theirs.midY))
            // focus that arrived by a click leaves the card alone; focus leaving always hides it
            if within { if TipFocusProbe.keyDriven() { set(true) } } else { set(false) }
        }

        private func set(_ on: Bool) {
            guard on != inside else { return }
            inside = on
            onChange?(on)
        }
    }
}
