import SwiftUI
import SupportFinsCore
import Viewer

/// What the fin readouts read of a part's build. `BuildModel` conforms; the window passes none until Track B puts
/// one on the session, and the Fins and Bed pad rows read "—" meanwhile.
@MainActor
protocol ReadoutBuildSource: AnyObject {
    /// `BuildResult.summary_json()` of the newest build, or nil before one lands.
    var readoutSummaryJSON: String? { get }
    /// `records_json().fins.count`
    var readoutFinCount: Int { get }
    /// Some wall top got an interface crest: `records_json().crestRanges` is not empty.
    var readoutHasCrest: Bool { get }
    /// The newest build, for `after_removal_json`.
    var readoutResult: BuildResult? { get }
    /// The record ids of `removed` fins this build matches (`matchRemoved`).
    func readoutRemovedIDs(_ removed: Set<FinKey>) -> [UInt64]
}

extension BuildModel: ReadoutBuildSource {
    var readoutSummaryJSON: String? { latest?.summaryJson() }
    var readoutFinCount: Int { records?.fins.count ?? 0 }
    var readoutResult: BuildResult? { latest }
    func readoutRemovedIDs(_ removed: Set<FinKey>) -> [UInt64] { matchedRemovals(of: removed).keys.map { UInt64($0) }.sorted() }
    var readoutHasCrest: Bool {
        guard let text = latest?.recordsJson(),
              let o = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any] else { return false }
        return !((o["crestRanges"] as? [Any])?.isEmpty ?? true)
    }
}

/// The stats card and the status card for the window's part, in the inspector's Part block (docs/UI-MAP.md
/// section 3 and 4.3). The text comes from `PartReadout` / `FitReadout` / `FinReadout`; this view only gathers
/// the inputs: the part's analysis (`Part.analyze`, rerun when the pose or the threshold changes), the session's
/// settings and the build.
///
/// Everything comes from the engine: `Part.analyze` (regions, raw regions, time), `Part.islands`, `PartInfo.weldMs`,
/// and the build's `summary_json` (pad, fill, `fillReach`, `addedBounds`) and `records_json` (`crestRanges`).
struct ReadoutSections: View {
    let controller: ViewerController
    let session: PartSession
    var build: (any ReadoutBuildSource)?
    /// Lines other features add to the status card.
    var extraLines: [StatusLine] = []
    /// The hand-placed supports (Features/Draw): their state for the Fins row and their volume and extent for the
    /// fit and the grams (`activeAdded`).
    var draw: DrawReadoutState?
    /// Rows other features add at the end of the stats card (`InspectorSlot.stats`).
    var statsRows: [InspectorSection] = []

    @State private var analysis: Analysis?
    @State private var afterRemoval: AfterRemoval?
    /// `flatRegions` (`part.js:255-258`): the part as loaded (identity pose) has this many overhang regions.
    @State private var flatRegions: Int?

    /// `after_removal_json`: what the readouts re-measure when fins are removed by hand (`ui/remove.js`).
    struct AfterRemoval: Decodable, Equatable {
        var removed: Int
        var bare: ReadoutBuilt.Bare?
        var addedBounds: ReadoutBuilt.Bounds?
        var addedVolumeMm3: Double
    }

    struct Analysis: Equatable {
        var summary: AnalysisSummary
        var islands: Int
        /// The starts themselves, for the viewer's cones; empty while a ring turns the part.
        var starts: [Island] = []
        var weldMs: Double
    }

    /// What the analysis depends on.
    private struct Key: Equatable {
        var pose: Pose
        var threshold: Double
        var parts: [ObjectIdentifier]
        /// A ring is turning the part: the starts are found after a drag, not during it (`ui/islands.js:11-12`).
        var dragging: Bool
    }

