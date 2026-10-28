import Observation
import SupportFinsCore
import simd

/// The Strength popover's state (`ui/strength.js`): the load direction, its verdict for the pose on screen, and
/// the "turn to the strongest printable pose" advice.
///
/// Only a direction is stored, in the part's own frame, so the arrow turns and re-seats with the part. The six
/// pad buttons are world directions (as the part sits on the bed); the load is `R⁻¹ · world`, and the verdict
/// is `loadAlignment` of `R · local` (world +Z is the build axis).
@MainActor
@Observable
final class StrengthModel {
    /// The pad (`PAD_DIRS`, `ui/strength.js:177-181`).
    enum Pad: String, CaseIterable, Identifiable {
        case up, down, left, right, front, back
        var id: String { rawValue }

        var world: SIMD3<Double> {
            switch self {
            case .up: [0, 0, 1]
            case .down: [0, 0, -1]
            case .right: [1, 0, 0]
            case .left: [-1, 0, 0]
            case .back: [0, 1, 0]
            case .front: [0, -1, 0]
            }
        }

        /// The labels and tooltips of `index.html` (`load-up` ... `load-back`).
        var label: String {
            switch self {
            case .up: "↑"
            case .down: "↓"
            case .left: "←"
            case .right: "→"
            case .front: "⊙ front"
            case .back: "⊗ back"
            }
        }

        var help: String {
            switch self {
            case .up: "Load points up"
            case .down: "Load points down"
            case .left: "Load points left"
            case .right: "Load points right"
            case .front: "Load points toward you"
            case .back: "Load points away from you"
            }
        }
    }

    /// The load, unit, in the part's own frame; nil when none is set.
    private(set) var loadLocal: SIMD3<Double>?
    /// `loadAlignment` for the pose on screen (nil: no load).
    private(set) var verdict: LoadAlignment?
    /// The pad button whose world direction the arrow points along now (cleared by turning the part off it).
    private(set) var activePad: Pad?
    /// "This is about the strongest printable orientation ..." once the advice said there is nothing better.
    private(set) var strongestNote: String?
    private(set) var thinking = false
    private(set) var error: String?

    /// The badge is lit while a load is set (`addBadge('strength-badge', { isOn: () => !!loadDir })`).
    var isLit: Bool { loadLocal != nil }
    /// The note under the pad: the strongest-pose note if shown, else the verdict's text.
    var note: String? { strongestNote ?? verdict?.text }
    /// The verdict's style: good / mixed / poor are the page's `good` / `ok` / `weak`.
    var quality: String? { strongestNote != nil ? (verdict?.quality ?? "mixed") : verdict?.quality }
    /// Offer "Turn to the strongest printable pose" (the verdict is not good, and the note is not up).
    var canSuggest: Bool { (verdict?.canSuggest ?? false) && strongestNote == nil }

    /// Tell the view the load changed: its direction in the part's own frame, or nil (the arrow follows).
    @ObservationIgnored var loadChanged: (SIMD3<Double>?) -> Void = { _ in }

    @ObservationIgnored private let suggester: Suggester?
    @ObservationIgnored private let pose: PoseAccess
    @ObservationIgnored private var sink: Sink!
    @ObservationIgnored private var wanted: UInt64?

    init(part: Part?, pose: PoseAccess) {
        suggester = part.map(Suggester.init(part:))
        self.pose = pose
        sink = Sink { [weak self] event in Task { @MainActor in self?.deliver(event) } }
    }

    /// Point the load along a world direction (one pad button).
    func setLoad(_ pad: Pad) {
        let local = Rot.applyInverse(pose.rotation(), pad.world)
        guard simd_length_squared(local) > 1e-9 else { return }
        loadLocal = simd_normalize(local)
        strongestNote = nil
        refresh()
        loadChanged(loadLocal)
    }

    func clear() {
        wanted = nil
        thinking = false
        loadLocal = nil
        strongestNote = nil
        refresh()
        loadChanged(nil)
    }

    /// The load in the current pose (world space).
    var loadWorld: SIMD3<Double>? { loadLocal.map { Rot.apply(pose.rotation(), $0) } }

    /// The verdict and the lit pad button follow the pose: call it after every turn (`updateLoadReadout`,
    /// `syncLoadUI`). A turn also takes the "already strongest" note down.
    func refresh() {
        guard let world = loadWorld else {
            verdict = nil
            activePad = nil
            return
        }
        verdict = try? loadAlignment(dirWorld: [world.x, world.y, world.z])
        activePad = Pad.allCases.first { simd_dot(world, $0.world) > 0.999 }
    }

    /// The pose changed: refresh, and drop a note that was about the old one.
    func poseChanged() {
        strongestNote = nil
        refresh()
    }

    /// "Turn to the strongest printable pose": ask the engine (off the main thread); it either turns the part or
    /// says this is already about as strong as it gets.
    func turnToStrongest() {
        guard let suggester, let local = loadLocal, let world = loadWorld, !thinking else { return }
        thinking = true
        error = nil
        wanted = suggester.strength(
            dirLocal: [local.x, local.y, local.z], dirWorld: [world.x, world.y, world.z], thresholdDeg: threshold,
            listener: sink)
    }

    /// The overhang threshold the advice is computed at (the inspector's).
    var threshold = 45.0

    fileprivate enum Event: Sendable {
        case advice(UInt64, StrengthAdvice)
        case failed(UInt64, String)
    }

    private func deliver(_ event: Event) {
        switch event {
        case .advice(let g, let advice):
            guard g == wanted else { return }
            wanted = nil
            thinking = false
            if let p = advice.pose {
                pose.apply(p.rot)         // turns the part; the caller's pose observer refreshes the verdict
                strongestNote = nil
                refresh()
            } else {
                strongestNote = advice.note
            }
        case .failed(let g, let message):
            guard g == wanted else { return }
            wanted = nil
            thinking = false
            error = message
        }
    }

    private final class Sink: SuggestListener, @unchecked Sendable {
        let send: @Sendable (Event) -> Void
        init(_ send: @escaping @Sendable (Event) -> Void) { self.send = send }
        func suggested(generation: UInt64, result: SuggestResult) {}
        func strength(generation: UInt64, advice: StrengthAdvice) { send(.advice(generation, advice)) }
        func failed(generation: UInt64, err: FfiError) { send(.failed(generation, err.engineMessage)) }
    }
}
