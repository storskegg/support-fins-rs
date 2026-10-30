import Foundation
import Testing
@testable import Support_Fins

/// What's new (tests/changelog.test.js and changelog_i18n.test.js, as far as they apply to the app): the app reads the list
/// `tools/whatsnew/gen.js` writes from `web/changelog.json`; the web's own rules (the entries' shape, released entries
/// only, newest release first, highlights in place of text, the card matching what `web/changelog.js` writes) are checked
/// by that script, and `just whatsnew-gen --check` (in `just gate`) fails when the bundled files are stale.
@Suite("WhatsNew")
struct WhatsNewTests {
    /// Two releases as the generator writes them: the newest first, each with the card's lines.
    static let two = Changelog(version: "1.0.1", releases: [
        .init(version: "1.0.1", tag: "v1.0.1", date: "2026-11-03", lines: ["last", "later"]),
        .init(version: "1.0.0", tag: "v1.0.0", date: "2026-10-28", lines: ["one", "two <b>"]),
    ])

    @Test("the generator's JSON decodes, newest release first")
    func decodes() throws {
        let json = """
        {"version":"1.0.1","releases":[{"version":"1.0.1","tag":"v1.0.1","date":"2026-11-03","lines":["last","later"]},\
        {"version":"1.0.0","tag":"v1.0.0","date":"2026-10-28","lines":["one","two <b>"]}]}
        """
        let c = try JSONDecoder().decode(Changelog.self, from: Data(json.utf8))
        #expect(c == Self.two)
        #expect(c.newest?.tag == "v1.0.1" && c.older.map(\.tag) == ["v1.0.0"], "the newest is named in the heading, the older ones above their lines")
        #expect(!c.isEmpty && Changelog.none.isEmpty && Changelog.none.newest == nil && Changelog.none.older.isEmpty)
    }

    /// X-15: `-SFChangelogFixture` is a DEBUG-only hook; a Release build never reads a list from a default. (The
    /// Release half is checked on the built binary by `just app-release-hooks`.)
    @Test("the fixture default is honoured in DEBUG builds only")
    func fixtureHook() throws {
        let suite = "sf-fixture-\(UUID().uuidString)"
        let d = try #require(UserDefaults(suiteName: suite))
        defer { d.removePersistentDomain(forName: suite) }
        #expect(Changelog.fixtureURL(defaults: d) == nil)
        d.set("/tmp/x.json", forKey: "SFChangelogFixture")
        #if DEBUG
        #expect(Changelog.fixtureURL(defaults: d)?.path == "/tmp/x.json")
        #else
        #expect(Changelog.fixtureURL(defaults: d) == nil)
        #endif
    }

    @Test("a file that is not a changelog is no changelog")
    func broken() throws {
        let url = FileManager.default.temporaryDirectory.appending(path: "whatsnew-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        try Data("{\"releases\": 3}".utf8).write(to: url)
        #expect(Changelog.load(url) == nil)
        #expect(Changelog.load(url.appending(path: "missing")) == nil)
    }

    @Test("nothing released: nothing is shown, as on the page")
    @MainActor func nothingReleased() {
        #expect(WhatsNewFeature.entries(for: .none, show: {}).isEmpty)
    }

    @Test("a release puts What's new in the Help menu, between Calibration and the links")
    @MainActor func menuItem() throws {
        let sections = WhatsNewFeature.entries(for: Self.two, show: {})
        let section = try #require(sections.first)
        #expect(sections.count == 1 && section.menu == .help && section.order == 15)
        #expect(section.entries.flatMap(\.snapshot) == ["What's new"])
        #expect(WhatsNewFeature.entries(for: Self.two, show: {}, enabled: false).count == 1)
    }

    @Test("the chrome is translated in every language (changelog_i18n.test.js)")
    func chrome() throws {
        for code in ["de", "zh-Hans", "pt", "ar", "nl"] {
            let lang = try #require(L10n.Language(code))
            for key in ["What's new", "See all releases", "Version"] {
                let same = code == "de" && key == "Version"   // German spells it the same
                #expect(same || JS.t(key, in: lang) != key, "\(code): \(key)")
            }
        }
    }

    @Test("the app's version is the changelog's")
    func version() throws {
        let bundled = try #require(Bundle.main.url(forResource: "changelog", withExtension: "json").flatMap(Changelog.load), "changelog.json is bundled")
        #expect(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String == bundled.version,
                "MARKETING_VERSION (app/Version.xcconfig) is what tools/whatsnew/gen.js wrote")
        #expect(Changelog.bundled == bundled)
    }
}
