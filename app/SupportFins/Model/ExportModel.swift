import AppKit
import Foundation
import Observation
import SupportFinsCore
import UniformTypeIdentifiers

/// The three files File > Export writes (`web/ui/export.js` FORMATS, docs/UI-MAP.md rows `export-stl`,
/// `export-3mf`, `export-fins`). "All plates (3MF)" is M6.
enum ExportFormat: CaseIterable, Sendable {
    case stl, threeMF, finsOnly

    var label: String {
        switch self {
        case .stl: JS.t("STL")
        case .threeMF: "3MF"
        case .finsOnly: JS.t("Fins only (STL)")
        }
    }

    /// The web's tooltips (`index.html:118-121`).
    var help: String {
        switch self {
        case .stl: JS.t("Part and fins merged into one solid.")
        case .threeMF: JS.t("Part and supports as two separate objects, so the slicer keeps them apart and the tines only touch the part: they snap off cleaner. Opens in place in Bambu Studio, OrcaSlicer and PrusaSlicer; don't Arrange or move one without the other. 'No config, geometry only' is expected.")
        case .finsOnly: JS.t("Just the fins and pad, in the part's print position, so they line up if imported beside it.")
        }
    }

    var fileExtension: String { self == .threeMF ? "3mf" : "stl" }
    var contentType: UTType { self == .threeMF ? UTType(filenameExtension: "3mf") ?? .data : .stl }
    /// The engine's name for the file (`export_file_name`).
    var kind: ExportKind {
        switch self {
        case .stl: .stl
        case .threeMF: .threeMf
        case .finsOnly: .finsOnly
        }
    }
}

/// Asks where a file goes (an `NSSavePanel` in the app, a stub in tests).
@MainActor
protocol SavePanel {
    func url(suggesting name: String, type: UTType) -> URL?
}

struct SystemSavePanel: SavePanel {
    func url(suggesting name: String, type: UTType) -> URL? {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = name
        panel.allowedContentTypes = [type]
        panel.canCreateDirectories = true
        panel.isExtensionHidden = false
        return panel.runModal() == .OK ? panel.url : nil
    }
}

/// What File > Export offers right now and writes it, through the engine's `export`, which writes what the page
/// exports (`web/ui/export.js`, docs/FFI.md): the part as oriented and seated on the plate with the fins and
/// the pad (STL: one solid; 3MF: separate objects), just the fins and pad in print position for any pose
/// ("Fins only"), or the part alone while the fins are off; removed fins are left out. One part per file until
/// the plate layout of M6.
@MainActor
@Observable
final class ExportModel {
    /// What the export needs from the window.
    struct Source {
        var partName: String
        var part: Part
        /// the nozzle and layer height the fins were built for: they go in the file name
        var nozzle: Double
        var layerHeight: Double
        /// the pose (nine numbers, column-major) and the overhang threshold: the part alone is seated from them
        var pose: [Double]
        var threshold: Double
        /// the build volume's x and y: a 3MF is written where the part stands on a centred bed
        var bed: [Double]
        /// the model's spot on the plate (`liveXY()`, `ui/models.js:38`): `[0, 0]` until Move and Arrange (M6)
        var placed: [Double] = [0, 0]
    }

    struct Availability: Equatable {
        var enabled: Set<ExportFormat>
        /// `#export-nofins`: why nothing (or only the part) can be exported yet.
        var note: String?
    }

    /// The last export's failure (the engine's message), or nil.
    private(set) var error: String?
    private(set) var lastWritten: URL?

    @ObservationIgnored let build: BuildModel
    @ObservationIgnored private let source: @MainActor () -> Source?
    @ObservationIgnored private let finsOn: @MainActor () -> Bool
    @ObservationIgnored private let panel: any SavePanel
    @ObservationIgnored private let removedKeys: @MainActor () -> Set<FinKey>
    /// The format ⌘E writes again.
    var lastFormat = ExportFormat.stl
    /// The hand-placed supports of Draw (Features/Draw): written between the fins and the pad, and not offered while
    /// they are being re-stood.
    struct Drawn {
        var sweep: DrawnSweep?
        var pending: Bool
    }
    @ObservationIgnored var drawn: @MainActor () -> Drawn = { Drawn(sweep: nil, pending: false) }

