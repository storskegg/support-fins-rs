import Observation
import Viewer

/// "Lay a face flat" (`lay-face`, `web/ui/pose.js:147-330`): armed behind a button so a stray click can't
/// re-lay the part. While armed the viewer lights the face under the pointer and reports a click; the click lays
/// that face on the plate as one undo step and disarms (one-shot). Esc, right click or Ctrl+click cancel.
@MainActor
@Observable
final class LayFaceModel {
    let session: PartSession
    @ObservationIgnored private weak var controller: ViewerController?
    /// `layPlacing`.
    private(set) var armed = false

    /// The button's text (`syncLayUI`, `ui/pose.js:216`).
    var title: String { armed ? JS.t("Click a face to lay it flat — Esc cancels") : JS.t("Lay a face flat") }

    init(session: PartSession, controller: ViewerController?) {
        self.session = session
        self.controller = controller
        controller?.onFaceClick = { [weak self] part, face in self?.click(part: part, face: face) }
        controller?.onFaceCancel = { [weak self] in self?.cancel() }
    }

    var canArm: Bool { !session.parts.isEmpty }

    /// `beginLay`: needs a part.
    func arm() {
        guard canArm, !armed else { return }
        armed = true
        controller?.facePicking = true
    }

    /// `cancelLay`.
    func cancel() {
        guard armed else { return }
        armed = false
        controller?.facePicking = false
    }

    func toggle() { armed ? cancel() : arm() }

    /// `layClick`: lay the face on the plate, one undo step, then disarm. A click that found no face (a part
    /// that is gone, a face out of range) leaves it armed, like `if (!hit) return`.
    func click(part: Int, face: Int) {
        guard armed, session.parts.indices.contains(part), let normal = session.parts[part].faceNormal(face) else { return }
        session.setPose(session.pose.laidFlat(onFaceNormal: normal), name: JS.t("Lay Face Flat"))
        cancel()
    }
}
