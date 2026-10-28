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
        .onExitCommand { registry.fold() }
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
        .popover(isPresented: Binding(get: { registry.open == slot && badge != nil && (badge?.tool?.isOn() ?? true) },
                                      set: { if !$0, registry.open == slot { registry.open = nil } }),
                 arrowEdge: .top) {
            badge?.popover().padding(12)
        }
    }
}
