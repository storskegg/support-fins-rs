import SwiftUI

/// The window with no part (`#drop`, `index.html:752-775`): the first-visit intro, or, once a file was tried in
/// this window, just the button and the drop hint (`#drop.compact`).
struct EmptyWindowView: View {
    var compact: Bool
    var onImport: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            if !compact {
                Text("Print it support-free, in any slicer").font(.largeTitle.weight(.semibold)).multilineTextAlignment(.center)
                VStack(alignment: .leading, spacing: 10) {
                    step(1, "Import an STL, 3MF, STEP or OBJ.", "Drop it anywhere on this page.")
                    step(2, "Rotate it.", "Red marks every surface that needs support.")
                    step(3, "Export.", "Fins come baked in — no slicer supports needed.")
                }
                .frame(maxWidth: 460, alignment: .leading)
                Text("Nothing is uploaded. The file is read inside this tab and never leaves your machine.")
                    .font(.callout).foregroundStyle(.secondary).frame(maxWidth: 460, alignment: .leading)
                WhatsNewCard()
            }
            Button("Import a model", action: onImport)
                .buttonStyle(.borderedProminent).controlSize(.large)
                .accessibilityIdentifier("empty-import")
            Text("or drop it anywhere on the page").font(.callout).foregroundStyle(.secondary)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
    }

    private func step(_ n: Int, _ title: LocalizedStringKey, _ detail: LocalizedStringKey) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text("\(n).").monospacedDigit().foregroundStyle(.secondary)
            Text("\(Text(title).bold()) \(Text(detail))")
        }
    }
}
