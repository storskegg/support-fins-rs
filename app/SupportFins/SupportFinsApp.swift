import SwiftUI

@main
struct SupportFinsApp: App {
    init() {
        // crash reports: the marker, the engine's panic hook, MetricKit (a unit-test host skips it)
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil { CrashReporter.shared.start() }
    }

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