    init(build: BuildModel, finsOn: @escaping @MainActor () -> Bool, source: @escaping @MainActor () -> Source?,
         removed: @escaping @MainActor () -> Set<FinKey> = { [] },
         panel: any SavePanel = SystemSavePanel()) {
        self.build = build
        self.removedKeys = removed
        self.source = source
        self.finsOn = finsOn
        self.panel = panel
    }

    /// `setOpen` of `ui/export.js:192-216`: nothing to write until a part is loaded, and nothing while a rebuild is
    /// out (the old fins under the new settings' name). With the fins off STL and 3MF write the part alone; Fins
    /// only needs fins.
    var availability: Availability {
        guard source() != nil else { return Availability(enabled: [], note: nil) }
        guard finsOn() else {
            return Availability(enabled: [.stl, .threeMF],
                                note: JS.t("No fins yet: STL and 3MF write the part alone. Click Add fins first."))
        }
        // fins on: they are the build, which must have landed
        guard let result = build.latest, !build.building, !drawn().pending else { return Availability(enabled: [], note: nil) }
        let hasFins = result.fins().count() + result.pad().count() + (drawn().sweep?.triangles().count() ?? 0) > 0
        return Availability(
            enabled: hasFins ? Set(ExportFormat.allCases) : [.stl, .threeMF],
            note: hasFins ? nil : JS.t("No supports on this plate: STL and 3MF write the part alone."))
    }

    /// The suggested file name: `part-fins-0.4nozzle-0.2layer.stl` (`export_file_name`, `ui/export.js` `plateName`).
    func fileName(for format: ExportFormat) -> String? {
        guard let s = source() else { return nil }
        return exportFileName(base: Self.base(of: s.partName), kind: format.kind, nozzle: s.nozzle, layerHeight: s.layerHeight)
    }

    /// The part's name without its mesh extension, or `part` (`ui/export.js:63`).
    nonisolated static func base(of partName: String) -> String {
        let stripped = partName.replacing(/\.(stl|3mf|step|stp)$/.ignoresCase(), with: "")
        return stripped.isEmpty ? "part" : stripped
    }

    /// Asks where, then writes. Returns the file written, or nil when cancelled or failed (`error` says why).
    @discardableResult
    func run(_ format: ExportFormat) -> URL? {
        guard availability.enabled.contains(format), let name = fileName(for: format),
              let url = panel.url(suggesting: name, type: format.contentType) else { return nil }
        lastFormat = format
        return write(format, to: url)
    }

    /// The engine's `export` options for `format` (docs/FFI.md): the fins off writes the part alone from the
    /// pose; removed fins are left out by their record ids; a 3MF carries the bed and the model's spot.
    func optionsJSON(for format: ExportFormat) -> String? {
        guard let s = source() else { return nil }
        var o: [String: Any] = [:]
        if format == .finsOnly { o["finsOnly"] = true }
        if !finsOn() {
            o["fins"] = false
            o["pose"] = s.pose
            o["threshold"] = s.threshold
        } else {
            let ids = build.matchedRemovals(of: removedKeys()).keys.sorted()
            if !ids.isEmpty { o["removed"] = ids }
        }
        if format == .threeMF {
            o["bed"] = s.bed
            o["placed"] = s.placed
        }
        guard let data = try? JSONSerialization.data(withJSONObject: o, options: [.sortedKeys]) else { return nil }
        return String(decoding: data, as: UTF8.self)
    }

    /// Writes `format` to `url` (the extension picks STL or 3MF in the engine, so it must match).
    @discardableResult
    func write(_ format: ExportFormat, to url: URL) -> URL? {
        error = nil
        guard let s = source(), let options = optionsJSON(for: format) else { return nil }
        // fins on: the build; off: the part alone, no build
        let builds: [BuildResult] = finsOn() ? build.latest.map { [$0] } ?? [] : []
        if finsOn() && builds.isEmpty { return nil }
        do {
            try exportWithDrawn(path: url.path(percentEncoded: false), parts: [s.part], builds: builds,
                                drawn: finsOn() ? drawn().sweep : nil, optionsJson: options)
            lastWritten = url
            return url
        } catch let e as FfiError {
            error = BuildModel.message(e)
        } catch {
            self.error = error.localizedDescription
        }
        return nil
    }
}
