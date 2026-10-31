import AppKit
import Observation
import SupportFinsCore
import UniformTypeIdentifiers

/// What the Plate ties print is made with, read from the window when it is saved: plain values, so the background
/// task that makes it captures nothing of the window.
struct TiePrintInputs: Sendable, Equatable {
    /// The printer profile (`PrinterProfileStore`) and its name.
    var profile: String
    var printer: String
    /// The window's fin settings (`FinSettings.optionsJSON()` / `tunablesJSON()`).
    var options: String
    var tunables: String
    /// The window's bed (`sf.volume`), x and y mm.
    var bed: [Double]

    /// `export_tie_calibration`'s export options: `{ "bed": [x, y] }`, or `{}` without a usable bed.
    var exportOptions: String {
        guard bed.count == 2, bed.allSatisfy({ $0.isFinite && $0 > 0 }) else { return "{}" }
        return "{\"bed\":[\(bed[0]),\(bed[1])]}"
    }
}

/// Calibration prints (`web/ui/calibrate.js`): the sheet's state, the estimates and the Download of a coupon; and the
/// app's own Plate ties print (issue #50), made by the engine when it is saved.
@MainActor
@Observable
final class CalibrationModel {
    /// The sheet is up (Help > Calibration Prints…).
    var isShowing = false
    private(set) var estimates: CouponEstimates?
    /// A download that failed, shown under the list.
    private(set) var error: String?

    @ObservationIgnored private let bundle: Bundle
    /// The window's printer and settings for the Plate ties print; nil when there is no window to read.
    @ObservationIgnored var tieInputs: () -> TiePrintInputs? = { nil }
    /// The Plate ties print's three thicknesses and the printer they are for, read when the sheet opens.
    private(set) var tieVariants: [TieCalibrationVariant] = []
    private(set) var tiePrinter = ""
    /// The Plate ties print is being made.
    private(set) var tieBusy = false
    /// What the last Plate ties print came to ("Plate ties: 9 of 9 ties on 3 pairs").
    private(set) var tieSummary: String?

    init(bundle: Bundle = .main) {
        self.bundle = bundle
        estimates = CalibrationCatalog.loadEstimates(in: bundle)
    }

    func show() {
        refreshTies()
        isShowing = true
    }

    /// The Plate ties print's thicknesses for the printer picked now.
    func refreshTies() {
        guard let i = tieInputs() else {
            tieVariants = []
            return
        }
        tieVariants = (try? tieCalibrationVariants(profileId: i.profile, settingsJson: "")) ?? []
        tiePrinter = i.printer
    }

    /// Makes the Plate ties print for the window's printer and settings and writes it to `url` (atomically). The engine
    /// runs off the main actor; only plain values go into the task.
    func exportTiePrint(to url: URL) async {
        error = nil
        tieSummary = nil
        guard !tieBusy else { return }
        // the row's Download is greyed without a window to read (no variants)
        guard let i = tieInputs() else { return }
        tieBusy = true
        defer { tieBusy = false }
        let outcome = await Task.detached(priority: .userInitiated) { () -> Result<TieCalibrationResult, any Error> in
            Result {
                var made: TieCalibrationResult?
                _ = try AtomicWrite.write(to: url) { path in
                    made = try exportTieCalibration(path: path, profileId: i.profile, settingsJson: "", optsJson: i.options,
                                                    tunablesJson: i.tunables, exportOptionsJson: i.exportOptions)
                }
                guard let made else { throw CocoaError(.fileWriteUnknown) }
                return made
            }
        }.value
        switch outcome {
        case .success(let r):
            tieVariants = r.variants
            tiePrinter = i.printer
            tieSummary = JS.t("Plate ties: {placed} of {requested} ties on {pairs} pairs",
                              ["placed": String(r.placed), "requested": String(r.requested), "pairs": String(r.variants.count)])
        case .failure(let e as FfiError):
            error = BuildModel.message(e)
        case .failure(let e):
            error = e.localizedDescription
        }
    }

    /// The Plate ties row's Download: a Save panel named for the print, then `exportTiePrint`.
    func downloadTiePrint(from window: NSWindow?) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = CalibrationCatalog.tiePrint.file
        panel.allowedContentTypes = [UTType(filenameExtension: "3mf")].compactMap { $0 }
        panel.canCreateDirectories = true
        panel.identifier = NSUserInterfaceItemIdentifier("calibration-save-panel")
        let finish: (NSApplication.ModalResponse) -> Void = { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor in await self?.exportTiePrint(to: url) }
        }
        if let window { panel.beginSheetModal(for: window, completionHandler: finish) } else { panel.begin(completionHandler: finish) }
    }

    /// Writes the coupon's file to `url` (the page's `<a download>`): the bundled bytes, unchanged.
    func export(_ coupon: Coupon, to url: URL) {
        error = nil
        guard let source = CalibrationCatalog.resourceURL(coupon.file, in: bundle) else {
            error = "\(coupon.file) is missing from the app."
            return
        }
        do {
            try AtomicWrite.copy(from: source, to: url)
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// The Download button: a Save panel named for the file (`go.download = c.file`), then `export`.
    func download(_ coupon: Coupon, from window: NSWindow?) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = coupon.file
        panel.allowedContentTypes = [UTType(filenameExtension: (coupon.file as NSString).pathExtension)].compactMap { $0 }
        panel.canCreateDirectories = true
        panel.identifier = NSUserInterfaceItemIdentifier("calibration-save-panel")
        let finish: (NSApplication.ModalResponse) -> Void = { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor in self?.export(coupon, to: url) }
        }
        if let window { panel.beginSheetModal(for: window, completionHandler: finish) } else { panel.begin(completionHandler: finish) }
    }
}
