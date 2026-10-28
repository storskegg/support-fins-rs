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

/// Model menu (docs/UI-MAP.md §8.1): the pose commands of the Rotate popover.
struct ModelCommands: Commands {
    @FocusedValue(\.session) private var session

    var body: some Commands {
        CommandMenu("Model") {
            ForEach(Pose.Axis.allCases, id: \.self) { axis in
                Button("Rotate 90° About \(axis.label)") { session?.rotate90(about: axis) }
            }
            Button("Reset Rotation") { session?.resetPose() }
        }
    }
}

/// View menu: the camera views of docs/UI-MAP.md (the web app's View popover) with ⌘0..⌘6.
struct ViewCommands: Commands {
    @FocusedValue(\.viewer) private var viewer

    var body: some Commands {
        CommandMenu("View") {
            Button("Frame Part") { viewer?.frame() }.keyboardShortcut("0")
            Divider()
            ForEach(Array(CameraView.allCases.enumerated()), id: \.offset) { i, v in
                Button(v.label) { viewer?.apply(v) }.keyboardShortcut(KeyEquivalent(Character("\(i + 1)")))
            }
        }
    }
}
