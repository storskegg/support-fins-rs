import Foundation
import SupportFinsCore
import UniformTypeIdentifiers
import Viewer

/// Exporting a plate of several models, and every plate (`ui/export.js` `plateGeometry` and `FORMATS['export-3mf']`,
/// `ui/plates.js` `exportAll`). An STL and Fins only write the model being edited (`ExportModel`); a 3MF writes the
/// whole plate, each model its own objects where it stands; Export all plates writes each plate's 3MF in one .zip.
/// The parked models' fins are kept to the plate's settings (`PlateBoard+Parked`), so nothing is made live to write
/// them: a model whose fins are not current is built where it is parked.
/// Where a model's hand-placed walls come from for an export: none, a sweep already stood, or a recipe the export stands
/// off the main thread (a `DrawKit` analysis and a full sweep, up to a second on a big part).
enum DrawnSource: Sendable {
    case none
    case ready(DrawnSweep)
    case stand(@Sendable () -> DrawnSweep?)
}

extension PlateBoard {
    /// A plate's 3MF as `plateGeometry` makes it: the models in the plate's order, and the file's name.
    struct PlateWrite {
        var models: [PlateModel]
        /// `plateName(gs, 'fins', '.3mf')`: the first model's name, `and-N-more` for several, the nozzle and layer height.
        var fileName: String
        /// Walls still to stand, by index into `models` (`plateWrite(_, deferStanding: true)`): `resolved()` stands them.
        var stands: [Int: @Sendable () -> DrawnSweep?] = [:]
        /// Each model's slot id, as text: its key for plate ties (`TieModelInput.key`).
        var keys: [String] = []

        /// This write with its walls stood: heavy, so called off the main thread.
        func resolved() -> PlateWrite {
            var w = self
            for (k, stand) in stands { w.models[k].drawn = stand() }
            w.stands = [:]
            return w
        }
    }

    /// The plate's settings: the page's for the plate being edited, as parked for another.
    func plateSettings(_ i: Int) -> PlateSettings {
        let current = PlateSettings(settings: session.settings, finsOn: session.finsOn)
        guard i != active else { return current }
        return plates[i].settings ?? plates[i].liveSlot.stash?.settings ?? current
    }

    /// Plate `i`'s models ready to write; nil when it holds none (`plateGeometry`).
    func plateWrite(_ i: Int, deferStanding: Bool = false) -> PlateWrite? {
        guard plates.indices.contains(i) else { return nil }
        let p = plates[i], settings = plateSettings(i)
        var models: [PlateModel] = []
        var keys: [String] = []
        var sessions: [String] = []
        var first: String?
        var stands: [Int: @Sendable () -> DrawnSweep?] = [:]
        for (k, m) in p.models.enumerated() {
            let part: CorePart, build: BuildModel?, pose: Pose, removed: Set<FinKey>
            if i == active && k == p.live {
                guard let live = session.parts.first else { continue }
                (part, build, pose, removed) = (live, host?.wiring?.build, session.pose, session.removedFins)
            } else if let s = m.stash {
                (part, build, pose, removed) = (s.part, s.build, s.pose, s.removedFins)
            } else {
                continue
            }
            let result = settings.finsOn ? build?.latest : nil
            let ids = result == nil ? [] : (build?.matchedRemovals(of: removed).keys.sorted().map { UInt64($0) } ?? [])
            // a plate of one model carries its session (`session: gs.length === 1 ? gs[0].session() : null`)
            sessions.append(SessionFile.json(settings: settings.settings, finsOn: settings.finsOn, walls: walls(m), removed: removed))
            var walls: DrawnSweep?
            if result != nil {
                switch drawn(m) {
                case .none: break
                case .ready(let sweep): walls = sweep
                case .stand(let stand): if deferStanding { stands[models.count] = stand } else { walls = stand() }
                }
            }
            models.append(PlateModel(part: part.part, build: result, drawn: walls, pose: pose.columnMajor,
                                     threshold: settings.settings.threshold, removed: ids, x: m.xy.x, y: m.xy.y, session: nil))
            keys.append(m.id.uuidString)
            if first == nil { first = part.info.name }
        }
        guard let first else { return nil }
        if models.count == 1 { models[0].session = sessions[0] }
        let base = ExportModel.base(of: first)
        let stem = models.count == 1 ? base : "\(base)-and-\(models.count - 1)-more"
        // a suggested name only: the plain one when the engine refuses
        let name = (try? exportFileName(base: stem, kind: .threeMf, nozzle: settings.settings.nozzle, layerHeight: settings.settings.layerHeight))
            ?? "\(stem)-fins.3mf"
        return PlateWrite(models: models, fileName: name, stands: stands, keys: keys)
    }

    /// Wait until plate `i`'s fins are the ones its settings make: the edited plate's from the live build and the
    /// parked builds (`parkedSettled`), another plate's built where its models are parked (`plateReady`).
    func settlePlate(_ i: Int) async -> Bool {
        if i == active {
            let build = host?.wiring?.build      // not the window: it may close meanwhile
            await build?.settled()
            return await parkedSettled()
        }
        let ps = plateSettings(i)
        guard ps.finsOn else { return true }
        for m in plates[i].models {
            guard let s = m.stash else { continue }
            let want = BuildRequest(rotation: s.pose.columnMajor, settings: ps.settings)
            if s.build.requested != want {
                s.build.build(rotation: want.rotation, optionsJSON: want.optionsJSON, tunablesJSON: want.tunablesJSON)
            }
            await s.build.settled()
        }
        return true
    }

