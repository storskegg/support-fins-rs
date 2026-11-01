import SwiftUI

/// The toolbar's Add fins (`fins-toggle`, `ui/settings.js:351`): "Add fins" off, "Fins on" on. Turning it on or
/// off is an undo step, as in the web.
///
/// The web's button is a word, and the one way to get fins: the toolbar would show only the icon (its default label
/// style), which nobody read as "build the supports" (S2, November 2026), so it is the word, as on the web: the word alone, at the small control size, so that it, Export and the inspector
/// toggle still fit in a 700 pt window with the inspector open (`WindowToolbarLayoutTests`).
struct FinsToggle: View {
    let session: PartSession

    var body: some View {
        Toggle(session.finsOn ? "Fins on" : "Add fins", systemImage: "square.stack.3d.up",
               isOn: Binding(get: { session.finsOn }, set: { on in session.perform(on ? JS.t("Add fins") : JS.t("Remove fins")) { session.finsOn = on } }))
            .toggleStyle(.button)
            .labelStyle(.titleOnly)
            .controlSize(.small)
            .help("Build the supports for this pose")
            .accessibilityIdentifier("fins-toggle")
            // a new toggle per state: in the window toolbar, another process (VoiceOver, XCUITest) kept reading the label
            // the item was made with, "Add fins" while it was on, though the title and the value had changed (UI run after
            // #263; `DocumentWindowToolbarTests` reads it as they do). An explicit `accessibilityLabel` did not refresh it
            .id(session.finsOn)
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
        .accessibilityElement(children: .combine)
        .accessibilityLabel("generating supports…")
        .accessibilityIdentifier("build-spinner")
    }
}
