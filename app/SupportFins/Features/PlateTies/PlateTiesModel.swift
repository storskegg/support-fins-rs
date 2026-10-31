import Foundation
import Observation
import SupportFinsCore
import Viewer

/// Plate ties on the plate being edited (issue #50, slice 2): the models that hold each other up, the links between
/// them, the ties, and the lock on the plate while they are on.
///
/// - The state (on, settings, links clicked off or added) is the plate's (`Plate.ties`); `state` is the edited plate's,
///   observed by the inspector, and written back to the plate on every change.
/// - Detection and the ties are synchronous FFI: they run off the main actor, on the plate as an export writes it
///   (`PlateBoard.plateWrite`). Each run carries a generation; a result that is not the newest is dropped.
/// - The plate is re-tied when what the engine builds on changes (`Fingerprint`): a model added, removed or turned,
///   its fins rebuilt, the settings or the links changed. `PlateBoard.sync` asks after every change.
/// - While the ties are on, Arrange and Move are locked (`lockReason`): the ties are built for where the models stand.
@MainActor
@Observable
final class PlateTiesModel {
    @ObservationIgnored weak let host: ViewerModel?
    @ObservationIgnored let session: PartSession
    @ObservationIgnored let profiles: PrinterProfileStore

    /// The edited plate's ties (a copy of `Plate.ties`, written back by `commit`).
    private(set) var state = PlateTieState()
    /// The pairs tied, every pair found (the ones clicked off too), and what the last build made.
    private(set) var links: [TieLink] = []
    private(set) var candidates: [TieLink] = []
    private(set) var pairs: [TiePairResult] = []
    private(set) var triangles = 0
    /// The engine refused the plate or the settings (its words), or nil.
    private(set) var error: String?
    /// A settings edit the engine refused (it names the key): not applied.
    private(set) var settingsError: String?
    /// A detect and build is out.
    private(set) var working = false
    /// Add a pair: armed, with the models picked so far (their keys).
    private(set) var pairing: [String]?

    /// The newest run; an older one's result is dropped.
    @ObservationIgnored private(set) var generation = 0
    @ObservationIgnored private var fingerprint: Fingerprint?
    @ObservationIgnored private var task: Task<Void, Never>?
    /// Polls while a fin build holds the run back: a build landing does not always lay the plate out again.
    @ObservationIgnored private var retryTask: Task<Void, Never>?
    /// The run is held back by a build that is out.
    @ObservationIgnored private(set) var waiting = false
    @ObservationIgnored private weak var plateShown: Plate?
    @ObservationIgnored private var wired = false
    /// The links as drawn (the scene's order): what a click on one means.
    private(set) var drawn: [TieLink] = []
    /// How long the run waits for edits to settle.
    @ObservationIgnored var settle: Duration = .milliseconds(80)

    init(host: ViewerModel, profiles: PrinterProfileStore = .shared) {
        self.host = host
        session = host.session
        self.profiles = profiles
    }

    var board: PlateBoard? { host?.features[PlatesFeature.self]?.board }

    // MARK: the board's hooks

    /// Hook into the plate (once, when the window's features are all made: the board is another feature's).
    func wire() {
        guard !wired, let board else { return }
        wired = true
        board.lockReason = { [weak self] in self?.lockReason }
        board.didSync = { [weak self] in self?.synced() }
        board.tieTap = { [weak self] x, y, w, h in self?.tapped(x: x, y: y, width: w, height: h) ?? false }
        board.tieRequest = { [weak self] i in self?.request(plate: i) }
        synced()
    }

    /// Why Arrange and Move are locked, or nil.
    var lockReason: String? {
        state.on ? JS.t("Plate ties are on: Arrange and Move are locked so the ties stay where they were built. Turn Plate ties off to move models.") : nil
    }

