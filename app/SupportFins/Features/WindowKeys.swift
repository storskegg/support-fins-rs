import AppKit
import SwiftUI
import Viewer

/// A window-wide key handler (a local `NSEvent` monitor): the web's capturing `keydown` on the page. It lives as long
/// as the feature and is removed with it. The handler gets every key of the app; it checks the window itself
/// (`ViewerController.isInKeyWindow`) and returns true to consume the key.
final class KeyMonitor: @unchecked Sendable {
    private let token: Any?

    @MainActor init(handler: @escaping @MainActor (NSEvent) -> Bool) {
        token = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            MainActor.assumeIsolated { handler(event) } ? nil : event
        }
    }

    deinit { if let token { NSEvent.removeMonitor(token) } }
}

extension NSEvent {
    /// Backspace or forward Delete, as the viewer names them.
    var deleteKey: ViewerController.HandKey? {
        switch keyCode {
        case 51: .backspace
        case 117: .delete
        default: nil
        }
    }
}

extension NSWindow {
    /// The keys belong to a field: a text field or editor (the page skips a `textarea` and a text `input`), and a
    /// slider or pop-up (an `input` or `select`, which only undo and redo pass).
    var focusIsInAField: Bool {
        guard let r = firstResponder else { return false }
        return r is NSText || r is NSTextField || r is NSSlider || r is NSPopUpButton
    }
}

/// Edit > Delete from anywhere in the window. The system's item sends `delete:` up the responder chain from the first
/// responder; only the viewer answered it, so it was dead with the focus in the inspector. This responder sits in the
/// chain after the window (the chain ends there for every view), answers `delete:` as Backspace and enables the item
/// while that would do something. A text field takes its own `delete:` first.
final class DeleteResponder: NSResponder, NSMenuItemValidation {
    weak var model: DrawModel?

    @objc func delete(_ sender: Any?) {
        MainActor.assumeIsolated { model?.deleteMenu() }
    }

    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard item.action == #selector(delete(_:)) else { return false }
        return MainActor.assumeIsolated { model?.canDelete ?? false }
    }
}

struct DeleteResponderInstaller: NSViewRepresentable {
    let model: DrawModel

    func makeNSView(context: Context) -> Probe { Probe() }
    func updateNSView(_ v: Probe, context: Context) { v.responder.model = model }

    final class Probe: NSView {
        let responder = DeleteResponder()

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window, window.nextResponder !== responder else { return }
            responder.nextResponder = window.nextResponder
            window.nextResponder = responder
        }
    }
}
