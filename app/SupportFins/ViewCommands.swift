import SwiftUI
import Viewer

struct ViewerKey: FocusedValueKey { typealias Value = ViewerController }
extension FocusedValues {
    var viewer: ViewerController? {
        get { self[ViewerKey.self] }
        set { self[ViewerKey.self] = newValue }
    }
}

struct SessionKey: FocusedValueKey { typealias Value = PartSession }
extension FocusedValues {
    var session: PartSession? {
        get { self[SessionKey.self] }
        set { self[SessionKey.self] = newValue }
    }
}

struct ToolbarKey: FocusedValueKey { typealias Value = ToolbarRegistry }
extension FocusedValues {
    var toolbar: ToolbarRegistry? {
        get { self[ToolbarKey.self] }
        set { self[ToolbarKey.self] = newValue }
    }
}

struct LayFaceKey: FocusedValueKey { typealias Value = LayFaceModel }
extension FocusedValues {
    var layFace: LayFaceModel? {
        get { self[LayFaceKey.self] }
        set { self[LayFaceKey.self] = newValue }
    }
}

struct RotateKey: FocusedValueKey { typealias Value = RotateModel }
extension FocusedValues {
    var rotate: RotateModel? {
        get { self[RotateKey.self] }
        set { self[RotateKey.self] = newValue }
    }
}

/// Model menu (docs/UI-MAP.md §8.1): the pose commands of the Rotate popover.
struct ModelCommands: Commands {
    @FocusedValue(\.session) private var session
    @FocusedValue(\.layFace) private var layFace
    @FocusedValue(\.toolbar) private var toolbar

    var body: some Commands {
        CommandMenu("Model") {
            ForEach(Pose.Axis.allCases, id: \.self) { axis in
                Button("Rotate 90° About \(axis.label)") { session?.rotate90(about: axis) }
            }
            Button("Reset Rotation") { session?.resetPose() }
            Divider()
            // `lay-face` (ui/pose.js:221): arms the lay-a-face pointer mode; a second choice disarms it
            Toggle("Lay a Face Flat", isOn: Binding(get: { layFace?.armed ?? false }, set: { _ in layFace?.toggle() }))
                .disabled(layFace?.canArm != true)
            Divider()
            // each opens its popover in the view toolbar (`ToolbarRegistry.open`); only a single part has them
            Button("Suggest Orientation") { toolbar?.open = .suggest }
                .disabled(toolbar?.badges[.suggest] == nil)
            Button("Strength Arrow…") { toolbar?.open = .strength }
                .disabled(toolbar?.badges[.strength] == nil)
        }
    }
}

/// View menu: the camera views of docs/UI-MAP.md (the web app's View popover) with ⌘0..⌘6.
struct ViewCommands: Commands {
    @FocusedValue(\.viewer) private var viewer
    @FocusedValue(\.rotate) private var rotate

    var body: some Commands {
        CommandMenu("View") {
            // `show-rings` (ui/pose.js:161): the rotate rings, remembered
            Toggle("Show Rotate Rings", isOn: Binding(get: { rotate?.showRings ?? true }, set: { rotate?.showRings = $0 }))
                .help("The rotate rings around the part. Off hides them; Rotate 90°, Reset and Lay a face flat still turn the part.")
            Button("Frame Part") { viewer?.frame() }.keyboardShortcut("0")
            // the view cube's house (`ui/viewcube.js`): the starting angle, keeping the target and the zoom. The web
            // has no Frame Part, so Home sits beside it rather than replacing it.
            Button("Home") { viewer?.goHome() }
                .help("Home view: front right, from above. Keeps the zoom.")
            // `show-layers` (ui/strength.js): the horizontal print layers around the part
            Toggle("Show Print Layers", isOn: Binding(get: { viewer?.showsLayers ?? false }, set: { viewer?.showsLayers = $0 }))
                .help("Show the horizontal print layers around the part.")
            #if DEBUG
            // docs/UI-MAP.md: the debug-only frame-time overlay (the web's `#fps`)
            Toggle("Show Frame Time", isOn: Binding(get: { viewer?.showsFrameTime ?? false }, set: { viewer?.showsFrameTime = $0 }))
            #endif
            Divider()
            ForEach(Array(CameraView.allCases.enumerated()), id: \.offset) { i, v in
                Button(v.label) { viewer?.apply(v) }.keyboardShortcut(KeyEquivalent(Character("\(i + 1)")))
            }
        }
    }
}
