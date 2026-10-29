import Foundation
import Observation
import simd

/// What the pointer can grab of the section plane: its arrow (push the cut along its normal), the two rings that
/// tilt it, and the outer view-plane ring (`E`: the rotate gizmo's, left on because `showXYZE`/`showZ` only drop
/// the middle ball and the ring about the normal, `web/ui/section.js:76-78`).
public enum SectionHandle: Equatable, Hashable, Sendable {
    case arrow, ringX, ringY, screen
}

/// The section view (`web/ui/section.js`): a plane the user moves and turns that hides everything on one side of
/// it. View only: it never reaches the build or the export, and it is not saved or undone. The renderer clips
/// the part and the supports by `clip` and draws the cut-away half as glass (`ghost`) or not at all; this
/// object holds the plane, its handle and the pointer's dealings with it.
///
/// The plane is held in the PART's frame (`followPart`): turn the part and the cut turns with it.
@MainActor
@Observable
public final class SectionTool {
    /// `sectionOn`.
    public private(set) var isOn = false
    /// Ghost (the web's default, `sf.sectionGhost`) or Hide.
    public var ghost = true { didSet { if ghost != oldValue { redraw() } } }
    public private(set) var plane = SectionPlane()
    /// The side of the square sheet, mm.
    public private(set) var sheetSize: Float = 1

    /// A press here would push the cut by its sheet (`hot`): the sheet lights and the cursor is a hand.
    @ObservationIgnored public private(set) var hot = false
    /// The handle under the pointer (the web's `g.axis`).
    @ObservationIgnored public private(set) var hover: SectionHandle?
    /// The handle being dragged.
    @ObservationIgnored public private(set) var dragging: SectionHandle?
    /// The sheet is being dragged.
    @ObservationIgnored public private(set) var sheetDragging = false
    /// The pointer is over the visible part (a click there is the part's, not the sheet's): set from the
    /// controller's face pick, which the clip already skips the cut half of (`keptHit`).
    @ObservationIgnored public var overPart = false { didSet { if overPart != oldValue { updateHot() } } }

    @ObservationIgnored weak var renderer: ViewerRenderer?
    @ObservationIgnored var onRedraw: (() -> Void)?
    @ObservationIgnored public var onCursorChange: (() -> Void)?
    /// The cut changed shape or place (the Section popover and the view's cursor follow).
    @ObservationIgnored public var onChange: (() -> Void)?

    /// `inPart`: the handle's pose in the part's frame.
    @ObservationIgnored private(set) var inPart = matrix_identity_float4x4
    @ObservationIgnored private var pointer: (x: Int, y: Int, w: Int, h: Int)?
    @ObservationIgnored private var sheetCandidate = false
    @ObservationIgnored private var drag: Drag?

    /// Same 5 degree snap as the part's rings; Shift frees it.
    public nonisolated static let snapDegrees: Double = 5
    /// `SHEET`, `SHEET_HOT`: the sheet's opacity, resting and lit.
    public nonisolated static let sheetOpacity: Float = 0.08
    public nonisolated static let sheetHotOpacity: Float = 0.18
    /// `HANDLE_PX`: a press this close to the handle's centre is the gizmo's (CSS pixels, i.e. points).
    public nonisolated static let handlePoints: Float = 80
    /// `size` of the gizmo (`g.setSize(0.7)`).
    public nonisolated static let gizmoSize: Float = 0.7
    public nonisolated static let ghostOpacity: Float = 0.14

    public init() {}

    private func redraw() { onRedraw?() }

    // MARK: on and off

    /// The part's world-space box: where the plane starts and how big it draws (`partBox`).
    var partBox: SectionBox? { renderer?.worldBounds }

    /// `setSection`: switch the cut on (aimed at the current view) or off. Nothing to cut without a part.
    public func setOn(_ on: Bool) {
        let hasPart = renderer.map { !$0.meshes.isEmpty } ?? false
        let now = on && hasPart
        if !now && !isOn { return }
        isOn = now
        if now {
            sizeSheet()
            aimAtView()
        } else {
            drag = nil
            dragging = nil
            sheetDragging = false
            hover = nil
            sheetCandidate = false
            hot = false
        }
        redraw()
        onChange?()
        onCursorChange?()
    }

