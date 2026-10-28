import SwiftUI

/// The stats card (`index.html:198-229`, docs/UI-MAP.md section 3): every row the web shows, in its order, as
/// `LabeledContent` rows in a grouped form section. Values arrive as plain structs (`PartReadout`, `FitReadout`,
/// `FinReadout`), so the card knows nothing of the engine.
struct StatsCard: View {
    var fileName: String
    var part: PartReadout
    var fit: FitReadout?
    var fins: FinReadout?
    @State private var showFinInfo = false

    var body: some View {
        Section("Part") {
            LabeledContent("File", value: fileName)
            LabeledContent("Triangles", value: part.triangles)
            LabeledContent("Size", value: part.size)
            if let fit {
                LabeledContent("Volume") { Text(fit.text).foregroundStyle(fit.warn ? Color.orange : Color.primary) }
            }
            LabeledContent("Overhangs") {
                overhangsText.foregroundStyle(part.overhangsGood ? Color.green : Color.primary)
                    .help(swatchHelp)
                    .accessibilityIdentifier("stat-overhangs")
            }
            LabeledContent("Area", value: part.area)
            LabeledContent("Bed contact") { Text(part.bed).foregroundStyle(part.bedWarn ? Color.orange : Color.primary) }
            LabeledContent("Fins") {
                HStack(spacing: 4) {
                    Text(fins?.fins ?? "—").foregroundStyle(fins?.finsWarn == true ? Color.orange : Color.primary)
                        .accessibilityIdentifier("stat-fins")
                    if let info = fins?.info, !info.isEmpty {
                        Button { showFinInfo.toggle() } label: { Image(systemName: "info.circle") }
                            .buttonStyle(.plain)
                            .help("How these supports work, and what this pose leaves uncovered")
                            .popover(isPresented: $showFinInfo) {
                                Text(info).padding().frame(width: 280, alignment: .leading)
                            }
                    }
                }
            }
            LabeledContent("Bed pad", value: fins?.pad ?? "—")
            LabeledContent("Analysis") {
                Text(fins?.time ?? part.time).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    /// The swatches' tooltips, one a line (a `Text` cannot carry one tooltip per swatch).
    private var swatchHelp: String {
        part.overhangs.compactMap { seg -> String? in
            switch seg {
            case .swatch(let h), .redSwatch(let h): h
            case .text: nil
            }
        }.joined(separator: "\n")
    }

    /// The row with its amber swatches (`<i class="sw sw-small">`, `ui/part.js:316-324`).
    private var overhangsText: Text {
        part.overhangs.reduce(Text("")) { acc, seg in
            switch seg {
            case .text(let s): acc + Text(s)
            case .swatch: acc + Text(Image(systemName: "circle.fill")).foregroundStyle(Color.orange).font(.caption2)
            case .redSwatch: acc + Text(Image(systemName: "circle.fill")).foregroundStyle(Color(red: 1, green: 0.353, blue: 0.302)).font(.caption2)
            }
        }
    }
}

/// The status card (`index.html:603-626`, UI-MAP 4.3): the notes under the stats and the grams receipt.
struct StatusCard: View {
    var part: PartReadout
    var fins: FinReadout?
    var importNotes: [String] = []
    @State private var showReceiptInfo = false

    var body: some View {
        let notes: [(String, Bool)] = importNotes.map { ($0, false) }
            + (part.flatNote.isEmpty ? [] : [(part.flatNote, part.flatNoteGood)])
            + (part.overWarn.isEmpty ? [] : [(part.overWarn, false)])
            + (fins?.lead.isEmpty == false ? [(fins!.lead, false)] : [])
        if !notes.isEmpty || fins?.grams != nil {
            Section("Status") {
                ForEach(Array(notes.enumerated()), id: \.offset) { _, note in
                    Text(note.0).font(.callout).foregroundStyle(note.1 ? Color.green : Color.secondary)
                }
                if let grams = fins?.grams {
                    HStack {
                        Text(grams).font(.title3.weight(.semibold)).monospacedDigit()
                        Text("of support material added").foregroundStyle(.secondary)
                        Button { showReceiptInfo.toggle() } label: { Image(systemName: "info.circle") }
                            .buttonStyle(.plain)
                            .help(FinReadout.receiptTip)
                            .popover(isPresented: $showReceiptInfo) {
                                Text(FinReadout.receiptTip).padding().frame(width: 280, alignment: .leading)
                            }
                    }
                }
            }
        }
    }
}
