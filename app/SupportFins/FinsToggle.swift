import SwiftUI

/// The toolbar's Add fins (`fins-toggle`, `ui/settings.js:351`): "Add fins" off, "Fins on" on. Turning it on or
/// off is an undo step, as in the web.
struct FinsToggle: View {
    let session: PartSession

    var body: some View {
        Toggle(session.finsOn ? "Fins on" : "Add fins", systemImage: "square.stack.3d.up",
               isOn: Binding(get: { session.finsOn }, set: { on in session.perform(on ? JS.t("Add fins") : JS.t("Remove fins")) { session.finsOn = on } }))
            .toggleStyle(.button)
            .help("Build the supports for this pose")
            .accessibilityIdentifier("fins-toggle")
    }
}

/// The progress overlay (`#spinner`), shown 300 ms into a build.
struct BuildSpinner: View {
    var body: some View {
        VStack {
            Spacer()
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("generating supports…").font(.callout)
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .background(.regularMaterial, in: .capsule)
            .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity)
        .allowsHitTesting(false)
        .transition(.opacity)
        .accessibilityLabel("generating supports…")
        .accessibilityIdentifier("build-spinner")
    }
}