    /// The plate was laid out again: another plate edited, or something on it changed.
    func synced() {
        hookExporter()
        guard let board else { return }
        if board.plate !== plateShown {
            plateShown = board.plate
            state = board.plate.ties
            pairing = nil
            fingerprint = nil
            clearResults()
        }
        schedule()
    }

    // MARK: on, off, settings

    func setOn(_ on: Bool) {
        guard on != state.on else { return }
        var s = state
        s.on = on
        if on {
            // the settings as they stand, or the profile's: a refusal leaves the ties off, with the reason
            do { s.settings = try TieSettings.validated(s.settings, profile: profiles.id) } catch {
                settingsError = Self.describe(error)
                return
            }
            // Move stops: the models stay where the ties are built
            host?.features[MoveFeature.self]?.model.setOn(false)
        } else {
            pairing = nil
        }
        commit(s)
    }

    /// The settings shown: the plate's, or the profile's defaults.
    var settings: TieSettings? {
        state.settings ?? (try? TieSettings.validated(nil, profile: profiles.id))
    }

    /// An edit of the settings, checked by the engine: refused (a contact thinner than the printer allows, a height range
    /// that does not rise), it is not applied and `settingsError` says why.
    func update(_ new: TieSettings) {
        do {
            let checked = try TieSettings.validated(new, profile: profiles.id)
            settingsError = nil
            var s = state
            s.settings = checked
            commit(s)
        } catch {
            settingsError = Self.describe(error)
        }
    }

    /// Reset to profile defaults: the settings the printer profile gives (`plate_ties_settings(profile, "")`).
    func resetToProfile() {
        settingsError = nil
        var s = state
        s.settings = try? TieSettings.validated(nil, profile: profiles.id)
        commit(s)
    }

    /// The printer changed: the plate's settings are checked against it; ones it cannot print go back to its defaults.
    func profileChanged() {
        guard let current = state.settings else { schedule(); return }
        var s = state
        s.settings = (try? TieSettings.validated(current, profile: profiles.id)) ?? (try? TieSettings.validated(nil, profile: profiles.id))
        commit(s)
    }

    private func commit(_ s: PlateTieState) {
        state = s
        board?.plate.ties = s
        if !s.on { clearResults() }
        schedule()
    }

    // MARK: links

    /// A link clicked: a tied pair goes off (one added by hand is taken away), a pair clicked off comes back.
    func toggle(_ link: TieLink) {
        var s = state
        let (a, b) = (link.a, link.b)
        if links.contains(where: { TiePairKey(a: $0.a, b: $0.b).same(a, b) }) {
            if let k = s.added.firstIndex(where: { $0.same(a, b) }) { s.added.remove(at: k) } else { s.disabled.append(TiePairKey(a: a, b: b)) }
        } else {
            s.disabled.removeAll { $0.same(a, b) }
        }
        commit(s)
    }

    /// Two models picked: tie them (a pair clicked off comes back; one already tied stays as it is).
    func addPair(_ a: String, _ b: String) {
        guard a != b else { return }
        var s = state
        if s.disabled.contains(where: { $0.same(a, b) }) {
            s.disabled.removeAll { $0.same(a, b) }
        } else if !links.contains(where: { TiePairKey(a: $0.a, b: $0.b).same(a, b) }) {
            s.added.append(TiePairKey(a: a, b: b))
        }
        commit(s)
    }

    /// Every link back as the detection finds it.
    func restoreLinks() {
        var s = state
        s.disabled = []
        s.added = []
        commit(s)
    }

    /// The pair of `link` is tied now (not clicked off).
    func isTied(_ link: TieLink) -> Bool { links.contains { TiePairKey(a: $0.a, b: $0.b).same(link.a, link.b) } }

    var hasOverrides: Bool { !state.disabled.isEmpty || !state.added.isEmpty }

    /// Add a pair: the next two models clicked are tied (Esc stops).
    func setPairing(_ on: Bool) {
        pairing = on && state.on ? [] : nil
    }

