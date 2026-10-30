import AppKit
import SwiftUI

/// The "Load anyway?" alert as a sheet on the window (an `NSAlert`: SwiftUI's `.alert` cannot give Return and Esc to
/// Cancel together, because `.defaultAction` on a cancel-role button takes Esc away).
///
/// Return and Esc both answer Cancel; Load anyway has no key equivalent and must be clicked. Every path ends by
/// answering the model, once (`ViewerModel.answerLimitPrompt` ignores a second answer): a button, Esc, the model
/// answering first (a test, a second question), the sheet ending, and the window closing.
@MainActor
final class LimitAlertPresenter {
    private weak var window: NSWindow?
    private(set) var alert: NSAlert?
    private var escMonitor: Any?
    private var closeObserver: Any?
    private let answer: (Bool) -> Void

    init(answer: @escaping (Bool) -> Void) { self.answer = answer }

    /// Follows the model's question: a prompt opens the sheet, none closes it.
    func update(_ prompt: LoadLimitPrompt?, window: NSWindow?) {
        if let prompt, alert == nil, let window {
            show(prompt, on: window)
        } else if prompt == nil {
            dismiss()
        }
    }

    private func show(_ prompt: LoadLimitPrompt, on window: NSWindow) {
        self.window = window
        let a = NSAlert()
        a.alertStyle = .informational
        a.messageText = prompt.title()
        a.informativeText = prompt.message()
        let cancel = a.addButton(withTitle: JS.t("Cancel"))      // first: Return (the default)
        let anyway = a.addButton(withTitle: JS.t("Load anyway"))
        anyway.keyEquivalent = ""
        anyway.setAccessibilityIdentifier("load-anyway")
        cancel.setAccessibilityIdentifier("load-cancel")
        alert = a
        // Esc is Cancel whatever the language calls it
        escMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard event.keyCode == 53, let self, let a = self.alert, a.window.isKeyWindow else { return event }
            self.finish(false)
            return nil
        }
        closeObserver = NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.finish(false) }
        }
        a.beginSheetModal(for: window) { [weak self] response in
            MainActor.assumeIsolated { self?.finish(response == .alertSecondButtonReturn) }
        }
    }

    /// Answers the model, takes the sheet down and forgets the alert; a second call does nothing.
    func finish(_ loadAnyway: Bool) {
        guard let a = alert else { return }         // already answered: the sheet's own completion comes second
        alert = nil
        if let m = escMonitor { NSEvent.removeMonitor(m) }
        escMonitor = nil
        if let o = closeObserver { NotificationCenter.default.removeObserver(o) }
        closeObserver = nil
        answer(loadAnyway)
        if let window, a.window.sheetParent != nil { window.endSheet(a.window) }
    }

    private func dismiss() { if alert != nil { finish(false) } }
}

/// Hands the SwiftUI view's window to `onWindow`.
struct WindowReader: NSViewRepresentable {
    var onWindow: (NSWindow?) -> Void
    func makeNSView(context: Context) -> NSView {
        let v = NSView()
        DispatchQueue.main.async { [weak v] in onWindow(v?.window) }
        return v
    }
    func updateNSView(_ v: NSView, context: Context) { DispatchQueue.main.async { [weak v] in onWindow(v?.window) } }
}
