import SwiftUI

/// The stats card (`index.html:198-229`, docs/UI-MAP.md section 3): every row the web shows, in its order, as
/// `LabeledContent` rows in a grouped form section, but the Fins row: that heads the status card, pinned above the
/// inspector's tabs (`StatusCard`). Values arrive as plain structs (`PartReadout`, `FitReadout`,
/// `FinReadout`), so the card knows nothing of the engine.
struct StatsCard: View {
    var fileName: String
    var part: PartReadout
    var fit: FitReadout?
    var fins: FinReadout?
    /// Rows other features add after Analysis (`InspectorSlot.stats`: Add model, Remove model).
    var rows: [InspectorSection] = []

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
            case .text(let s): Text("\(acc)\(Text(s))")
            case .swatch: Text("\(acc)\(Text(Image(systemName: "circle.fill")).foregroundStyle(Color.orange).font(.caption2))")
            case .redSwatch: Text("\(acc)\(Text(Image(systemName: "circle.fill")).foregroundStyle(Color(red: 1, green: 0.353, blue: 0.302)).font(.caption2))")
            }
        }
    }
}

/// The status card (`index.html:603-626`, UI-MAP 4.3), pinned above the inspector's tabs (app-only: the web's sits at
/// the foot of its one column): the Fins line (the count, or Add fins while they are off), then the notes and the grams
/// receipt. The notes scroll inside the card past `notesMaxHeight`, so a long run of warnings (slivers, midair starts,
/// plate ties) leaves the tabs most of the column. No accessibility identifier on the card or its scroll view: it holds
/// identified elements (#204, #206).
struct StatusCard: View {
    var part: PartReadout
    var fins: FinReadout?
    /// `#s-import-note` (`renderImportNote`, `ui/io.js:26`); empty hides it.
    var importNote = ""
    /// `#island-warn` (`renderIslandCard`): last in the card, in the island card's red.
    var islandCard: String?
    /// Lines other features add (`Feature.statusLines`), after the island card.
    var extra: [StatusLine] = []
    /// Turns Add fins on, while it is off: the Fins line then offers it where "—" would stand (S2: the toolbar's
    /// toggle alone was not found). Nil while the fins are on.
    var addFins: (() -> Void)?
    @State private var showFinInfo = false
    @State private var showReceiptInfo = false
    /// The notes' laid-out height: the scroll view is as tall, up to `notesMaxHeight`.
    @State private var notesHeight: CGFloat = 0

    /// The notes' most height before they scroll: six or so lines of callout text.
    static let notesMaxHeight: CGFloat = 120

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
        VStack(alignment: .leading, spacing: 6) {
            finsLine
            if !notes.isEmpty || fins?.grams != nil {
                ScrollView(.vertical) {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(Array(notes.enumerated()), id: \.offset) { _, note in
                            Text(note.text).font(.callout).foregroundStyle(note.color)
                                .fixedSize(horizontal: false, vertical: true)
                                .accessibilityIdentifier(note.text == importNote ? "import-note" : "status-note")
                                .padding(.leading, note.bar == nil ? 0 : 9)
                                .overlay(alignment: .leading) {
                                    if let bar = note.bar { RoundedRectangle(cornerRadius: 1.5).fill(bar).frame(width: 3) }
                                }
                        }
                        if let grams = fins?.grams { receipt(grams) }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { notesHeight = $0 }
                }
                .scrollBounceBehavior(.basedOnSize)
                .frame(height: min(max(notesHeight, 1), Self.notesMaxHeight))
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quinary, in: RoundedRectangle(cornerRadius: 8))
    }

    /// The Fins row of the web's stats card (`#s-fins`): the count, or Add fins while they are off.
    private var finsLine: some View {
        LabeledContent("Fins") {
            HStack(spacing: 4) {
                if let addFins {
                    Button("Add fins", action: addFins)
                        .controlSize(.small)
                        .help("Build the supports for this pose")
                        .accessibilityIdentifier("stat-add-fins")
                }
                Text(fins?.fins ?? "—").foregroundStyle(fins?.finsWarn == true ? Color.orange : Color.primary)
                    .multilineTextAlignment(.trailing)
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
    }

    /// The grams receipt (`#r-grams`), with its info button.
    private func receipt(_ grams: String) -> some View {
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
