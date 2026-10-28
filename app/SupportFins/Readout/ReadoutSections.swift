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
}

extension BuildModel: ReadoutBuildSource {
    var readoutSummaryJSON: String? { latest?.summaryJson() }
    var readoutFinCount: Int { records?.fins.count ?? 0 }
}

/// The stats card and the status card for the window's part, in the inspector's Part block (docs/UI-MAP.md
/// section 3 and 4.3). The text comes from `PartReadout` / `FitReadout` / `FinReadout`; this view only gathers
/// the inputs: the part's analysis (`Part.analyze`, rerun when the pose or the threshold changes), the session's
/// settings and the build.
///
/// PLACEHOLDER(rust) inputs, to be replaced when `summary_json` / `AnalysisSummary` carry them (the list in the
/// readout PR): `rawRegionCount` (= `regionCount`, so no sliver warning), the island count (0), the weld time (0),
/// the fin+pad extent for "does not fit (with fins)" (none), the pad details, `fill` / `curvedFill`, `fillReach`
/// and the crest flag.
struct ReadoutSections: View {
    let controller: ViewerController
    let session: PartSession
    var build: (any ReadoutBuildSource)?

    @State private var analysis: Analysis?

    struct Analysis: Equatable {
        var summary: AnalysisSummary
        var ms: Double
    }

    /// What the analysis depends on.
    private struct Key: Equatable {
        var pose: Pose
        var threshold: Double
        var parts: [ObjectIdentifier]
    }

    var body: some View {
        let settings = session.settings
        let built = build?.readoutSummaryJSON.flatMap { try? ReadoutBuilt(summaryJSON: $0, finCount: build?.readoutFinCount ?? 0) }
        let size = analysis?.summary.size ?? []
        let dims = size.count == 3 ? (x: size[0], y: size[1], z: size[2]) : (x: 0.0, y: 0.0, z: 0.0)
        let regions = Int(analysis?.summary.regionCount ?? 0)
        let part = PartReadout.make(PartReadoutInput(
            name: controller.meshName, triangles: controller.faceCount, size: dims,
            regionCount: regions, rawRegionCount: regions, // PLACEHOLDER(rust): slivers
            overArea: analysis?.summary.overArea ?? 0, bedArea: analysis?.summary.bedArea ?? 0,
            smallHoleCount: built?.smallHoleRegions ?? 0, islandCount: 0, // PLACEHOLDER(rust): islands
            highlightSmall: settings.highlightSmall, flatRegions: nil,
            analysisMs: analysis?.ms ?? 0, weldMs: 0)) // PLACEHOLDER(rust): weld time
        let fit = FitReadout.make(FitInput(
            size: dims, added: nil, // PLACEHOLDER(rust): the fins' and pad's extent
            volume: (Double(controller.volume.x), Double(controller.volume.y), Double(controller.volume.z))))
        let fins = FinReadout.make(built, context(settings, built, regions))
        Group {
            StatsCard(fileName: controller.meshName, part: part, fit: analysis == nil ? nil : fit, fins: fins)
            StatusCard(part: part, fins: fins)
        }
        .task(id: Key(pose: session.pose, threshold: session.settings.threshold,
                      parts: session.parts.map { ObjectIdentifier($0) })) { await analyse() }
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
        c.analysisTiming = analysis.map { "\(JS.toFixed0($0.ms)) ms" } ?? ""
        if let b = built {
            c.hasAdded = b.finCount > 0 || b.pad != nil
            c.volumeMm3 = b.finVolumeMm3 + b.padVolumeMm3
            c.wallsPresent = b.finCount > 0
        }
        // PLACEHOLDER(rust): crestPresent (crestRanges), fillReach
        return c
    }

    private func analyse() async {
        guard let core = session.parts.first else { analysis = nil; return }
        let part = core.part, threshold = session.settings.threshold
        let rot = CorePart.columnMajor(session.pose.float3x3)
        let result = await Task.detached(priority: .userInitiated) { () -> Analysis? in
            let start = ContinuousClock.now
            guard let summary = try? part.analyze(thresholdDeg: threshold, rot: rot) else { return nil }
            let d = ContinuousClock.now - start
            return Analysis(summary: summary, ms: Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15)
        }.value
        if !Task.isCancelled { analysis = result }
    }
}
