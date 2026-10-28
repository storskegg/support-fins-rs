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
        case .stl: "STL"
        case .threeMF: "3MF"
        case .finsOnly: "Fins only (STL)"
        }
    }

    /// The web's tooltips (`index.html:118-121`).
    var help: String {
        switch self {
        case .stl: "Part and fins merged into one solid."
        case .threeMF: "Part and supports as two separate objects, so the slicer keeps them apart and the tines only touch the part: they snap off cleaner. Opens in place in Bambu Studio, OrcaSlicer and PrusaSlicer; don't Arrange or move one without the other. 'No config, geometry only' is expected."
        case .finsOnly: "Just the fins and pad, in the part's print position, so they line up if imported beside it."
        }
    }

    var fileExtension: String { self == .threeMF ? "3mf" : "stl" }
    var contentType: UTType { self == .threeMF ? UTType(filenameExtension: "3mf") ?? .data : .stl }
    /// The stem the web puts after the part's name (`ui/export.js:150-186`: `fins` or `fins-only`).
    var stem: String { self == .finsOnly ? "fins-only" : "fins" }
    /// `export(options_json:)`
    var optionsJSON: String { self == .finsOnly ? #"{"finsOnly":true}"# : "{}" }
}

/// Export file names (`web/exportname.js`, GitHub #279): the fins are built for one nozzle and one layer
/// height, and the slicer has to be set to the same two, so the name carries both:
/// `part-fins` + `-0.6nozzle-0.24layer` + `.3mf`.
enum ExportName {
    /// `"0.24"`, `"0.6"`: `+Number(v).toFixed(2)` printed: at most two decimals, no trailing zeros, no float
    /// noise. `toFixed` rounds the exact binary value half up, so the digits are read from its exact decimal
    /// expansion (never from `v * 100`, which rounds first).
    static func mm(_ v: Double) -> String {
        // `String(Infinity)`; `toFixed` itself returns `String(x)` from 1e21 on (here: "1e+21")
        guard v.isFinite, abs(v) < 1e21 else { return v.isInfinite ? (v < 0 ? "-Infinity" : "Infinity") : "\(v)" }
        // the exact expansion of a double has at most 1074 places; 60 is far past any value that matters
        let exact = String(format: "%.60f", abs(v))
        let parts = exact.split(separator: ".", omittingEmptySubsequences: false)
        var whole = String(parts[0])
        let frac = Array(parts[1].utf8.map { Int($0) - 48 })
        var cents = frac[0] * 10 + frac[1]
        // half up on what follows the second place (any non-zero digit past a leading 5 is above half)
        if frac[2] >= 5 { cents += 1 }
        if cents >= 100 {
            cents -= 100
            whole = String((Int(whole) ?? 0) + 1)
        }
        var s = whole
        if cents != 0 {
            s += cents % 10 == 0 ? ".\(cents / 10)" : String(format: ".%02d", cents)
        }
        return v < 0 && s != "0" ? "-" + s : s
    }

    /// `exportName(stem, ext, { nozzle, layerHeight })` (`web/exportname.js:16`). A setting that isn't a
    /// positive number is left out of the name.
    static func name(stem: String, ext: String, nozzle: Double?, layerHeight: Double?) -> String {
        var tags: [String] = []
        if let n = nozzle, n > 0 { tags.append("\(mm(n))nozzle") }
        if let l = layerHeight, l > 0 { tags.append("\(mm(l))layer") }
        return ([stem] + tags).joined(separator: "-") + "." + ext
    }

    /// The part's name without its mesh extension, or `part` (`ui/export.js:63`).
    static func base(of partName: String) -> String {
        let stripped = partName.replacing(/\.(stl|3mf|step|stp)$/.ignoresCase(), with: "")
        return stripped.isEmpty ? "part" : stripped
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

/// What File > Export offers right now and writes it: one part and its build per file, through the engine's
/// `export` (`docs/FFI.md`, the bytes the CLI writes). Several objects in one file are written one file per
/// object until the plate layout of M6 (a folder is asked for, the files are named per object).
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
    @ObservationIgnored private let objectCount: @MainActor () -> Int
    /// The format ⌘E writes again.
    private(set) var lastFormat = ExportFormat.stl

    init(build: BuildModel, finsOn: @escaping @MainActor () -> Bool, source: @escaping @MainActor () -> Source?,
         objectCount: @escaping @MainActor () -> Int = { 1 }, panel: any SavePanel = SystemSavePanel()) {
        self.build = build
        self.objectCount = objectCount
        self.source = source
        self.finsOn = finsOn
        self.panel = panel
    }

    /// `setOpen` of `ui/export.js:192-216`: nothing to write until a part is loaded and its fins are built,
    /// and nothing while a rebuild is out (the old fins under the new settings' name); no fins-only file
    /// without fins.
    ///
    /// Divergence: the web also writes STL and 3MF with the fins off (the part alone). The engine's `export`
    /// takes a build, so without one (fins off) STL and 3MF wait for Add fins.
    var availability: Availability {
        guard source() != nil else { return Availability(enabled: [], note: nil) }
        guard let result = build.latest else {
            return Availability(enabled: [], note: finsOn() ? nil : "No fins yet: click Add fins first.")
        }
        guard !build.building else { return Availability(enabled: [], note: nil) }
        let hasFins = result.fins().count() + result.pad().count() > 0
        return Availability(
            enabled: hasFins ? Set(ExportFormat.allCases) : [.stl, .threeMF],
            note: hasFins ? nil : "No supports on this plate: STL and 3MF write the part alone.")
    }

    /// Said in the menu when the file holds several objects (UI-MAP "multi-object files one part per file until
    /// M6"): they would need a plate layout the engine does not have.
    var multiObjectNote: String? {
        objectCount() > 1 ? "This file has \(objectCount()) objects: export writes the one being edited. One file per object until plates arrive." : nil
    }

    /// The suggested file name: `part-fins-0.4nozzle-0.2layer.stl`.
    func fileName(for format: ExportFormat) -> String? {
        guard let s = source() else { return nil }
        return ExportName.name(stem: "\(ExportName.base(of: s.partName))-\(format.stem)", ext: format.fileExtension,
                               nozzle: s.nozzle, layerHeight: s.layerHeight)
    }

    /// Asks where, then writes. Returns the file written, or nil when cancelled or failed (`error` says why).
    @discardableResult
    func run(_ format: ExportFormat) -> URL? {
        guard availability.enabled.contains(format), let name = fileName(for: format),
              let url = panel.url(suggesting: name, type: format.contentType) else { return nil }
        lastFormat = format
        return write(format, to: url)
    }

    /// Writes `format` to `url` (the extension picks STL or 3MF in the engine, so it must match).
    @discardableResult
    func write(_ format: ExportFormat, to url: URL) -> URL? {
        error = nil
        guard let s = source(), let result = build.latest else { return nil }
        do {
            try export(path: url.path(percentEncoded: false), parts: [s.part], builds: [result],
                       optionsJson: format.optionsJSON)
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