    /// A model clicked while Add a pair is armed.
    func picked(_ key: String) {
        guard var p = pairing, !p.contains(key) else { return }
        p.append(key)
        if p.count == 2 {
            pairing = nil
            addPair(p[0], p[1])
        } else {
            pairing = p
        }
    }

    /// A click on the view (`PlateBoard.tapped`): a link toggles; with Add a pair armed, a model is picked. True when
    /// the click was taken.
    func tapped(x: Int, y: Int, width: Int, height: Int) -> Bool {
        guard state.on, let renderer = host?.controller?.renderer else { return false }
        if pairing != nil {
            Task { [weak self] in
                let pick = await renderer.pickModel(x: x, y: y, width: width, height: height)
                guard let self, let board = self.board, self.pairing != nil else { return }
                switch pick {
                case .live: if let k = board.tieKey(of: board.plate.liveSlot) { self.picked(k) }
                case .parked(let i):
                    guard board.parkedSlots.indices.contains(i), board.parkedPlates.indices.contains(i), board.parkedPlates[i] == board.active else { return }
                    if let k = board.tieKey(of: board.parkedSlots[i]) { self.picked(k) }
                case nil: break
                }
            }
            return true
        }
        guard let k = renderer.linkAt(x: x, y: y, width: width, height: height), drawn.indices.contains(k) else { return false }
        toggle(drawn[k])
        return true
    }

    // MARK: the run

    /// What the engine builds the ties from: when none of it changed, neither do the ties.
    struct Fingerprint: Equatable {
        struct Model: Equatable {
            var key: String
            var part: ObjectIdentifier
            var build: ObjectIdentifier?
            var drawn: ObjectIdentifier?
            /// Walls still to stand: their ids, as the sweep is made anew each time.
            var standing: [UUID]?
            var pose: [Double]
            var threshold: Double
            var removed: [UInt64]
            var x: Double
            var y: Double
        }
        var plate: ObjectIdentifier
        var profile: String
        var settings: String
        var overrides: TieOverrides
        var models: [Model]
    }

    /// The edited plate's ties as asked now: the request, the plate's write and its fingerprint; nil with ties off,
    /// nothing to tie, or a build still out (it lands, the board syncs, and this runs again).
    private enum Pending {
        /// Nothing to tie: off, no model, or settings the engine refuses (`error` says so).
        case none
        /// A fin build is out, or a ring is being turned: wait for it.
        case wait
        case ready(TieRequest, PlateBoard.PlateWrite, Fingerprint)
    }

    private func pending() -> Pending {
        guard let board, state.on, board.hasLive else { return .none }
        if session.ringDragging || host?.wiring?.build?.building == true { return .wait }
        let parked = board.plate.models.enumerated().compactMap { k, m in k == board.plate.live ? nil : m.stash }
        if parked.contains(where: { $0.build.building || board.isStale($0) }) { return .wait }
        guard let w = board.plateWrite(board.active, deferStanding: true), let req = request(plate: board.active) else { return .none }
        let slots = Dictionary(zip(board.tieKeys(board.active), board.plate.models).compactMap { k, m in k.map { ($0, m) } }) { a, _ in a }
        let models = w.models.enumerated().map { k, m in
            let slot = slots[w.keys[k]]
            return Fingerprint.Model(key: w.keys[k], part: ObjectIdentifier(m.part), build: m.build.map(ObjectIdentifier.init),
                                     drawn: m.drawn.map(ObjectIdentifier.init),
                                     standing: w.stands[k] == nil ? nil : slot.map { board.walls($0).map(\.id) },
                                     pose: m.pose, threshold: m.threshold, removed: m.removed, x: m.x, y: m.y)
        }
        return .ready(req, w, Fingerprint(plate: ObjectIdentifier(board.plate), profile: req.profile, settings: req.settings, overrides: req.overrides, models: models))
    }

