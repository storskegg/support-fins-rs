import Foundation
import Observation
import SupportFinsCore

/// Scale (`web/ui/scale.js`): one uniform percent for the part. A new scale reloads the part at that size
/// (`setPart`): the pose is kept; drawn walls, removed fins and undo start over, as for a new file. The part as
/// loaded is kept (`base`) so every scale is taken from it, never compounded.
@MainActor
@Observable
final class ScaleModel {
    /// `scale`: the factor in force (1 = 100 %).
    private(set) var scale = 1.0
    /// The part at 100 %.
    @ObservationIgnored private(set) var base: Part?
    /// Reloads the part on show at the new size (the window's `reshow`).
    @ObservationIgnored var apply: ((Part) -> Void)?
    /// Bumped by each request, so a slow rescale that is overtaken is dropped.
    @ObservationIgnored private var generation = 0
    /// This model's own reload is going through `partDidShow`: it is not a new part.
    @ObservationIgnored private(set) var rescaling = false

    var hasPart: Bool { base != nil }
    var isScaled: Bool { scale != 1 }
    /// The field's text (`+(scale * 100).toFixed(2)`, `render`).
    var fieldText: String { scalePercentText(factor: scale) }
    /// `#scale-now`: the badge's suffix, empty at 100 %.
    var badgeSuffix: String { scale == 1 ? "" : " \(fieldText)%" }

    /// A new part (`resetScale`): it is at 100 %.
    func newPart(_ part: Part?) {
        generation += 1
        scale = 1
        base = part
    }

    /// The field's Enter or blur (`scale-pct` `change`): junk or empty shows the scale that holds, else rescale.
    /// Returns the text the field should show.
    @discardableResult
    func commit(text: String) -> String {
        if let s = scaleFromPercentText(text: text) { setScale(s) }
        return fieldText
    }

    /// Reset (`scale-reset`).
    func reset() { setScale(1) }

    /// `setScale`: nothing without a part or when the scale is already `s`.
    func setScale(_ s: Double) {
        guard let base, s != scale else { return }
        generation += 1
        let mine = generation
        Task { @MainActor in
            // the weld is the engine's work: off the main actor
            let part = s == 1 ? base : await Task.detached(priority: .userInitiated) { base.scaled(factor: s) }.value
            guard mine == generation else { return }
            rescaling = true
            apply?(part)
            rescaling = false
            scale = s
        }
    }
}
