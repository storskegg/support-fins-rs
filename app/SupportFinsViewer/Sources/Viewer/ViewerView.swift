import AppKit
import MetalKit
import SwiftUI

/// The MTKView with the trackpad and mouse input: the Default preset of the web app (`web/ui/navpresets.js`)
/// plus the trackpad. Two-finger scroll orbits, pinch zooms, Shift or Option with the scroll pans, the
/// wheel of a mouse zooms; left drag orbits, right and middle drag pan; hovering picks a face.
public final class ViewerMTKView: MTKView {
    weak var controller: ViewerController?
    private var tracking: NSTrackingArea?

    public override var acceptsFirstResponder: Bool { true }

    private var scale: CGFloat { window?.backingScaleFactor ?? 2 }

    public override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let t = NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                               owner: self, userInfo: nil)
        addTrackingArea(t)
        tracking = t
    }

    @MainActor private func hover(_ e: NSEvent) {
        let p = convert(e.locationInWindow, from: nil)
        let s = scale
        controller?.hover(x: Int(p.x * s), y: Int((bounds.height - p.y) * s),
                          width: Int(bounds.width * s), height: Int(bounds.height * s))
    }

    public override func mouseMoved(with e: NSEvent) { MainActor.assumeIsolated { hover(e) } }
    public override func mouseExited(with e: NSEvent) { MainActor.assumeIsolated { controller?.clearHover() } }

    public override func mouseDragged(with e: NSEvent) {
        MainActor.assumeIsolated {
            controller?.clearHover()
            if e.modifierFlags.contains(.shift) { controller?.pan(dx: e.deltaX, dy: e.deltaY, viewHeight: bounds.height) }
            else { controller?.orbit(dx: e.deltaX, dy: e.deltaY, viewHeight: bounds.height) }
        }
    }
    public override func rightMouseDragged(with e: NSEvent) {
        MainActor.assumeIsolated { controller?.pan(dx: e.deltaX, dy: e.deltaY, viewHeight: bounds.height) }
    }
    public override func otherMouseDragged(with e: NSEvent) {
        MainActor.assumeIsolated { controller?.pan(dx: e.deltaX, dy: e.deltaY, viewHeight: bounds.height) }
    }

    public override func scrollWheel(with e: NSEvent) {
        MainActor.assumeIsolated {
            guard let c = controller else { return }
            if e.hasPreciseScrollingDeltas {
                // a trackpad: the scroll is the finger's drag
                if e.modifierFlags.contains(.shift) || e.modifierFlags.contains(.option) {
                    c.pan(dx: e.scrollingDeltaX, dy: e.scrollingDeltaY, viewHeight: bounds.height)
                } else {
                    c.orbit(dx: e.scrollingDeltaX, dy: e.scrollingDeltaY, viewHeight: bounds.height)
                }
            } else {
                c.zoom(wheelDelta: -e.scrollingDeltaY * 10)   // a wheel notch is about 10 web wheel units
            }
        }
    }

    public override func magnify(with e: NSEvent) {
        MainActor.assumeIsolated { controller?.zoom(magnification: e.magnification) }
    }

    /// Two-finger double tap: frame the part.
    public override func smartMagnify(with e: NSEvent) {
        MainActor.assumeIsolated { controller?.frame() }
    }
}

/// The viewer in SwiftUI: an `MTKView` whose delegate is the renderer, paced to the display (120 Hz on ProMotion).
public struct ViewerViewport: NSViewRepresentable {
    let controller: ViewerController

    public init(controller: ViewerController) { self.controller = controller }

    public func makeNSView(context: Context) -> ViewerMTKView {
        let view = ViewerMTKView(frame: .zero, device: controller.renderer.device)
        view.controller = controller
        view.delegate = controller.renderer
        view.colorPixelFormat = ViewerRenderer.colorFormat
        view.depthStencilPixelFormat = ViewerRenderer.depthFormat
        view.preferredFramesPerSecond = 120
        view.isPaused = false
        view.enableSetNeedsDisplay = false
        view.framebufferOnly = true
        view.autoResizeDrawable = true
        let bg = Palette.background
        view.clearColor = MTLClearColor(red: Double(bg.x), green: Double(bg.y), blue: Double(bg.z), alpha: 1)
        return view
    }

    public func updateNSView(_ view: ViewerMTKView, context: Context) {}
}
