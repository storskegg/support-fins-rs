import SwiftUI

/// The stats card (`index.html:198-229`, docs/UI-MAP.md section 3): every row the web shows, in its order, as
/// `LabeledContent` rows in a grouped form section. Values arrive as plain structs (`PartReadout`, `FitReadout`,
/// `FinReadout`), so the card knows nothing of the engine.
struct StatsCard: View {
    var fileName: String
    var part: PartReadout
    var fit: FitReadout?
    var fins: FinReadout?
    /// Rows other features add after Analysis (`InspectorSlot.stats`: Add model, Remove model).
    var rows: [InspectorSection] = []
    @State private var showFinInfo = false

    var body: some View {
        Section("Part") {
            LabeledContent("File") { Text(fileName).accessibilityIdentifier("stat-name") }
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
                            .accessibilityLabel("About these supports")
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
            ForEach(rows) { $0.content() }
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
    /// `#s-import-note` (`renderImportNote`, `ui/io.js:26`); empty hides it.
    var importNote = ""
    /// `#island-warn` (`renderIslandCard`): last in the card, in the island card's red.
    var islandCard: String?
    /// Lines other features add (`Feature.statusLines`), after the island card.
    var extra: [StatusLine] = []
    @State private var showReceiptInfo = false

    /// The card's lines in the order of `index.html:603-610`: import, flat, fin, over-warn, island-warn. The two
    /// warning cards carry the web's 3 px bar on their left (`.over-warn` amber #ffb454, `.island-warn` red
    /// #ff5a4d, `style.css:874-887`).
    static func lines(importNote: String, flat: String, flatGood: Bool, fin: String, overWarn: String,
                      island: String?) -> [(text: String, color: Color, bar: Color?)] {
        (importNote.isEmpty ? [] : [(importNote, Color.secondary, nil)])
            + (flat.isEmpty ? [] : [(flat, flatGood ? Color.green : Color.secondary, nil)])
            + (fin.isEmpty ? [] : [(fin, Color.secondary, nil)])
            + (overWarn.isEmpty ? [] : [(overWarn, Color.primary, Self.amber)])
            + (island.map { [($0, Color.primary, Self.red)] } ?? [])
    }

    static let amber = Color(red: 1, green: 0.706, blue: 0.329)
    static let red = Color(red: 1, green: 0.353, blue: 0.302)

    var body: some View {
        let notes = Self.lines(importNote: importNote, flat: part.flatNote, flatGood: part.flatNoteGood,
                               fin: fins?.lead ?? "", overWarn: part.overWarn, island: islandCard)
            + extra.map { ($0.text, $0.color, $0.bar) }
        if !notes.isEmpty || fins?.grams != nil {
            Section("Status") {
                ForEach(Array(notes.enumerated()), id: \.offset) { _, note in
                    Text(note.text).font(.callout).foregroundStyle(note.color)
                        .padding(.leading, note.bar == nil ? 0 : 9)
                        .overlay(alignment: .leading) {
                            if let bar = note.bar { RoundedRectangle(cornerRadius: 1.5).fill(bar).frame(width: 3) }
                        }
                }
                if let grams = fins?.grams {
                    HStack {
                        Text(grams).font(.title3.weight(.semibold)).monospacedDigit()
                        Text("of support material added").foregroundStyle(.secondary)
                        Button { showReceiptInfo.toggle() } label: { Image(systemName: "info.circle") }
                            .buttonStyle(.plain)
                            .accessibilityLabel("About the support material")
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
