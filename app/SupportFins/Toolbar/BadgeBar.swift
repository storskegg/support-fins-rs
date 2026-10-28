import SwiftUI

/// A stand-in for the view toolbar's badge slots (`ui/toolbar.js` `addBadge`; docs/UI-MAP.md §5) until track A's
/// toolbar lands: a capsule of badges, each a button that opens a popover and is lit while its tool is on.
/// Replaced by the real toolbar's slots in a small PR once A's API is on main.
struct BadgeBar<Content: View>: View {
    let content: Content
    init(@ViewBuilder content: () -> Content) { self.content = content() }

    var body: some View {
        HStack(spacing: 4) { content }
            .padding(6)
            .background(.regularMaterial, in: .capsule)
            .shadow(radius: 6, y: 2)
    }
}

/// One badge: a toggle that opens its popover; `lit` shows the tool is on while the popover is folded
/// (`isOn` in `addBadge`).
struct Badge<Popover: View>: View {
    let title: String
    let help: String
    var lit = false
    @Binding var open: Bool
    @ViewBuilder var popover: () -> Popover

    var body: some View {
        Button(title) { open.toggle() }
            .buttonStyle(.bordered)
            .tint(lit ? .accentColor : nil)
            .help(help)
            .accessibilityIdentifier(title.lowercased() + "-badge")
            .popover(isPresented: $open, arrowEdge: .top) { popover().padding(12) }
    }
}
