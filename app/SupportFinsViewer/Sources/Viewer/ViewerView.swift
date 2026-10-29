import AppKit
import MetalKit
import SwiftUI

/// The MTKView with the trackpad and mouse input: the Mouse preset of the web app (`web/ui/navpresets.js`,
/// Default until the View popover says otherwise) plus the trackpad. Default: two-finger scroll orbits, pinch
/// zooms, Shift or Option with the scroll pans, the wheel of a mouse zooms; left drag orbits, middle drag
/// dollies, right drag pans; hovering picks a face.
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

    /// The event's position in drawable pixels, origin top left, with the drawable's size.
    private func pixel(_ e: NSEvent) -> (x: Int, y: Int, w: Int, h: Int) {
        let p = convert(e.locationInWindow, from: nil)
        let s = scale
        return (Int(p.x * s), Int((bounds.height - p.y) * s), Int(bounds.width * s), Int(bounds.height * s))
    }

    // A click is a press that did not drag (the web: pointerdown and up within 4 px, `app.js`): the viewer's
    // left drag orbits, so a press that moves is a turn, not a pick.
    private var pressAt: NSPoint?
    private static let clickSlop: CGFloat = 4

    /// A press that landed on a rotate ring turns the part; the drag belongs to the ring until the button is up.
    private var ringGrabbed = false
    /// A press that landed on the section plane's handle or sheet: the drag is the section's until the button is up.
    private var sectionGrabbed = false
    /// A press that landed on a model with Move on: the drag is Move's until the button is up.
    private var moveGrabbed = false

    public override func mouseDown(with e: NSEvent) {
        MainActor.assumeIsolated {
            window?.makeFirstResponder(self)
            // Ctrl+click is the Mac's right click: it cancels an armed mode
            if e.modifierFlags.contains(.control), controller?.cancelArmed() == true { pressAt = nil; return }
            let p = pixel(e)
            // the section's handle first (Move leaves a press to it: `planeBusy`, `ui/move.js:115-118`), then Move,
            // then the part's rings; Ctrl+click is a cancel, not a grab
            sectionGrabbed = !e.modifierFlags.contains(.control) && controller?.section.begin(x: p.x, y: p.y, width: p.w, height: p.h) == true
            if sectionGrabbed { pressAt = nil; return }
            // Move: a press on a model drags it, anywhere else orbits. The press stays a click candidate: a model
            // pressed and released in place is still clicked (a parked one is edited)
            moveGrabbed = !e.modifierFlags.contains(.control) && controller?.move.begin(x: p.x, y: p.y, width: p.w, height: p.h) == true
            if moveGrabbed { pressAt = e.locationInWindow; return }
            ringGrabbed = controller?.beginRingDrag(x: p.x, y: p.y, width: p.w, height: p.h) ?? false
            pressAt = ringGrabbed ? nil : e.locationInWindow
        }
    }

    public override func mouseUp(with e: NSEvent) {
        MainActor.assumeIsolated {
            if sectionGrabbed {
                controller?.section.ended()
                sectionGrabbed = false
                pressAt = nil
                return
            }
            if moveGrabbed {
                controller?.move.ended()
                moveGrabbed = false
            } else if ringGrabbed {
                controller?.endRingDrag()
                ringGrabbed = false
                pressAt = nil
                return
            }
            defer { pressAt = nil }
            guard let start = pressAt, hypot(e.locationInWindow.x - start.x, e.locationInWindow.y - start.y) <= Self.clickSlop else { return }
            let p = convert(e.locationInWindow, from: nil)
            let s = scale
            controller?.click(x: Int(p.x * s), y: Int((bounds.height - p.y) * s),
                              width: Int(bounds.width * s), height: Int(bounds.height * s))
        }
    }

    public override func rightMouseDown(with e: NSEvent) {
        MainActor.assumeIsolated { _ = controller?.cancelArmed() }
    }

    public override func keyDown(with e: NSEvent) {
        let handled = MainActor.assumeIsolated { () -> Bool in e.keyCode == 53 && controller?.cancelArmed() == true }
        if !handled { super.keyDown(with: e) }
    }

    public override func resetCursorRects() {
        let pointing = MainActor.assumeIsolated { controller.map { $0.hoveringSupport || $0.hoveringFaceToLay } == true }
        // the section sheet lights with a hand while a press would grab it
        let sheet = MainActor.assumeIsolated { () -> Int in
            guard let s = controller?.section else { return 0 }
            return s.sheetDragging ? 2 : s.hot ? 1 : 0
        }
        if sheet != 0 { addCursorRect(bounds, cursor: sheet == 2 ? .closedHand : .openHand); return }
        // Move: the grab hand over a model, the closed one while it is dragged (`ui/move.js:108-111, 130-134`)
        let hand = MainActor.assumeIsolated { () -> Int in
            guard let m = controller?.move, m.isOn else { return 0 }
            return m.dragging ? 2 : m.hover != nil ? 1 : 0
        }
        if hand != 0 { addCursorRect(bounds, cursor: hand == 2 ? .closedHand : .openHand); return }
        if pointing { addCursorRect(bounds, cursor: .pointingHand) }
    }

    public override func mouseMoved(with e: NSEvent) {
        MainActor.assumeIsolated {
            let p = pixel(e)
            controller?.pointerMoved(x: p.x, y: p.y, width: p.w, height: p.h)
        }
    }
    public override func mouseExited(with e: NSEvent) {
        MainActor.assumeIsolated { controller?.clearHover(); controller?.clearRingHover(); controller?.section.pointerLeft() }
    }

    public override func mouseDragged(with e: NSEvent) {
        MainActor.assumeIsolated {
            if sectionGrabbed {
                // Shift releases the 5 degree snap of the tilt rings (`web/ui/section.js`)
                let p = pixel(e)
                controller?.section.dragged(x: p.x, y: p.y, width: p.w, height: p.h, snap: !e.modifierFlags.contains(.shift))
                return
            }
            if moveGrabbed {
                let p = pixel(e)
                controller?.move.dragged(x: p.x, y: p.y, width: p.w, height: p.h)
                return
            }
            if ringGrabbed {
                // Shift releases the 5 degree snap (`web/ui/pose.js`)
                let p = pixel(e)
                controller?.dragRing(x: p.x, y: p.y, width: p.w, height: p.h, snap: !e.modifierFlags.contains(.shift))
                return
            }
            controller?.clearHover()
            navigate(.left, e)
        }
    }
    public override func rightMouseDragged(with e: NSEvent) { MainActor.assumeIsolated { navigate(.right, e) } }
    public override func otherMouseDragged(with e: NSEvent) { MainActor.assumeIsolated { navigate(.middle, e) } }

    /// A drag with a button down does what the Mouse preset says (`web/ui/navpresets.js`); Shift, Ctrl or Cmd
    /// swaps orbit and pan, as OrbitControls does.
    @MainActor private func navigate(_ button: NavPreset.Button, _ e: NSEvent) {
        guard let c = controller else { return }
        let chord = !e.modifierFlags.isDisjoint(with: [.shift, .control, .command])
        c.navigate(c.navPreset.dragAction(button, modifier: chord), dx: e.deltaX, dy: e.deltaY, viewHeight: bounds.height)
    }

    public override func layout() {
        super.layout()
        MainActor.assumeIsolated { controller?.viewHeight = bounds.height }
    }

    public override func scrollWheel(with e: NSEvent) {
        MainActor.assumeIsolated {
            guard let c = controller else { return }
            // Shift + scroll pushes the cut 1 mm a notch (the web's `deltaY || deltaX`), ahead of the navigation
            if c.section.isOn, e.modifierFlags.contains(.shift) {
                let d = e.scrollingDeltaY != 0 ? -e.scrollingDeltaY : -e.scrollingDeltaX
                if e.momentumPhase.isEmpty { c.section.scrollPush(Double(d)) }
                return
            }
            if e.hasPreciseScrollingDeltas {
                // a trackpad: the scroll is the finger's drag
                let chord = !e.modifierFlags.isDisjoint(with: [.shift, .option])
                c.navigate(c.navPreset.scrollAction(modifier: chord), dx: e.scrollingDeltaX, dy: e.scrollingDeltaY,
                           viewHeight: bounds.height)
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
        controller.viewport = view
        controller.onCursorChange = { [weak view] in if let view { view.window?.invalidateCursorRects(for: view) } }
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

    public func updateNSView(_ view: ViewerMTKView, context: Context) {
        controller.renderer.pixelScale = Float(view.window?.backingScaleFactor ?? 2)
    }
}
