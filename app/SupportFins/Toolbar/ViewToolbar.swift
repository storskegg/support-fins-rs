import AppKit
import SwiftUI

/// The floating bottom toolbar: a glass capsule over the view with Undo, Redo, then the badge slots
/// (`#viewbar`, UI-MAP section 5). Undo/Redo drive the session's history; the slots come from the registry.
struct ViewToolbar: View {
    let session: PartSession
    @Bindable var registry: ToolbarRegistry

    var body: some View {
        GlassEffectContainer {
            HStack(spacing: 4) {
                ToolbarButton(title: "Undo", systemImage: "arrow.uturn.backward", help: "Undo (⌘Z)",
                              enabled: session.canUndo) { session.undo() }
                ToolbarButton(title: "Redo", systemImage: "arrow.uturn.forward", help: "Redo (⇧⌘Z)",
                              enabled: session.canRedo) { session.redo() }
                Divider().frame(height: 18).padding(.horizontal, 2)
                ForEach(registry.slots) { slot in
                    SlotButton(slot: slot, registry: registry)
                }
            }
            .padding(.horizontal, 8).padding(.vertical, 6)
            .glassEffect(.regular, in: .capsule)
        }
        .padding(.bottom, 14)
        .background(EscapeFolds(registry: registry))
    }
}

private struct ToolbarButton: View {
    let title: LocalizedStringKey
    let systemImage: String
    let help: LocalizedStringKey
    var enabled = true
    var lit = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .labelStyle(.titleAndIcon)
                .padding(.horizontal, 8).padding(.vertical, 4)
                .background(lit ? AnyShapeStyle(.tint.opacity(0.25)) : AnyShapeStyle(.clear), in: .capsule)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .help(help)
    }
}

private struct SlotButton: View {
    let slot: ToolbarSlot
    @Bindable var registry: ToolbarRegistry

    var body: some View {
        let badge = registry.badges[slot]
        ToolbarButton(title: slot.title, systemImage: slot.systemImage, help: slot.help,
                      enabled: badge != nil, lit: registry.open == slot || (badge?.isLit() ?? false)) {
            registry.toggle(slot)
        }
        .overlay(alignment: .top) {
            if let f = registry.flash, f.slot == slot {
                Text(f.text).font(.caption.monospacedDigit()).padding(.horizontal, 6).padding(.vertical, 2)
                    .glassEffect(.regular, in: .capsule).offset(y: -30).allowsHitTesting(false)
            }
        }
        .accessibilityIdentifier("toolbar-\(slot.id)")
        .popover(isPresented: Binding(get: { registry.open == slot && badge != nil },
                                      set: { if !$0, registry.open == slot { registry.open = nil } }),
                 arrowEdge: .top) {
            badge?.popover().padding(12)
        }
    }
}

/// Esc folds the open popover away, wherever the key focus is (`ui/toolbar.js:104-114`: a keydown listener on the
/// page's window, so it runs whatever has the focus, and the page's other Esc listeners still run after it).
///
/// SwiftUI's `onExitCommand` can't do this. It reaches only the focused view and its ancestors, and the toolbar's
/// plain buttons never hold the focus: the Esc goes to the popover's own window, to the viewer (which keeps Esc for
/// an armed mode), or to whatever else in the window has the keys. None of those is inside the toolbar (the
/// Section UI test's Esc did nothing). So this watches the app's key events (a local monitor, one per window's toolbar) and folds when
/// the Esc is for this window: its own keys, or those of a window it owns that cannot become main (the popover's).
/// The event is passed on untouched, so the window's Esc cascade (`ContentView`) still ends an armed mode, as the
/// web's `app.js` listener does next to the toolbar's.
private struct EscapeFolds: NSViewRepresentable {
    let registry: ToolbarRegistry

    func makeNSView(context: Context) -> Probe {
        let v = Probe()
        v.registry = registry
        return v
    }

    func updateNSView(_ v: Probe, context: Context) { v.registry = registry }

    final class Probe: NSView {
        weak var registry: ToolbarRegistry?
        private var monitor: Any?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] e in
                MainActor.assumeIsolated { self?.escape(e) }
                return e
            }
        }

        private func escape(_ e: NSEvent) {
            guard e.keyCode == 53, let registry, registry.open != nil, let own = window, let to = e.window else { return }
            guard to === own || to.parent === own || (own.isMainWindow && !to.canBecomeMain) else { return }
            registry.fold()
        }
    }
}
