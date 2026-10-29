import AppKit
import SwiftUI
import Viewer

extension ToolbarSlot {
    static let arrange = ToolbarSlot(id: "arrange", title: "Arrange", systemImage: "square.grid.2x2",
                                     help: "Pack this plate's models apart on the bed, biggest in the middle. Undo puts them back.", order: 5)
}

/// Several models on one plate (`ui/models.js`, `ui/plates.js`): File > Add Model to Plate…, Edit > Remove Model from
/// Plate, a click on a parked model to edit it, and the Undo toast of a removal. The state is `PlateBoard`.
@MainActor
final class PlatesFeature: Feature {
    unowned let host: ViewerModel
    let board: PlateBoard

    init(host: ViewerModel) {
        self.host = host
        board = PlateBoard(host: host)
        host.controller?.onTap = { [board] x, y, w, h in board.tapped(x: x, y: y, width: w, height: h) }
    }

    // MARK: the toolbar's Arrange

    var toolbarSlots: [ToolbarSlot] { [.arrange] }

    /// Arrange (`#arrange-btn`, `ui/arrange.js` `syncButton`): there only while a model is on show and the plate has
    /// more than one; a click packs the plate (`PlateBoard.arrange`), the button greyed while it waits for the builds.
    func registerTools(in toolbar: ToolbarRegistry) {
        guard host.controller != nil else { return }
        toolbar.registerAction(.arrange, isShown: { [board] in board.arrangeShown }, isEnabled: { [board] in !board.arranging && !board.busy }) {
            [board] in Task { await board.arrange() }
        }
    }

    // MARK: a part arrives

    func partWillShow() {
        if !board.swapping && host.addingModel { board.willAdd() }
    }

    func partDidShow(_ parts: [LoadedPart]) {
        if !board.swapping { board.shown(parts.first?.mesh as? CorePart) }
    }

    // MARK: window

    var overlays: [WindowOverlay] {
        [
            WindowOverlay("plate-labels", order: 25, ignoresSafeArea: true) { PlateLabels(board: self.board) },
            WindowOverlay("plate-tabs", order: 65, alignment: .top) { PlateTabs(board: self.board) },
            WindowOverlay("clash-banner", order: 66, alignment: .top) { ClashBanner(board: self.board) },
            WindowOverlay("plate-toast", order: 70, alignment: .bottom) { PlateToast(board: self.board) },
        ]
    }

    var inspectorSections: [InspectorSection] {
        [InspectorSection("plate-part-buttons", slot: .stats) {
            if self.host.hasPart { PlatePartButtons(feature: self) }
        }]
    }

    var modifiers: [WindowModifier] {
        [
            // the plate's settings: a change rebuilds the parked models' fins (`scheduleParked`)
            .onChange("plates-settings", of: { SettingsKey(session: self.host.session) }) { _, _ in self.board.scheduleParked() },
            // the models' footprints change with a turn, a fin rebuild, a removal, or the seating the analysis lands:
            // the clash cells and the banner follow (`checkOverlap`)
            .onChange("plates-clash", of: { FootprintKey(host: self.host) }) { _, _ in self.board.sync() },
            // the beds follow the build volume
            .onChange("plates-volume", of: { self.host.controller?.volume }) { _, _ in self.board.sync() },
        ]
    }

    /// What the live model's footprint is made from.
    struct FootprintKey: Equatable {
        var pose: Pose
        var finsOn: Bool
        var build: ObjectIdentifier?
        var removed: Set<FinKey>
        var size: SIMD3<Float>

        @MainActor init(host: ViewerModel) {
            pose = host.session.pose
            finsOn = host.session.finsOn
            build = host.wiring?.build?.latest.map(ObjectIdentifier.init)
            removed = host.session.removedFins
            size = host.controller?.posedSize ?? .zero
        }
    }

    /// What a support build follows: the options, the tunables, the mode and fins on or off (`settingsKey`).
    struct SettingsKey: Equatable {
        var options: String
        var tunables: String
        var mode: FinSettings.Mode
        var finsOn: Bool

        @MainActor init(session: PartSession) {
            options = session.settings.optionsJSON()
            tunables = session.settings.tunablesJSON()
            mode = session.settings.mode
            finsOn = session.finsOn
        }
    }

    // MARK: commands

    /// Add model (`add-part`, `file-add`): the files chosen go onto this plate, beside its models, in turn (a 3MF's
    /// object picker waits for its answer).
    func addModels() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = PartDocument.readableContentTypes
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK else { return }
        addModels(panel.urls)
    }

    /// Several files at once go one to a plate, starting on this one if it is empty; in turn, a 3MF's object picker
    /// waiting for its answer (`loadFiles`, `ui/io.js`).
    func loadFiles(_ urls: [URL]) {
        Task {
            for url in urls where OpenRules.accepts(url) {
                board.freePlate()
                await host.load(url: url)
                while host.picker != nil { try? await Task.sleep(for: .milliseconds(100)) }
            }
            board.dropEmptyPlate()
        }
    }

    func addModels(_ urls: [URL]) {
        Task {
            for url in urls where OpenRules.accepts(url) {
                await host.load(url: url, adding: true)
                while host.picker != nil { try? await Task.sleep(for: .milliseconds(100)) }
            }
        }
    }

    static let addHelp = JS.t("Put another model on this plate, beside this one. Click a model to edit it.")
    static let removeHelp = JS.t("Take this model off the plate, to import another without reloading the page.")

    static func menus(_ f: PlatesFeature?) -> [MenuSection] {
        [
            MenuSection(menu: .fileExport, order: 10, entries: [
                .item(JS.t("Add Model to Plate…"), help: addHelp, enabled: f != nil) { f?.addModels() },
            ]),
            MenuSection(menu: .edit, order: 10, entries: [
                .item(JS.t("Remove Model from Plate"), help: removeHelp, enabled: (f?.board.hasLive ?? false) && !(f?.board.busy ?? true)) {
                    f?.board.removeModel()
                },
            ]),
        ]
    }
}

/// The stats card's Add model and Remove model (`part-btns`, `index.html:219-224`): the card's last row, after
/// Analysis, so the status card follows them as on the web (where it sits in the other rail).
struct PlatePartButtons: View {
    let feature: PlatesFeature

    var body: some View {
        HStack {
            Button("Add model") { feature.addModels() }
                .help(PlatesFeature.addHelp)
                .accessibilityIdentifier("add-part")
            Button("Remove model") { feature.board.removeModel() }
                .help(PlatesFeature.removeHelp)
                .disabled(!feature.board.hasLive || feature.board.busy)
                .accessibilityIdentifier("remove-part")
        }
    }
}

/// The Undo toast of a removed model (`#toast`, `ui/plates.js`): bottom centre, over the view, for a few seconds.
struct PlateToast: View {
    let board: PlateBoard

    var body: some View {
        if let text = board.toast {
            HStack(spacing: 12) {
                Text(text).accessibilityIdentifier("toast-text")
                Button("Undo") { board.undoToast() }
                    .accessibilityIdentifier("toast-undo")
            }
            .padding(.vertical, 8)
            .padding(.leading, 14)
            .padding(.trailing, 10)
            .background(.regularMaterial, in: .rect(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.separator))
            .shadow(radius: 12)
            .padding(.bottom, 70)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("toast")
        }
    }
}