    var body: some View {
        let settings = session.settings
        let built = effectiveBuilt()
        let size = analysis?.summary.size ?? []
        let dims = size.count == 3 ? (x: size[0], y: size[1], z: size[2]) : (x: 0.0, y: 0.0, z: 0.0)
        let regions = Int(analysis?.summary.regionCount ?? 0)
        let part = PartReadout.make(PartReadoutInput(
            name: controller.meshName, triangles: controller.faceCount, size: dims,
            regionCount: regions, rawRegionCount: Int(analysis?.summary.rawRegionCount ?? 0),
            overArea: analysis?.summary.overArea ?? 0, bedArea: analysis?.summary.bedArea ?? 0,
            smallHoleCount: built?.smallHoleRegions ?? 0, islandCount: analysis?.islands ?? 0,
            highlightSmall: settings.highlightSmall, flatRegions: flatRegions,
            analysisMs: analysis?.summary.analyzeMs ?? 0, weldMs: analysis?.weldMs ?? 0))
        let fit = FitReadout.make(FitInput(
            size: dims, added: built?.addedBounds.map { (x0: $0.min[0], x1: $0.max[0], y0: $0.min[1], y1: $0.max[1], z1: $0.max[2]) },
            volume: (Double(controller.volume.x), Double(controller.volume.y), Double(controller.volume.z))))
        let fins = FinReadout.make(built, context(settings, built, regions))
        Group {
            StatsCard(fileName: controller.meshName, part: part, fit: analysis == nil ? nil : fit, fins: fins, rows: statsRows)
            StatusCard(part: part, fins: fins, importNote: session.parts.first?.info.importNote ?? "",
                       islandCard: islandCard(built, count: analysis?.islands ?? 0), extra: extraLines)
        }
        .task(id: RemovalKey(removed: session.removedFins, generation: build?.readoutResult.map(ObjectIdentifier.init))) { await measureRemoval() }
        .task(id: Key(pose: session.pose, threshold: session.settings.threshold,
                      parts: session.parts.map { ObjectIdentifier($0) },
                      dragging: controller.ringDragging != nil)) { await analyse() }
        .task(id: FlatKey(threshold: session.settings.threshold, parts: session.parts.map { ObjectIdentifier($0) })) { await measureFlat() }
    }

    /// The build's numbers, with the bare area, the extent and the volume measured again when fins are removed by hand.
    private func effectiveBuilt() -> ReadoutBuilt? {
        guard var b = build?.readoutSummaryJSON.flatMap({ try? ReadoutBuilt(summaryJSON: $0, finCount: build?.readoutFinCount ?? 0) })
        else { return nil }
        // the hand-placed supports are added to the plate too: the fit check and the grams count them
        if let added = draw?.added {
            b.finVolumeMm3 += added.volumeMm3
            if let lo = added.min, let hi = added.max {
                let cur = b.addedBounds
                b.addedBounds = ReadoutBuilt.Bounds(
                    min: [Swift.min(cur?.min[0] ?? lo.x, lo.x), Swift.min(cur?.min[1] ?? lo.y, lo.y), Swift.min(cur?.min[2] ?? lo.z, lo.z)],
                    max: [Swift.max(cur?.max[0] ?? hi.x, hi.x), Swift.max(cur?.max[1] ?? hi.y, hi.y), Swift.max(cur?.max[2] ?? hi.z, hi.z)])
            }
        }
        if let r = afterRemoval, !session.removedFins.isEmpty, r.removed > 0 {
            b.bare = r.bare
            b.addedBounds = r.addedBounds
            b.finVolumeMm3 = r.addedVolumeMm3 - b.padVolumeMm3
        }
        return b
    }

    /// The midair-starts card (`renderIslandCard`): nil when there is none to show.
    private func islandCard(_ built: ReadoutBuilt?, count: Int) -> String? {
        let full = built?.islandsBuild
        let bare: UInt64? = full.map { $0.all == true ? UInt64(count) : UInt64($0.bare ?? 0) }
        let text = SupportFinsCore.islandCard(count: UInt64(count), bare: bare, capped: full?.capped ?? false,
                                              markersVisible: session.settings.showIslands)
        return text.isEmpty ? nil : text
    }

    private struct RemovalKey: Equatable {
        var removed: Set<FinKey>
        var generation: ObjectIdentifier?
    }

