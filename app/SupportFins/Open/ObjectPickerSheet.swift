import SwiftUI

/// The picker as a sheet (`#picker`, `index.html:778-786`): Cancel is Esc, Load is Return.
struct ObjectPickerSheet: View {
    @Bindable var model: ObjectPickerModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("This file has several objects").font(.headline)
            Text("Pick the one to add fins to. Check more than one to merge them into a single part.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            // "Select all" (user suggestion S1): its checkbox lines up with the rows' (the list's row inset), and shows
            // the mixed dash while only some rows are checked; a click on it when mixed checks every row (macOS).
            Toggle(sources: selectAllSources, isOn: \.self) { Text("Select all") }
                .toggleStyle(.checkbox)
                .padding(.leading, Self.rowInset)
                .accessibilityIdentifier("picker-select-all")
            List(model.entries) { entry in
                Toggle(isOn: Binding(get: { model.isSelected(entry.id) }, set: { model.set(entry.id, $0) })) {
                    HStack {
                        Text(entry.name)
                        Spacer()
                        Text(entry.meta).foregroundStyle(.secondary).monospacedDigit()
                    }
                }
                .toggleStyle(.checkbox)
                .accessibilityIdentifier("picker-row-\(entry.id)")
            }
            .frame(minHeight: 120, maxHeight: 280)
            HStack {
                Text(model.hint).font(.callout).foregroundStyle(.secondary)
                Spacer()
                Button("Cancel", role: .cancel) { model.cancel() }.keyboardShortcut(.cancelAction)
                Button(model.loadTitle) { model.load() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!model.canLoad)
            }
        }
        .padding(20)
        .frame(width: 460)
    }

    /// How far the list's rows sit in from its edge (the inset list style's margin plus the row's own inset).
    static let rowInset: CGFloat = 16

    /// The Select all checkbox's two sources (`ObjectPickerModel.selectAllSources`): setting either checks or unchecks
    /// every row.
    private var selectAllSources: [Binding<Bool>] {
        [0, 1].map { i in Binding(get: { model.selectAllSources[i] }, set: { model.setAll($0) }) }
    }
}
