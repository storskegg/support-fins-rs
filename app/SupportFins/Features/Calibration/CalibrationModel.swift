import AppKit
import Observation
import UniformTypeIdentifiers

/// Calibration prints (`web/ui/calibrate.js`): the sheet's state, the estimates and the Download of a coupon.
@MainActor
@Observable
final class CalibrationModel {
    /// The sheet is up (Help > Calibration Prints…).
    var isShowing = false
    private(set) var estimates: CouponEstimates?
    /// A download that failed, shown under the list.
    private(set) var error: String?

    @ObservationIgnored private let bundle: Bundle

    init(bundle: Bundle = .main) {
        self.bundle = bundle
        estimates = CalibrationCatalog.loadEstimates(in: bundle)
    }

    func show() { isShowing = true }

    /// Writes the coupon's file to `url` (the page's `<a download>`): the bundled bytes, unchanged.
    func export(_ coupon: Coupon, to url: URL) {
        error = nil
        guard let source = CalibrationCatalog.resourceURL(coupon.file, in: bundle) else {
            error = "\(coupon.file) is missing from the app."
            return
        }
        do {
            if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) }
            try FileManager.default.copyItem(at: source, to: url)
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