    /// `sizeSheet`.
    private func sizeSheet() {
        guard let box = partBox else { return }
        sheetSize = SectionPlane.sheetSize(for: box)
    }

    /// `aimAtView` ("Re-aim"): through the part's centre, square to the part's axis nearest the view direction.
    public func aimAtView() {
        guard isOn, let r = renderer, let box = partBox else { return }
        plane = SectionPlane.aimed(look: r.camera.basis.forward, model: r.modelMatrix, box: box)
        holdInPart()
        redraw()
    }

    /// "Flip" (`section-flip`).
    public func flip() {
        guard isOn else { return }
        plane.flip()
        holdInPart()
        redraw()
    }

    /// Remember where the handle sits on the part (after the user or a button moved it).
    private func holdInPart() {
        guard let r = renderer else { return }
        inPart = r.modelMatrix.inverse * plane.matrix
    }

    /// The part turned or reseated (`followPart`): carry the cut along with it.
    func follow() {
        guard isOn, let r = renderer, drag == nil else { return }
        plane = SectionPlane(matrix: r.modelMatrix * inPart)
        redraw()
    }

    /// `sectionState` / `restoreSection`: the cut as the part holds it, for a plate that is parked.
    public var heldCut: simd_float4x4? { isOn ? inPart : nil }

    public func restore(_ m: simd_float4x4) {
        setOn(true)
        inPart = m
        follow()
    }

    /// `keptHit`: whether a point is on the visible side.
    public func keeps(_ p: SIMD3<Float>) -> Bool { !isOn || plane.distance(to: p) >= 0 }

    // MARK: shift + scroll

    /// Shift + scroll pushes the cut 1 mm a notch, scrolling away pushing it deeper (`web/ui/section.js:95-107`).
    /// `delta` is the web's `deltaY || deltaX`.
    public func scrollPush(_ delta: Double) {
        guard isOn, let r = renderer, delta != 0 else { return }
        let away: Float = simd_dot(r.camera.basis.forward, plane.normal) >= 0 ? 1 : -1
        plane.push(-(delta > 0 ? 1 : -1) * away)
        holdInPart()
        redraw()
    }

    // MARK: the gizmo

    /// The gizmo's geometry for the camera as it is now.
    var gizmo: SectionGizmo? {
        guard isOn, let r = renderer else { return nil }
        return SectionGizmo(plane: plane, camera: r.camera)
    }

    // MARK: the pointer (pixels of the drawable, origin top left)

    /// The pointer is on a handle, or dragging: the section owns the pointer and no pick lights up through it
    /// (`planeBusy`). A hot sheet alone is not busy.
    public var busy: Bool { isOn && (sheetDragging || dragging != nil || hover != nil) }

    /// Pointer moved without a button (`hoverSoon`).
    public func pointerMoved(x: Int, y: Int, width: Int, height: Int) {
        guard isOn, drag == nil else { return }
        pointer = (x, y, width, height)
        let handle = gizmo?.hit(atPixel: SIMD2(Float(x), Float(y)), width: Float(width), height: Float(height))
        if handle != hover { hover = handle; redraw() }
        sheetCandidate = handle == nil && sheetPoint(x: x, y: y, width: width, height: height) != nil
        updateHot()
    }

    public func pointerLeft() {
        guard drag == nil else { return }
        pointer = nil
        sheetCandidate = false
        if hover != nil { hover = nil; redraw() }
        updateHot()
    }

    private func updateHot() {
        let now = isOn && sheetCandidate && !overPart || sheetDragging
        guard now != hot else { return }
        hot = now
        redraw()
        onCursorChange?()
    }

    private func ray(x: Int, y: Int, width: Int, height: Int) -> (origin: SIMD3<Float>, direction: SIMD3<Float>)? {
        guard let r = renderer, width > 0, height > 0 else { return nil }
        return r.camera.ray(ndcX: 2 * Float(x) / Float(width) - 1, ndcY: 1 - 2 * Float(y) / Float(height))
    }

