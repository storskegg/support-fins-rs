import SwiftUI

@main
struct SupportFinsApp: App {
    init() {
        // crash reports: the marker, the engine's panic hook, MetricKit. A unit-test host skips it; the UI tests' app may carry the same
        // environment, so a test that names its own crash folder (DEBUG-only `-SFCrashDir`) starts it anyway.
        // A unit-test host still gets the native crash handlers, writing into a per-run folder of its own (1E).
        if CrashReporter.shouldStart(environment: ProcessInfo.processInfo.environment) {
            CrashReporter.shared.start()
        } else {
            CrashReporter.startTestCapture()
        }
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