    private func measureRemoval() async {
        guard let build, let result = build.readoutResult, !session.removedFins.isEmpty else { afterRemoval = nil; return }
        let ids = build.readoutRemovedIDs(session.removedFins)
        let text = await Task.detached(priority: .userInitiated) { try? result.afterRemovalJson(removedIds: ids) }.value
        if !Task.isCancelled { afterRemoval = text.flatMap { try? JSONDecoder().decode(AfterRemoval.self, from: Data($0.utf8)) } }
    }

    private func context(_ s: FinSettings, _ built: ReadoutBuilt?, _ regions: Int) -> ReadoutContext {
        var c = ReadoutContext()
        c.mode = s.mode.rawValue
        c.removed = session.removedFins.count
        c.regions = regions
        c.tineDir = s.tineDirection.rawValue
        c.padStyle = s.bedPad == .off ? "auto" : s.bedPad.rawValue
        c.padCustomGap = s.bedPad == .custom ? s.padGap : 0
        c.iface = s.interfaceMaterial == .off ? nil : s.interfaceMaterial.rawValue
        c.analysisTiming = analysis.map { "\(JS.toFixed0($0.summary.analyzeMs)) ms · weld \(JS.toFixed0($0.weldMs)) ms" } ?? ""
        if let b = built {
            c.hasAdded = b.finCount > 0 || b.pad != nil
            c.volumeMm3 = b.finVolumeMm3 + b.padVolumeMm3
            c.wallsPresent = b.finCount > 0
        }
        c.crestPresent = build?.readoutHasCrest ?? false
        if let d = draw {
            c.draw = d.input
            if d.added != nil { c.hasAdded = true; c.wallsPresent = true }
        }
        c.fillReach = built?.fillReach
        return c
    }

    private struct FlatKey: Equatable {
        var threshold: Double
        var parts: [ObjectIdentifier]
    }

    /// `computeFlatBaseline` (`part.js:256-258`): the overhang regions of the part as loaded, which depends on the
    /// threshold and the part, never on the pose. Read by the "prints clean lying flat" note.
    private func measureFlat() async {
        guard let core = session.parts.first else { flatRegions = nil; return }
        let part = core.part, threshold = session.settings.threshold
        let n = await Task.detached(priority: .userInitiated) {
            (try? part.analyze(thresholdDeg: threshold, rot: [1, 0, 0, 0, 1, 0, 0, 0, 1]))
                .map { Int($0.regionCount) }
        }.value
        if !Task.isCancelled { flatRegions = n }
    }

    private func analyse() async {
        guard let core = session.parts.first else { analysis = nil; controller.islandMarkers = []; return }
        let part = core.part, threshold = session.settings.threshold
        let rot = CorePart.columnMajor(session.pose.float3x3)
        let weld = core.info.weldMs
        let dragging = controller.ringDragging != nil
        let kept = analysis?.islands ?? 0
        let result = await Task.detached(priority: .userInitiated) { () -> Analysis? in
            guard let summary = try? part.analyze(thresholdDeg: threshold, rot: rot) else { return nil }
            // mid-drag none are found, and the last count stands so the notes do not flash (`hadIslands`)
            let starts = dragging ? [] : ((try? part.islands(thresholdDeg: threshold, rot: rot)) ?? [])
            return Analysis(summary: summary, islands: dragging ? kept : starts.count, starts: starts, weldMs: weld)
        }.value
        if Task.isCancelled { return }
        analysis = result
        // One model on the plate: the engine seats a part the way the viewer does. A group is seated as one.
        controller.islandMarkers = session.parts.count == 1
            ? (result?.starts ?? []).map { s in
                IslandMarker(tip: SIMD3(Float(s.x), Float(s.y), Float(s.z)),
                             points: stride(from: 0, to: s.points.count - 2, by: 3).map { SIMD3(Float(s.points[$0]), Float(s.points[$0 + 1]), Float(s.points[$0 + 2])) })
            } : []
    }
}