    /// Plate `i`'s ties for the engine, or nil when they are off (or its settings no longer pass: then `error` says so).
    func request(plate i: Int) -> TieRequest? {
        guard let board, board.plates.indices.contains(i) else { return nil }
        let s = i == board.active ? state : board.plates[i].ties
        guard s.on else { return nil }
        let keys = Set(board.tieKeys(i).compactMap { $0 })
        do {
            let settings = try plateTiesSettings(profileId: profiles.id, settingsJson: s.settings?.json ?? "")
            return TieRequest(profile: profiles.id, settings: settings, overrides: s.overrides(on: keys))
        } catch {
            if i == board.active { self.error = Self.describe(error) }
            return nil
        }
    }

    /// Re-tie the plate if what the ties are built from changed: after a short settle, off the main actor.
    func schedule() {
        guard state.on else { return }
        let req: TieRequest, write: PlateBoard.PlateWrite, print: Fingerprint
        switch pending() {
        case .none:
            waiting = false
            // settings the engine refuses: nothing is tied until they pass (`error` says why)
            if error != nil, state.on {
                links = []; candidates = []; pairs = []; triangles = 0
                show(nil)
            }
            return
        case .wait:
            // a build is out (or a ring is being turned): the ties shown are for the plate as it was, and the run waits
            if !links.isEmpty || triangles > 0 { setStale(true) }
            waitForBuilds()
            return
        case .ready(let r, let w, let p):
            (req, write, print) = (r, w, p)
        }
        waiting = false
        retryTask?.cancel()
        retryTask = nil
        guard print != fingerprint else { return }
        fingerprint = print
        generation += 1
        let gen = generation, settle = settle
        working = true
        setStale(true)
        task?.cancel()
        task = Task { [weak self] in
            try? await Task.sleep(for: settle)
            guard !Task.isCancelled else { return }
            let outcome = await Task.detached(priority: .userInitiated) { () -> Result<TieOutcome, any Error> in
                Result { try req.run(write) }
            }.value
            guard let self, self.host != nil, gen == self.generation else { return }
            self.apply(outcome)
        }
    }

