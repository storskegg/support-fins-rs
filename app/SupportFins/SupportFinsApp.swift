import SwiftUI

@main
struct SupportFinsApp: App {
    var body: some Scene {
        DocumentGroup(viewing: PartDocument.self) { file in
            ContentView(document: file.document)
        }
        .defaultSize(width: 1100, height: 760)
        .commands {
            FeatureCommands()
        }
    }
}
