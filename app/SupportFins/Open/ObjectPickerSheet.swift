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
            List(model.entries) { entry in
                Toggle(isOn: Binding(get: { model.isSelected(entry.id) }, set: { model.set(entry.id, $0) })) {
                    HStack {
                        Text(entry.name)
                        Spacer()
                        Text(entry.meta).foregroundStyle(.secondary).monospacedDigit()
                    }
                }
                .toggleStyle(.checkbox)
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
}