    /// Look again shortly while a build holds the run back.
    private func waitForBuilds() {
        guard state.on, board?.hasLive == true else { waiting = false; return }
        waiting = true
        guard retryTask == nil else { return }
        retryTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(100))
            guard !Task.isCancelled, let self else { return }
            self.retryTask = nil
            self.schedule()
        }
    }

    /// Wait for the newest run to land (tests, and the measurements); false after `seconds`.
    @discardableResult
    func settled(seconds: Double = 60) async -> Bool {
        let deadline = ContinuousClock.now + .seconds(seconds)
        while working || waiting {
            if ContinuousClock.now > deadline { return false }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return true
    }

    /// The engine's message, or the system's.
    static func describe(_ e: any Error) -> String {
        (e as? FfiError).map(BuildModel.message) ?? e.localizedDescription
    }

    private func apply(_ outcome: Result<TieOutcome, any Error>) {
        working = false
        task = nil
        switch outcome {
        case .success(let o):
            error = nil
            links = o.links
            candidates = o.candidates
            pairs = o.built.pairs()
            triangles = Int(o.built.triangleCount())
            show(o.built)
        case .failure(let e):
            error = Self.describe(e)
            links = []
            candidates = []
            pairs = []
            triangles = 0
            show(nil)
        }
    }

    /// The ties and the links on the plate: the tied pairs in blue (orange when added by hand), the ones clicked off grey.
    private func show(_ built: TieBuildResult?) {
        guard let renderer = host?.controller?.renderer else { return }
        var mesh: OverlayMesh?
        if let built {
            let tris = built.mesh()
            if tris.count() > 0, let b = tris.metalBuffer(device: renderer.device) { mesh = OverlayMesh(buffer: b, vertices: Int(tris.count())) }
        }
        let tied = Set(links.map { [$0.a, $0.b].sorted() })
        let all = candidates + links.filter { l in !candidates.contains { [$0.a, $0.b].sorted() == [l.a, l.b].sorted() } }
        drawn = all
        func v(_ p: [Double]) -> SIMD3<Float> { p.count == 3 ? SIMD3(Float(p[0]), Float(p[1]), Float(p[2])) : .zero }
        let lines = all.map { l in
            TieLinkLine(from: v(l.from), to: v(l.to), enabled: tied.contains([l.a, l.b].sorted()), manual: l.origin == .manual)
        }
        renderer.setTieScene(TieScene(ties: mesh, links: lines))
    }

    private func setStale(_ stale: Bool) {
        guard let renderer = host?.controller?.renderer, var scene = renderer.tieScene, scene.stale != stale else { return }
        scene.stale = stale
        renderer.setTieScene(scene)
    }

    private func clearResults() {
        task?.cancel()
        task = nil
        retryTask?.cancel()
        retryTask = nil
        waiting = false
        generation += 1
        working = false
        links = []
        candidates = []
        pairs = []
        triangles = 0
        drawn = []
        error = nil
        host?.controller?.renderer.setTieScene(nil)
        if !state.on { fingerprint = nil }
    }

    // MARK: what the status card says

    /// A model's name, by key (`TieKey`).
    func name(_ key: String) -> String { TieKey.name(key) }

    /// The notes of the last build, in the app's language: a tie on the face, a shallow sink, a tie left out, no room.
    var notes: [String] {
        pairs.flatMap { p in
            let pair = "\(name(p.link.a)) – \(name(p.link.b))"
            return p.notes.map { TieNotes.text($0, pair: pair, model: $0.model.map(name)) }
        }
    }

    var placed: Int { pairs.reduce(0) { $0 + Int($1.placed) } }
    var requested: Int { pairs.reduce(0) { $0 + Int($1.requested) } }

    // MARK: a tied 3MF

    /// The plate's tie document while its ties are on (`plate_ties_document`: the printer, the settings, the links
    /// clicked off and added, by model name), or nil. A one-model 3MF carries it (decision D of #50), so a tied 3MF
    /// opened and written again keeps its ties; a plate of several writes `TieBuildResult.document_json` instead.
    var document: String? {
        guard state.on else { return nil }
        return try? plateTiesDocument(profileId: profiles.id, settingsJson: state.settings?.json ?? "", overrides: state.overrides)
    }

    /// The one-model export asks for the document: hooked again whenever the window makes a new exporter.
    func hookExporter() {
        host?.wiring?.exporter?.plateTies = { [weak self] in self?.document }
    }

    /// Apply a tie document (`TieBuildResult.document_json`): on, its settings checked against the printer chosen
    /// here, its links as saved. They name models (`TieKey`): on a plate of the models it was saved with they
    /// come back; ones naming no model here are kept, and written out again with the plate.
    func apply(document doc: String) {
        guard let obj = (try? JSONSerialization.jsonObject(with: Data(doc.utf8))) as? [String: Any] else { return }
        func pairs(_ k: String) -> [TiePairKey] {
            (obj[k] as? [[String]] ?? []).compactMap { $0.count == 2 ? TiePairKey(a: $0[0], b: $0[1]) : nil }
        }
        var s = PlateTieState(on: true)
        if let settings = obj["settings"], let data = try? JSONSerialization.data(withJSONObject: settings) {
            let saved = TieSettings.decode(String(decoding: data, as: UTF8.self))
            s.settings = (try? TieSettings.validated(saved, profile: profiles.id)) ?? (try? TieSettings.validated(nil, profile: profiles.id))
        }
        s.disabled = pairs("disabled")
        s.added = pairs("added")
        host?.features[MoveFeature.self]?.model.setOn(false)
        commit(s)
    }

    func close() {
        task?.cancel(); task = nil
        retryTask?.cancel(); retryTask = nil
    }
}
