import SupportFinsCore
import SwiftUI

/// The Suggest popover (`#suggest-pop`, `ui/suggest.js`): the button ranks on click, never on open; the rows
/// are the engine's, worded as the page words them; a click on a row turns the part.
struct SuggestPopover: View {
    @Bindable var model: SuggestModel
    /// Rank with the window's settings.
    var rank: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button(model.buttonTitle, action: rank)
                .disabled(model.isRanking)
                .accessibilityIdentifier("suggest-orient")
            if !model.note.isEmpty {
                Text(model.note)
                    .font(.callout)
                    .foregroundStyle(model.noteIsGood ? Color.green : Color.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("suggest-note")
            }
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(model.rows.enumerated()), id: \.offset) { i, row in
                    SuggestRow(row: row, active: model.picked == i) { model.pick(i) }
                }
            }
        }
        .frame(width: 340, alignment: .leading)
    }
}

struct SuggestRow: View {
    let row: Suggestion
    let active: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(row.rank).bold().frame(width: 40, alignment: .leading)
                // one tight line per pose: height, fins, rough holes, then the badge (ui/suggest.js:101-108)
                Text("\(Text(row.line))\(row.badge.map { Text("  \($0)").foregroundColor(.green).bold() } ?? Text(""))")
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, 5)
            .padding(.horizontal, 8)
            .background(active ? Color.accentColor.opacity(0.25) : Color.clear, in: .rect(cornerRadius: 6))
            .overlay(alignment: .leading) {
                // the green left border of a pose that needs no fins; red text for a pose that cannot print
                if row.tier == .free { Rectangle().fill(.green).frame(width: 3) }
            }
            .foregroundStyle(row.cantPrint ? Color.red : Color.primary)
        }
        .buttonStyle(.plain)
        // one element per row (its words as the label), so each row is queryable by its identifier whatever holds it
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("suggest-row")
    }
}
