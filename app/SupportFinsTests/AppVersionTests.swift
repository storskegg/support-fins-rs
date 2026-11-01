import Foundation
import SupportFinsCore
import Testing
@testable import Support_Fins

/// The app's version and build number (docs/AGENTS.md "Versioning and releases"): one version, Cargo.toml's, in the
/// bundle (MARKETING_VERSION, app/Version.xcconfig) and in the engine; the build number is HEAD's commit count, set at
/// build time by the app target's "Build number" script.
@Suite("AppVersion")
struct AppVersionTests {
    @Test("the bundle's version is the engine's: one source of truth")
    func versionIsTheEngines() {
        #expect(AppVersion.version.range(of: #"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$"#, options: .regularExpression) != nil,
                "a semver triple without leading zeros: \(AppVersion.version)")
        #expect(engineVersion().hasPrefix(AppVersion.version + " (upstream "),
                "MARKETING_VERSION (app/Version.xcconfig) is Cargo.toml's version: \(AppVersion.version) vs \(engineVersion())")
    }

    @Test("the build number is the commit count, set at build time")
    func buildIsTheCommitCount() throws {
        let n = try #require(Int(AppVersion.build), "CFBundleVersion is a number: \(AppVersion.build)")
        #expect(n > 0, "the Build number script set it (CURRENT_PROJECT_VERSION is 0)")
    }

    @Test("crash reports, Help and the report link name the version and the build")
    @MainActor func shownWhereItShouldBe() {
        let display = "\(AppVersion.version) (\(AppVersion.build))"
        #expect(AppVersion.display == display)
        #expect(CrashReporter.appVersion == display)
        #expect(HelpFeature.versionLine == "Version \(display)")
        #expect(FeatureSet.entries(of: .help, in: nil).flatMap(\.snapshot).last == "(Version \(display))")
        #expect(ReportModel.site.hasPrefix("Support Fins for Mac \(display), engine "))
    }
}
