import Foundation
import Observation
import Viewer

/// The Rotate popover's state (`ui/pose.js`, `index.html:637-651`): the delta of the last turn beside the live
/// readout, "Show rotate rings", and the angle flashed over the badge while a ring is dragged with the popover
/// folded. The pose itself is `PartSession`'s; every button is one undo step there.
@MainActor
@Observable
final class RotateModel {
    let session: PartSession
    /// Where the drag angle is flashed; set by the window.
    @ObservationIgnored weak var registry: ToolbarRegistry?
    @ObservationIgnored weak var controller: ViewerController?
    @ObservationIgnored private let defaults: UserDefaults

    /// `#rot-delta`: "Z +90°", "free -12.5°", or empty (`showDelta`, `ui/pose.js:106`).
    private(set) var delta = ""
    /// `show-rings`, remembered like the web does (`sf.showRings`, `ui/pose.js:161-176`).
    var showRings: Bool {
        didSet {
            defaults.set(showRings, forKey: Self.ringsKey)
            controller?.showRings = showRings
        }
    }
    /// A ring is being dragged right now (`ringDrag`, `ui/pose.js:118`).
    private(set) var ringDrag = false
    /// What the flash over the badge last said for the delta (`#drag-delta` keeps its text after release).
    @ObservationIgnored private var flashDelta = ""

    static let ringsKey = "sf.showRings"
    /// `DRAG_FADE_MS` (`ui/pose.js:127`).
    static let dragFadeSeconds = 1.2

    init(session: PartSession, controller: ViewerController? = nil, defaults: UserDefaults = .standard) {
        self.session = session
        self.controller = controller
        self.defaults = defaults
        showRings = defaults.object(forKey: Self.ringsKey) == nil ? true : defaults.bool(forKey: Self.ringsKey)
        controller?.showRings = showRings
    }

    var hasPart: Bool { !session.parts.isEmpty }
    /// `#rot-now`.
    var now: String { session.pose.euler.readout }

    // MARK: buttons

    /// Rotate 90° X / Y / Z: one step, and the delta says which (`rotate90`, `ui/pose.js:228-236`).
    func rotate90(about axis: Pose.Axis) {
        guard hasPart else { return }
        session.rotate90(about: axis)
        delta = Self.deltaText(name: axis.label, degrees: 90) ?? delta
    }

    /// Reset (`ui/pose.js:241-248`): one step, and the delta is cleared.
    func reset() {
        guard hasPart else { return }
        session.resetPose()
        delta = ""
    }

    // MARK: ring drags

    /// A ring event, after `RingEdits` has put it in the pose.
    func ring(_ event: ViewerController.RingEvent) {
        switch event {
        case .began:
            ringDrag = true
            flash()
        case .turned(_, let degrees, let handle):
            if let text = Self.deltaText(name: handle.name, degrees: degrees) { delta = text }
            flash()
        case .ended:
            ringDrag = false
            delta = ""                        // `el('rot-delta').textContent = ''` (pose.js:56)
            flash(hold: false)                // the settled angle, then fade (`endDragAngle`)
        }
    }

    /// `showDragAngle` / `endDragAngle` (`ui/pose.js:121-146`): not while the popover is open, where the readout
    /// already says it.
    private func flash(hold: Bool = true) {
        guard let registry, registry.open != .rotate else { registry?.flash = nil; return }
        if hold { flashDelta = delta }
        let text = [flashDelta, now].filter { !$0.isEmpty }.joined(separator: "  ")
        registry.flash(text, over: .rotate, seconds: hold ? nil : Self.dragFadeSeconds)
    }

    // MARK: the delta's wording

    /// `showDelta` (`ui/pose.js:106-113`): nothing for a zero turn; else the handle's name ('XYZE', the free
    /// trackball, reads "free"), a sign, and the angle to a decimal only when it has a fraction.
    static func deltaText(name: String, degrees deg: Double) -> String? {
        guard deg != 0 else { return nil }
        let label = name.count > 1 ? "free" : name
        let fraction = deg.truncatingRemainder(dividingBy: 1) != 0
        let mag = abs(deg)
        let digits = fraction ? JS.toFixed1(mag) : JS.toFixed0(mag)
        return "\(label) \(deg >= 0 ? "+" : "-")\(digits)°"
    }
}
