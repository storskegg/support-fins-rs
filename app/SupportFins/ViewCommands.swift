import SwiftUI
import Viewer

struct ViewerKey: FocusedValueKey { typealias Value = ViewerController }
extension FocusedValues {
    var viewer: ViewerController? {
        get { self[ViewerKey.self] }
        set { self[ViewerKey.self] = newValue }
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
