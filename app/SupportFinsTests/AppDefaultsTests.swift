import Foundation
import Testing
@testable import Support_Fins

/// The test host never reads or writes the user's preferences (`AppDefaults`): the installed app and the test host share
/// the container, and a layer height the user picked (0.05 mm) once made PlateTiesTests' undo check fail.
@Suite("AppDefaults")
struct AppDefaultsTests {
    #if DEBUG
    @Test("the unit-test host keeps its preferences in a suite of its own")
    func theTestHostIsIsolated() {
        #expect(AppDefaults.shared !== UserDefaults.standard)
        #expect(AppDefaults.isolated(environment: ProcessInfo.processInfo.environment, arguments: .standard))
    }

    @Test("a test host, or -SFIsolatedDefaults YES (the UI tests' launch), isolates; nothing else does")
    func whoIsIsolated() throws {
        let name = "AppDefaultsTests-\(UUID().uuidString)"
        let args = try #require(UserDefaults(suiteName: name))
        defer { args.removePersistentDomain(forName: name) }
        #expect(!AppDefaults.isolated(environment: [:], arguments: args))
        #expect(AppDefaults.isolated(environment: ["XCTestConfigurationFilePath": "/x"], arguments: args))
        args.set(true, forKey: "SFIsolatedDefaults")
        #expect(AppDefaults.isolated(environment: [:], arguments: args))
    }
    #endif

    @Test("the stores' defaults are AppDefaults.shared: a saved setting never reaches the user's preferences")
    func storesWriteTheIsolatedSuite() {
        let key = SettingsStore.Key.layerHeight
        let before = UserDefaults.standard.object(forKey: key) as? Double   // read only: the user's value, if any
        let had = AppDefaults.shared.object(forKey: key)
        defer { if let had { AppDefaults.shared.set(had, forKey: key) } else { AppDefaults.shared.removeObject(forKey: key) } }
        var s = FinSettings()
        s.layerHeight = 0.07
        SettingsStore().save(s)
        #expect(AppDefaults.shared.double(forKey: key) == 0.07)
        #if DEBUG
        #expect(UserDefaults.standard.object(forKey: key) as? Double == before, "the user's sf.layerHeight is untouched")
        #endif
        #expect(SettingsStore().defaults === AppDefaults.shared && VolumeStore().defaults === AppDefaults.shared)
    }
}
