import Foundation
import Observation

/// The object picker (`pickObjects`, `web/ui/io.js:140-200`): a file with several objects asks which to fin.
/// Largest first, except that our own 3MF export's "<name> supports" object goes last, and the first row is
/// pre-selected (`io.js:151-161`), so re-opening an export pre-selects the part, not its tine-heavy supports.
@MainActor
@Observable
final class ObjectPickerModel: Identifiable {
    struct Entry: Identifiable, Equatable {
        /// The object's place in the file (what `onLoad` hands back).
        let id: Int
        let name: String
        let triangles: Int
        let size: [Int]

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
    @ObservationIgnored var onCancel: () -> Void = {}

    /// `objects` are the file's objects in file order, as (name, triangles, size in mm).
    init(objects: [(name: String, triangles: Int, size: [Double])]) {
        let ours = { (n: String) in n.hasSuffix(" supports") ? 1 : 0 }
        let order = objects.indices.sorted {
            (ours(objects[$0].name), -objects[$0].triangles, $0) < (ours(objects[$1].name), -objects[$1].triangles, $1)
        }
        entries = order.map { i in
            let o = objects[i]
            return Entry(id: i, name: o.name, triangles: o.triangles, size: o.size.map { Int($0.rounded()) })
        }
        selected = order.first.map { [$0] } ?? []
    }

    func isSelected(_ id: Int) -> Bool { selected.contains(id) }

    func set(_ id: Int, _ on: Bool) {
        if on { selected.insert(id) } else { selected.remove(id) }
    }

    /// Load is disabled until one is checked (`io.js:172`).
    var canLoad: Bool { !selected.isEmpty }

    /// "Load", or "Merge 2 & load" (`io.js:173`).
    var loadTitle: String { selected.count > 1 ? JS.t("Merge {n} & load", ["n": String(selected.count)]) : JS.t("Load") }

    /// "2 selected — merged into one part" (`io.js:174`).
    var hint: String { selected.count > 1 ? JS.t("{n} selected — merged into one part", ["n": String(selected.count)]) : "" }

    func load() {
        guard canLoad else { return }
        onLoad(selected.sorted())
    }

    func cancel() { onCancel() }
}
