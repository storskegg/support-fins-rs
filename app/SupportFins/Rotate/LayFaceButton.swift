import SwiftUI

/// The popover's "Lay a face flat" toggle (`#lay-face`, `index.html:647`).
struct LayFaceButton: View {
    @Bindable var model: LayFaceModel

    var body: some View {
        Button(LocalizedStringKey(model.title)) { model.toggle() }
            .disabled(!model.canArm)
            .help("Click this, then click a face to set it flat on the bed. Off by default so a stray click can't re-lay the part.")
            .accessibilityIdentifier("lay-face")
    }
}

/// The armed state over the view: with the popover folded (arming folds it, since the next click is on the
/// part), this is what says the next click lays a face.
struct LayFaceHint: View {
    let model: LayFaceModel

    var body: some View {
        if model.armed {
            Text(LocalizedStringKey(model.title))
                .padding(.horizontal, 12).padding(.vertical, 6)
                .glassEffect(.regular, in: .capsule)
                .padding(.top, 12)
                .allowsHitTesting(false)
                .accessibilityIdentifier("lay-face-hint")
        }
    }
}