    /// Where a press would grab the sheet, or nil (off it, at the handle). Whether the part is under the pointer is
    /// `overPart`'s (`sheetPoint`).
    func sheetPoint(x: Int, y: Int, width: Int, height: Int) -> SIMD3<Float>? {
        guard isOn, let r = renderer, let ray = ray(x: x, y: y, width: width, height: height) else { return nil }
        let n = r.camera.project(plane.position)
        let c = SIMD2((n.x * 0.5 + 0.5) * Float(width), (0.5 - n.y * 0.5) * Float(height))
        if simd_length(SIMD2(Float(x), Float(y)) - c) < Self.handlePoints * r.pixelScale { return nil }
        let d = simd_dot(ray.direction, plane.normal)
        guard abs(d) > 1e-9 else { return nil }
        let t = simd_dot(plane.position - ray.origin, plane.normal) / d
        guard t > 0 else { return nil }
        let p = ray.origin + t * ray.direction
        let local = p - plane.position
        let half = sheetSize / 2
        guard abs(simd_dot(local, plane.axisX)) <= half, abs(simd_dot(local, plane.axisY)) <= half else { return nil }
        return p
    }

    // MARK: dragging

    private struct Drag {
        enum Kind {
            /// `pointStart` (relative to the handle), the handle's start, its axis.
            case arrow(start: SIMD3<Float>, position: SIMD3<Float>, axis: SIMD3<Float>, planeNormal: SIMD3<Float>)
            /// `facing` is the pointer plane's normal: the camera's view axis (`TransformControlsPlane` lays the
            /// rotate plane parallel to the camera), not `eye`, the direction from the centre to the camera.
            case ring(handle: SectionHandle, start: SIMD3<Float>, eye: SIMD3<Float>, facing: SIMD3<Float>, speed: Float,
                      orientation: simd_quatf)
            case sheet(p0: SIMD3<Float>, h0: SIMD3<Float>, n: SIMD3<Float>, y0: Int, wpp: Float)
        }
        var kind: Kind
    }

    /// A press: starts a drag when it lands on the arrow, a ring or the sheet. True when the section took it (no
    /// orbit, no pick). Ctrl+click is the Mac's right click: the caller keeps that out.
    public func begin(x: Int, y: Int, width: Int, height: Int) -> Bool {
        guard isOn, drag == nil, let r = renderer, let g = gizmo,
              let ray = ray(x: x, y: y, width: width, height: height) else { return false }
        pointer = (x, y, width, height)
        let center = plane.position
        if let handle = g.hit(atPixel: SIMD2(Float(x), Float(y)), width: Float(width), height: Float(height)) {
            let eye = simd_normalize(r.camera.eye - center)
            switch handle {
            case .arrow:
                let axis = plane.normal
                // the plane through the axis that faces the camera most (TransformControlsPlane)
                let pn = eye - axis * simd_dot(eye, axis)
                guard simd_length(pn) > 1e-6, let p = Self.hit(ray, planeNormal: simd_normalize(pn), through: center) else { return false }
                drag = Drag(kind: .arrow(start: p - center, position: center, axis: axis, planeNormal: simd_normalize(pn)))
            case .ringX, .ringY, .screen:
                let facing = r.camera.basis.forward
                guard let p = Self.hit(ray, planeNormal: facing, through: center) else { return false }
                // `ROTATION_SPEED = 20 / distance(camera, centre)`
                drag = Drag(kind: .ring(handle: handle, start: p - center, eye: eye, facing: facing,
                                        speed: 20 / simd_length(r.camera.eye - center), orientation: plane.orientation))
            }
            dragging = handle
            hover = handle
            redraw()
            return true
        }
        // the sheet: only where it is not over the solid part
        guard !overPart, let p0 = sheetPoint(x: x, y: y, width: width, height: height) else { return false }
        let dist = simd_length(r.camera.eye - p0)
        drag = Drag(kind: .sheet(p0: p0, h0: plane.position, n: plane.normal, y0: y,
                                 wpp: 2 * dist * tan(r.camera.fovY / 2) / Float(height)))
        sheetDragging = true
        updateHot()
        redraw()
        return true
    }

