import Viewer
import simd

/// The rotate rings' events as session edits: one undo step per drag (`ui/pose.js`, `history.js:111`). The
/// viewer turns the part itself while the pointer is down; this puts the same turn in the pose, in double
/// precision, as the world rotation `delta` applied to the pose the drag began from.
@MainActor
final class RingEdits {
    private let session: PartSession
    private var start: Pose?
    /// Told of every event once the pose has taken it (the Rotate popover's delta and the badge's angle).
    var onEvent: ((ViewerController.RingEvent) -> Void)?

    init(session: PartSession, controller: ViewerController) {
        self.session = session
        controller.onRing = { [weak self] event in self?.handle(event) }
    }

    func handle(_ event: ViewerController.RingEvent) {
        defer { onEvent?(event) }
        switch event {
        case .began:
            start = session.pose
            session.beginGesture()
            session.ringDragBegan()
        case .turned(let delta, _, _):
            guard let start else { return }
            session.pose = Pose(rotation: delta * start.rotation)
        case .ended:
            // a drag that ended where it began leaves the pose as it was, so no step is recorded
            session.commitGesture(name: JS.t("Rotate"))
            session.ringDragging = false                  // after the final pose is in: one reaction sees both
            start = nil
        }
    }
}
