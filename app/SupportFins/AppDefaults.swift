import Foundation

/// The app's preferences: what every store reads and writes (`SettingsStore`, `VolumeStore`, `PrinterProfileStore`,
/// `ViewSettings`, `RotateModel`, `SectionModel`, `DrawModel`). Normally `UserDefaults.standard`.
///
/// A test must never read or write the user's preferences. A test host would otherwise start every window from what the
/// user last picked in the installed app (both share the container, `com.storskegg.SupportFins`): a remembered 0.05 mm layer
/// height made a Plate ties undo test fail. So a DEBUG build that is a unit-test host (`XCTestConfigurationFilePath` set) or
/// was launched with `-SFIsolatedDefaults YES` (every UI test, `PartWindowUITestCase`) uses a suite of its own,
/// `com.storskegg.SupportFins.isolated`, emptied when the process first touches it: each run starts from the defaults and
/// leaves the user's untouched. Compiled out of Release builds (X-15): a Release app always uses `.standard`.
///
/// Launch arguments (`-SFCrashDir`, `-SFChangelogFixture`, `-AppleLanguages`) are the argument domain, which every
/// `UserDefaults` instance sees; the code that only reads those keeps reading `.standard`.
enum AppDefaults {
    static let isolatedSuite = "com.storskegg.SupportFins.isolated"

    nonisolated(unsafe) static let shared: UserDefaults = {
        #if DEBUG
        if isolated(environment: ProcessInfo.processInfo.environment, arguments: .standard),
           let suite = UserDefaults(suiteName: isolatedSuite) {
            suite.removePersistentDomain(forName: isolatedSuite)
            return suite
        }
        #endif
        return .standard
    }()

    /// Whether this process keeps its preferences apart from the user's: a unit-test host, or `-SFIsolatedDefaults YES`.
    /// Always false in a Release build.
    static func isolated(environment: [String: String], arguments: UserDefaults) -> Bool {
        #if DEBUG
        return environment["XCTestConfigurationFilePath"] != nil || arguments.bool(forKey: "SFIsolatedDefaults")
        #else
        return false
        #endif
    }
}
