import SwiftUI

/// A tip card (`web/ui/tips.js`, GH #351): a setting's tooltip under its 240 x 130 diagram (`web/ui/tippics.js`), shown
/// beside the inspector after the pointer rests on the setting for 350 ms (`HOVER_MS`), hidden when it leaves or on Esc
/// (the popover's own). A setting with no picture shows the text alone (`.plain`). It replaces the plain tooltip: the
/// system's would pop up over the card. The text is the setting's own tooltip, translated like the rest (`Tip`).
struct TipCard: View {
    /// A picture key of `TipPictures`, nil for a card of text only.
    let pic: String?
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let pic, TipPictures.has(pic) {
                TipPicture(key: pic).frame(maxWidth: .infinity)
            }
            if !text.isEmpty {
                Text(verbatim: text).font(.callout).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(12)
        .frame(width: 264, alignment: .leading)
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
            .onDisappear { wait?.cancel(); shown = false }
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
