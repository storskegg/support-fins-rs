import Testing

@testable import Support_Fins

/// The picker's "Select all" checkbox (user suggestion S1): checking it checks every row, unchecking it unchecks every
/// row; it is checked exactly when every row is (checked by hand or not), and mixed when only some are.
@MainActor
@Suite("Object picker: Select all")
struct ObjectPickerSelectAllTests {
    /// Four objects, the first pre-selected as the picker does.
    private func picker() -> ObjectPickerModel {
        ObjectPickerModel(objects: (0..<4).map { (name: "o\($0)", triangles: 12, size: [10.0, 10, 10]) })
    }

    @Test func oneRowPreselectedIsMixed() {
        let p = picker()
        #expect(p.selected.count == 1)
        #expect(p.allSelection == .some)
    }

    @Test func checkingSelectAllChecksEveryRow() {
        let p = picker()
        p.setAll(true)
        #expect(p.selected == Set(p.entries.map(\.id)))
        #expect(p.allSelection == .all)
        #expect(p.loadTitle == "Merge 4 into one part" && p.hint == "4 selected — merged into one part")
    }

    @Test func uncheckingSelectAllUnchecksEveryRow() {
        let p = picker()
        p.setAll(true)
        p.setAll(false)
        #expect(p.selected.isEmpty)
        #expect(p.allSelection == .none)
        #expect(!p.canLoad && p.loadTitle == "Load" && p.hint == "")
    }

    @Test func checkingEveryRowByHandChecksSelectAll() {
        let p = picker()
        for e in p.entries.dropLast() { p.set(e.id, true) }
        #expect(p.allSelection == .some)
        p.set(p.entries.last!.id, true)
        #expect(p.allSelection == .all)
    }

    @Test func uncheckingOneRowWhenAllWereCheckedUnchecksSelectAll() {
        let p = picker()
        p.setAll(true)
        p.set(p.entries[2].id, false)
        #expect(p.allSelection == .some)
        #expect(p.selected.count == 3 && p.loadTitle == "Merge 3 into one part")
    }

    @Test func uncheckingEveryRowByHandUnchecksSelectAll() {
        let p = picker()
        p.set(p.entries[0].id, false)
        #expect(p.allSelection == .none)
    }

    /// The checkbox shows the state through two sources (SwiftUI's mixed `Toggle(sources:)`): any row checked, and
    /// every row checked. They agree at none and all, and differ (mixed) in between.
    @Test func theCheckboxSourcesShowOffMixedAndOn() {
        let p = picker()
        #expect(p.selectAllSources == [true, false])  // mixed
        p.setAll(true)
        #expect(p.selectAllSources == [true, true])
        p.setAll(false)
        #expect(p.selectAllSources == [false, false])
    }

    /// A one-object file never shows the picker, but the model holds: its single row decides.
    @Test func aSingleRowIsAllOrNone() {
        let p = ObjectPickerModel(objects: [(name: "only", triangles: 12, size: [10, 10, 10])])
        #expect(p.allSelection == .all)
        p.set(p.entries[0].id, false)
        #expect(p.allSelection == .none)
    }
}
