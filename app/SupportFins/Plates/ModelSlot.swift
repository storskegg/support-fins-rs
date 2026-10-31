import Metal
import Observation
import SupportFinsCore
import Viewer
import simd

/// The fin settings of a plate (`plate.settings`, `ui/models.js`): every field, the mode and fins on or off. Models
/// on one plate print in one job, so the settings are the plate's, not a model's.
struct PlateSettings: Equatable {
    var settings: FinSettings
    var finsOn: Bool
}

/// A model's place on a plate (`slot`, `ui/models.js`). The live slot's `stash` is nil (its state is the window's),
/// and so is an empty plate's one slot.
@MainActor
@Observable
final class ModelSlot: Identifiable {
    let id = UUID()
    /// Where the model stands on its plate, mm from the bed's centre.
    var xy: SIMD2<Double>
    /// The model while it is parked.
    var stash: ParkedModel?

    init(xy: SIMD2<Double> = .zero, stash: ParkedModel? = nil) {
        self.xy = xy
        self.stash = stash
    }

    /// The footprint cells of this slot's part and what it adds, kept while neither changes (`fpPart` / `fpAdded` /
    /// `fpAll`, `ui/arrange.js`).
    @ObservationIgnored var footprintCache: (key: FootprintKey, grid: Footprint?)?
}

/// What a footprint was made from: when none of it changed the cells are the same.
struct FootprintKey: Equatable {
    var pose: [Double]
    var offset: SIMD3<Float>
    /// The build whose fins and pad are counted (nil: none, fins off), and the fin ids left out.
    var build: ObjectIdentifier?
    var removed: [UInt64]
    /// The hand-placed supports' sweep that is counted too (Features/Draw), nil: none.
    var drawn: ObjectIdentifier? = nil
}

/// A model that is not the one being edited (`stash` of a slot, `ui/models.js park`): its mesh, its pose and removed
/// fins, and the build that holds its fins, kept to the plate's settings by `PlateBoard` (`ui/parkedbuild.js`).
/// Nothing is re-read or re-welded when it comes back.
@MainActor
@Observable
final class ParkedModel {
    let part: CorePart
    /// The name the window showed for it.
    let openedName: String?
    var pose: Pose
    var removedFins: Set<FinKey>
    /// Its fins: the live build while it was edited, then rebuilt in the background when the plate's settings change.
    let build: BuildModel
    /// The seating offset of its pose: it draws at `pose * p + offset`.
    var offset: SIMD3<Float>
    /// The overhang colours as they were when it was parked.
    var flags: MTLBuffer?
    /// The plate's settings when it was parked: a model put back onto a plate that has none keeps them.
    let settings: PlateSettings
    /// What the features keep per model (`Feature.modelState`), by feature type.
    var featureStates: [ObjectIdentifier: Any] = [:]
    @ObservationIgnored private var layer: (result: ObjectIdentifier, layer: SupportLayer)?

    init(part: CorePart, openedName: String?, pose: Pose, removedFins: Set<FinKey>, build: BuildModel, offset: SIMD3<Float>,
         flags: MTLBuffer?, settings: PlateSettings) {
        self.part = part
        self.openedName = openedName
        self.pose = pose
        self.removedFins = removedFins
        self.build = build
        self.offset = offset
        self.flags = flags
        self.settings = settings
    }

    var name: String { part.info.name }

    /// The fins and pad of the newest build, wrapped for the viewer once per build.
    func supportLayer(device: MTLDevice) -> SupportLayer? {
        guard let result = build.latest else { return nil }
        let id = ObjectIdentifier(result)
        if layer?.result != id, let made = build.supportLayer(device: device) { layer = (id, made) }
        return layer?.layer
    }
}

/// One plate: its model slots and which is live -- or, on a plate not being edited, which comes back live when it
/// is -- and, kept when its live model parks, its settings and undo history (nil until then) (`newPlate`,
/// `ui/plates.js`).
@MainActor
final class Plate {
    var models: [ModelSlot]
    var live: Int
    var settings: PlateSettings?
    var history: History<SessionSnapshot>?
    /// The plate's ties (issue #50, `Features/PlateTies`): on or off, their settings and the links clicked off or added.
    var ties = PlateTieState()

    init(models: [ModelSlot] = [ModelSlot()], live: Int = 0) {
        self.models = models
        self.live = live
    }

    var liveSlot: ModelSlot { models[live] }
}
