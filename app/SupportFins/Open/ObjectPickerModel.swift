import Foundation
import Observation

/// The object picker (`pickObjects`, `web/ui/io.js:140-200`): a file with several objects asks which to fin.
/// Largest first, except that our own 3MF export's "<name> supports" object goes last, and the first row is
/// pre-selected (`io.js:151-161`), so re-opening an export pre-selects the part, not its tine-heavy supports.
///
/// Several checked load either merged into one part (the web's only way) or as separate models on one plate, each where
/// it stands in the file (the app's: Plate ties need separate models). Objects whose boxes do not overlap look like a
/// set of separate parts, and separate is then the default; overlapping ones (a part and its insert) merge by default.
@MainActor
@Observable
final class ObjectPickerModel: Identifiable {
    struct Entry: Identifiable, Equatable {
        /// The object's place in the file (what `onLoad` hands back).
        let id: Int
        let name: String
        let triangles: Int
        let size: [Int]
        /// The object's box in the file, `[min x, y, z, max x, y, z]` (`Part.bounds`), or nil when not known.
        let bounds: [Double]?

        /// `3,208 tris · 40×40×40 mm` (`io.js:166-168`).
        var meta: String {
            let n = triangles.formatted(.number)
            return "\(n) \(JS.t("tris")) · \(size[0])×\(size[1])×\(size[2]) mm"
        }
    }

    let entries: [Entry]
    private(set) var selected: Set<Int>
    /// Called with the chosen objects' file indexes, in file order.
    @ObservationIgnored var onLoad: ([Int]) -> Void = { _ in }
    /// Called with the chosen objects' file indexes, in file order, to load each as a model of its own.
    @ObservationIgnored var onLoadSeparately: ([Int]) -> Void = { _ in }
    @ObservationIgnored var onCancel: () -> Void = {}

    /// `objects` are the file's objects in file order, as (name, triangles, size in mm); `bounds`, each one's box in
    /// the file (`Part.bounds`), when known.
    init(objects: [(name: String, triangles: Int, size: [Double])], bounds: [[Double]]? = nil) {
        let ours = { (n: String) in n.hasSuffix(" supports") ? 1 : 0 }
        let order = objects.indices.sorted {
            (ours(objects[$0].name), -objects[$0].triangles, $0) < (ours(objects[$1].name), -objects[$1].triangles, $1)
        }
        entries = order.map { i in
            let o = objects[i]
            let b = bounds.flatMap { $0.indices.contains(i) && $0[i].count == 6 ? $0[i] : nil }
            return Entry(id: i, name: o.name, triangles: o.triangles, size: o.size.map { Int($0.rounded()) }, bounds: b)
        }
        selected = order.first.map { [$0] } ?? []
    }

    func isSelected(_ id: Int) -> Bool { selected.contains(id) }

    func set(_ id: Int, _ on: Bool) {
        if on { selected.insert(id) } else { selected.remove(id) }
    }

    /// The "Select all" checkbox above the rows (user suggestion S1; the web has none): checked when every row is,
    /// unchecked when none is, mixed in between.
    enum AllSelection: Equatable { case none, some, all }

    var allSelection: AllSelection {
        if selected.isEmpty { return .none }
        return entries.allSatisfy { selected.contains($0.id) } ? .all : .some
    }

    /// Check every row, or uncheck every row.
    func setAll(_ on: Bool) {
        selected = on ? Set(entries.map(\.id)) : []
    }

    /// The checkbox's state as the two sources of SwiftUI's mixed-state `Toggle(sources:)`: any row checked, every row
    /// checked. Both off is unchecked, both on is checked, one of each is the mixed dash.
    var selectAllSources: [Bool] { [allSelection != .none, allSelection == .all] }

    /// Load is disabled until one is checked (`io.js:172`).
    var canLoad: Bool { !selected.isEmpty }

    /// The checked objects stand apart in the file: their boxes overlap nowhere (touching is apart). False when a box
    /// is not known, or for fewer than two.
    var standApart: Bool {
        let boxes = entries.filter { selected.contains($0.id) }.map(\.bounds)
        guard boxes.count > 1, boxes.allSatisfy({ $0 != nil }) else { return false }
        let b = boxes.compactMap { $0 }
        for i in b.indices {
            for j in b.indices where j > i {
                if (0..<3).allSatisfy({ k in b[i][k] < b[j][k + 3] - Self.touch && b[j][k] < b[i][k + 3] - Self.touch }) { return false }
            }
        }
        return true
    }

    /// Boxes that share a face (within this, in mm) touch; they do not overlap.
    static let touch = 1e-6

    /// Several checked: separate models is the default (Return) when they stand apart, merging otherwise.
    var separateByDefault: Bool { selected.count > 1 && standApart }

    /// "Load", or the default of two for several checked: "Load 3 as separate models" or "Merge 3 into one part".
    var loadTitle: String {
        guard selected.count > 1 else { return JS.t("Load") }
        return separateByDefault ? separateTitle : mergeTitle
    }

    /// The other way for several checked (nil for one or none): the button beside the default.
    var otherTitle: String? {
        guard selected.count > 1 else { return nil }
        return separateByDefault ? mergeTitle : separateTitle
    }

    private var separateTitle: String { JS.t("Load {n} as separate models", ["n": String(selected.count)]) }
    private var mergeTitle: String { JS.t("Merge {n} into one part", ["n": String(selected.count)]) }

    /// What the default does with several checked: "2 selected — merged into one part" (`io.js:174`), or that each
    /// goes on the plate as its own model, where it stands in the file.
    var hint: String {
        guard selected.count > 1 else { return "" }
        return separateByDefault
            ? JS.t("{n} selected — each its own model, where it stands in the file", ["n": String(selected.count)])
            : JS.t("{n} selected — merged into one part", ["n": String(selected.count)])
    }

    /// The default button (Return): one object, or several the default way.
    func load() {
        guard canLoad else { return }
        load(separately: separateByDefault)
    }

    /// The other button (several checked).
    func loadOther() {
        guard selected.count > 1 else { return }
        load(separately: !separateByDefault)
    }

    func load(separately: Bool) {
        guard canLoad else { return }
        if separately && selected.count > 1 { onLoadSeparately(selected.sorted()) } else { onLoad(selected.sorted()) }
    }

    func cancel() { onCancel() }
}