    /// The build volume's x and y: the 3MF is written where the models stand on a centred bed.
    private var bedOptions: String {
        let v = host?.controller?.volume ?? .standard
        return "{\"bed\":[\(v.x),\(v.y)]}"
    }

    /// Nothing to write until the model on show has its build (`export-3mf` is disabled while a rebuild is out).
    var canExportPlate: Bool { hasLive && !busy && host?.wiring?.build?.building != true }

    /// Some parked model of the plate has fins (`parkedFins`): the "No supports on this plate" note is about all of it.
    var parkedFinsExist: Bool {
        session.finsOn && plate.models.enumerated().contains { k, m in k != plate.live && m.stash?.build.latest != nil }
    }

    // MARK: the plate being edited

    /// File > Export > 3MF with several models: asks where, then writes the plate (`export-3mf`).
    @discardableResult
    func exportPlate() async -> URL? {
        guard canExportPlate, !busy else { return nil }
        CrashReporter.note("plate: export")
        busy = true
        defer { busy = false }
        // the other models' fins, kept to the plate's settings: wait for any still building, then write them as they stand
        _ = await settlePlate(active)
        guard let w = plateWrite(active, deferStanding: true), let url = savePanel.url(suggesting: w.fileName, type: ExportFormat.threeMF.contentType) else { return nil }
        return await writePlate(w, to: url)
    }

    /// Writes the plate on a background task (`exportJob`: progress and Cancel); the file appears only when whole.
    @discardableResult
    func writePlate(_ w: PlateWrite, to url: URL) async -> URL? {
        let bed = bedOptions
        // a tied plate is one merged object (issue #50): its ties built on the very models written
        let ties = tieRequest(active)
        return await write(to: url, title: "3MF") { path in
            let r = w.resolved()
            if let ties {
                try exportPlateTied(path: path, models: r.models, ties: ties.build(models: r.models, keys: r.keys), optionsJson: bed)
            } else {
                try SupportFinsCore.exportPlate(path: path, models: r.models, optionsJson: bed)
            }
        } done: { host?.wiring?.exporter?.lastFormat = .threeMF }
    }

    /// Runs `work` on the export job and turns its outcome into the URL, or nil with `exportError` set (nil too when
    /// cancelled). The engine's errors are shown by message.
    private func write(to url: URL, title: String, work: @escaping @Sendable (String) throws -> Void, done: () -> Void = {}) async -> URL? {
        exportError = nil
        do {
            guard try await exportJob.run(title: title, to: url, work: work) else { return nil }
            done()
            return url
        } catch let e as FfiError {
            exportError = BuildModel.message(e)
        } catch {
            exportError = error.localizedDescription
        }
        return nil
    }

    // MARK: every plate

    /// Export > All plates (3MF): every plate's models as a 3MF, all in one .zip (one plate: the 3MF alone)
    /// (`exportAll`, `ui/plates.js`).
    @discardableResult
    func exportAll() async -> URL? {
        guard !busy else { return nil }
        busy = true
        defer { busy = false }
        for i in plates.indices { _ = await settlePlate(i) }
        let files = plates.indices.compactMap { i in plateWrite(i, deferStanding: true).map { (i, $0) } }
        guard let (first, one) = files.first else { return nil }
        if files.count == 1 {
            guard let url = savePanel.url(suggesting: "plate-\(first + 1)-\(one.fileName)", type: ExportFormat.threeMF.contentType) else { return nil }
            return await writeAll(files, to: url)
        }
        guard let url = savePanel.url(suggesting: "support-fins-plates.zip", type: .zip) else { return nil }
        return await writeAll(files, to: url)
    }

    @discardableResult
    func writeAll(_ files: [(Int, PlateWrite)], to url: URL) async -> URL? {
        let bed = bedOptions
        let title = files.count == 1 ? "3MF" : "ZIP"
        // each tied plate is one merged object (issue #50); the others are written as ever
        let ties = files.map { tieRequest($0.0) }
        return await write(to: url, title: title) { path in
            if ties.allSatisfy({ $0 == nil }) {
                if files.count == 1 {
                    try SupportFinsCore.exportPlate(path: path, models: files[0].1.resolved().models, optionsJson: bed)
                } else {
                    try SupportFinsCore.exportPlates(path: path, plates: files.map { PlateFile(name: "plate-\($0.0 + 1)-\($0.1.fileName)", models: $0.1.resolved().models) },
                                                     optionsJson: bed)
                }
                return
            }
            let plates = try zip(files, ties).map { f, t in
                let r = f.1.resolved()
                return PlateExport(name: "plate-\(f.0 + 1)-\(f.1.fileName)", models: r.models, ties: try t?.build(models: r.models, keys: r.keys))
            }
            if plates.count == 1 {
                try exportPlateTied(path: path, models: plates[0].models, ties: plates[0].ties, optionsJson: bed)
            } else {
                try exportPlatesTied(path: path, plates: plates, optionsJson: bed)
            }
        }
    }
}
