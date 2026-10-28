import Observation
import Viewer

/// Per-fin removal (Auto and Full): click a fin to drop just that one (`web/ui/remove.js`).
///
/// A removal is remembered by the fin's spatial key (`FinKey`), not by the build's sequential id, so it
/// survives a rebuild of the same pose and even a round trip through another pose; `removedHere` is the
/// build's own fins those keys name, matched to the nearest fin (`FinKey.matchRemoved`). Only the fin
/// buffer's vertex ranges are masked: the geometry is not rebuilt, filtering is cheap.
///
/// The set of removed keys is `removed`: the session owns it once `PartSession` lands (it is part of its
/// undo snapshots), so every change calls `onChange(old, new)` (`histPush` in the web) and the session can
/// register the undo and write it back.
@MainActor
@Observable
final class RemovalModel {
    /// The keys of the fins the user clicked away, in every pose (`removedSigs`).
    var removed: Set<FinKey> = [] {
        didSet { if removed != oldValue { onChange?(oldValue, removed) } }
    }
    /// Remove mode is armed (`removeMode`): hover lights the fin a click would drop, a click drops it.
    private(set) var armed = false
    /// The record id of the fin under the pointer (`hoverFinId`).
    private(set) var hoveredFin: Int?

    @ObservationIgnored let build: BuildModel
    @ObservationIgnored private let finsOn: @MainActor () -> Bool
    @ObservationIgnored private let mode: @MainActor () -> FinSettings.Mode
    @ObservationIgnored var onChange: (@MainActor (_ old: Set<FinKey>, _ new: Set<FinKey>) -> Void)?
    @ObservationIgnored private var index: (result: ObjectIdentifier, entries: [(from: Int, to: Int, fin: Int)])?

    init(build: BuildModel, finsOn: @escaping @MainActor () -> Bool, mode: @escaping @MainActor () -> FinSettings.Mode) {
        self.build = build
        self.finsOn = finsOn
        self.mode = mode
    }

    // MARK: availability

    /// `autoLike`: Auto and Full coverage; Draw places walls by hand (`ui/settings.js:39`).
    var autoLike: Bool { mode() == .auto || mode() == .full }

    /// The Remove fins button and Restore all are offered only in Auto / Full with fins on screen
    /// (`syncRemoveUI`).
    var controlsShown: Bool { finsOn() && autoLike }

    /// `removeActive`: armed, fins on, Auto or Full.
    var isActive: Bool { armed && controlsShown }

    /// The fins this build's removed keys name: record id -> the key it matched (`removedIds` + `removedBy`).
    var removedHere: [Int: FinKey] { build.matchedRemovals(of: removed) }

    /// "Restore all" is offered only when a fin is removed in THIS build: a removal made in another pose
    /// would map to nothing here (`syncRemoveUI`).
    var canRestore: Bool { controlsShown && !removedHere.isEmpty }

    /// The button's label (`syncRemoveUI`).
    var toggleLabel: String { armed ? "Click a fin — Esc done" : "Remove fins" }

    // MARK: arming

    /// The toggle: armed -> off, off -> armed.
    func toggle() { armed ? disarm() : arm() }

    /// `beginRemove`. Armed with every fin already removed is a harmless no-op until a rebuild brings fins back.
    func arm() {
        guard controlsShown else { return }
        armed = true
    }

    /// `cancelRemove`: Esc, a right click, the toggle again, a mode switch, fins off, a new part.
    func disarm() {
        armed = false
        hoveredFin = nil
    }

    /// Disarms when the mode can no longer be active (fins off, Draw): the web cancels at each of those
    /// places (`settings.js:103,421,433`, `finbuild.js:231`). The wiring calls it when the settings change.
    func sync() {
        if armed && !controlsShown { disarm() }
    }

    /// A new part: keys can coincidentally match a different model's fins (`resetRemovals`).
    func reset() {
        disarm()
        removed = []
    }

    // MARK: picking

    /// The fin owning a triangle of the fin buffer, by `triRanges` (`pickFin`).
    func fin(atTriangle triangle: Int) -> FinRecord? {
        guard let records = build.records, let result = build.latest else { return nil }
        let id = ObjectIdentifier(result)
        if index?.result != id {
            let entries = records.fins.enumerated().flatMap { i, f in f.triRanges.map { (from: $0.from, to: $0.to, fin: i) } }
            index = (id, entries.sorted { $0.from < $1.from })
        }
        let v = triangle * 3
        // the last range starting at or before the vertex
        var lo = 0, hi = index!.entries.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if index!.entries[mid].from <= v { lo = mid + 1 } else { hi = mid }
        }
        guard lo > 0, case let e = index!.entries[lo - 1], v < e.to else { return nil }
        return records.fins[e.fin]
    }

    /// The pointer moved over a triangle of the fin buffer (nil: off the fins): `hoverRemove`.
    func hover(triangle: Int?) {
        guard isActive else { hoveredFin = nil; return }
        hoveredFin = triangle.flatMap { fin(atTriangle: $0)?.id }
    }

    /// One click drops just the fin under the pointer (`clickRemove`). A fin with no key (no line) cannot be
    /// remembered and stays.
    func click(triangle: Int?) {
        guard isActive, let triangle, let fin = fin(atTriangle: triangle),
              let i = build.records?.fins.firstIndex(where: { $0.id == fin.id }),
              let key = build.keys[i].sig else { return }
        // a stored key from an earlier build can read the same as this fin's while it names another fin
        // (matched by nearness): keep each stored key unique
        var stored = key, n = 1
        while removed.contains(stored) {
            stored = FinKey(raw: "\(key.raw)~\(n)")
            n += 1
        }
        removed.insert(stored)
        hoveredFin = nil
    }

    /// Restore all: only THIS pose's removals, the keys that matched a fin in the current build; removals
    /// made in other poses stay (`restore-fins`).
    func restoreAll() {
        let here = removedHere
        guard !here.isEmpty else { return }
        removed.subtract(here.values)
        hoveredFin = nil
    }

    // MARK: the viewer

    /// The fin buffer's vertex ranges the renderer leaves out.
    var hiddenRanges: [Range<Int>] { build.hiddenRanges(removed: removed) }

    /// The hovered fin's vertex ranges, drawn red over the fins.
    var highlightedRanges: [Range<Int>] { hoveredFin.map { build.ranges(ofFin: $0) } ?? [] }

    /// Wires the viewer's pointer to this model and pushes the masks. Call after a build lands and after each
    /// change of `removed`, `hoveredFin` or `armed`.
    func update(_ controller: ViewerController) {
        controller.onSupportHover = { [weak self] t in
            self?.hover(triangle: t)
            self?.update(controller)
        }
        controller.onSupportClick = { [weak self] t in
            self?.click(triangle: t)
            self?.update(controller)
        }
        controller.onCancelArmed = { [weak self] in
            self?.disarm()
            self?.update(controller)
        }
        controller.supportPicking = isActive
        controller.renderer.setFinMask(hidden: hiddenRanges, crest: [])
        controller.renderer.setFinHighlight(highlightedRanges)
    }
}