    private static func hit(_ ray: (origin: SIMD3<Float>, direction: SIMD3<Float>), planeNormal n: SIMD3<Float>,
                            through c: SIMD3<Float>) -> SIMD3<Float>? {
        let d = simd_dot(ray.direction, n)
        guard abs(d) > 1e-9 else { return nil }
        let t = simd_dot(c - ray.origin, n) / d
        guard t > 0 else { return nil }
        return ray.origin + t * ray.direction
    }

    /// The pointer moved with the button down. `snap` is false while Shift is held.
    public func dragged(x: Int, y: Int, width: Int, height: Int, snap: Bool) {
        guard let drag, let r = renderer, let ray = ray(x: x, y: y, width: width, height: height) else { return }
        switch drag.kind {
        case .arrow(let start, let position, let axis, let pn):
            guard let p = Self.hit(ray, planeNormal: pn, through: position) else { return }
            let offset = (p - position) - start
            plane.position = position + axis * simd_dot(offset, axis)       // `offset.projectOnVector(axis)`
        case .ring(let handle, let start, let eye, let facing, let speed, let q0):
            let center = plane.position
            guard let p = Self.hit(ray, planeNormal: facing, through: center) else { return }
            let end = p - center
            let offset = end - start
            let local: SIMD3<Float> = handle == .ringX ? SIMD3(1, 0, 0) : handle == .ringY ? SIMD3(0, 1, 0) : SIMD3(0, 0, 1)
            var axis = local
            var angle: Float = 0
            var inPlane = handle == .screen
            if !inPlane {
                // the ring's axis in the world, crossed with the eye
                let tangent = simd_cross(q0.act(local), eye)
                if simd_length(tangent) == 0 { inPlane = true } else { angle = simd_dot(offset, simd_normalize(tangent)) * speed }
            }
            if inPlane {
                axis = eye
                let a = simd_normalize(start), b = simd_normalize(end)
                angle = acos(Swift.min(Swift.max(simd_dot(a, b), -1), 1))      // `pointEnd.angleTo(pointStart)`
                angle *= simd_dot(simd_cross(b, a), eye) < 0 ? 1 : -1
            }
            if snap {
                let step = Float(Self.snapDegrees * .pi / 180)
                angle = (angle / step).rounded() * step
            }
            let turn = simd_quatf(angle: angle, axis: simd_normalize(axis))
            if handle != .screen { plane.orientation = (q0 * turn).normalized } else { plane.orientation = (turn * q0).normalized }
        case .sheet(let p0, let h0, let n, let y0, let wpp):
            let look = r.camera.basis.forward
            // Slide along the normal: on the plane through it that faces the camera most. Looking (nearly) square at
            // the sheet there is no such plane -- then up the screen pushes it away from you, like Shift + scroll.
            let across = look - n * simd_dot(look, n)
            var t: Float
            if simd_length(across) > 0.25 {
                guard let p = Self.hit(ray, planeNormal: simd_normalize(across), through: p0) else { return }
                t = simd_dot(p - p0, n)
            } else {
                let s: Float = simd_dot(look, n) > 0 ? 1 : simd_dot(look, n) < 0 ? -1 : 0
                t = Float(y0 - y) * wpp * s
            }
            if let box = partBox { t = SectionPlane.clampedPush(t, start: h0, normal: n, box: box) }
            plane.position = h0 + n * t
        }
        holdInPart()
        redraw()
    }

    /// The button is up.
    public func ended() {
        guard drag != nil else { return }
        drag = nil
        dragging = nil
        let wasSheet = sheetDragging
        sheetDragging = false
        if wasSheet, let p = pointer {
            sheetCandidate = sheetPoint(x: p.x, y: p.y, width: p.w, height: p.h) != nil && gizmo?.hit(
                atPixel: SIMD2(Float(p.x), Float(p.y)), width: Float(p.w), height: Float(p.h)) == nil
        }
        updateHot()
        onCursorChange?()
        redraw()
    }
}

extension simd_quatf {
    fileprivate var normalized: simd_quatf { simd_normalize(self) }
}
