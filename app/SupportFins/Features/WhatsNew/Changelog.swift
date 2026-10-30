import Foundation

/// The releases the landing card and Help > What's new list: `web/changelog.json` at the pinned upstream commit, reduced
/// by `tools/whatsnew/gen.js` (`just whatsnew-gen`) to `Resources/changelog.json`. Released entries only, newest release
/// first, each with the lines `web/changelog.js` puts on the card (an entry's highlights stand in for its text, newest
/// entry first). The entries stay English, as on the page.
struct Changelog: Decodable, Equatable, Sendable {
    struct Release: Decodable, Equatable, Identifiable, Sendable {
        /// "1.0.1"
        let version: String
        /// "v1.0.1"
        let tag: String
        /// "2026-10-28"
        let date: String
        let lines: [String]
        var id: String { tag }
    }

    /// The app's version (`app/Version.xcconfig`): the newest release's, or the one the first release will take.
    let version: String
    let releases: [Release]

    /// The newest release: the card names it in its heading and the top bar showed it on the page.
    var newest: Release? { releases.first }
    /// The older releases, each listed under its own "v1.0.0 2026-10-28" line (`p.rel`).
    var older: [Release] { Array(releases.dropFirst()) }
    /// Nothing released: the page shows no What's new at all, and neither does the app.
    var isEmpty: Bool { releases.isEmpty }

    /// `RELEASES_URL` (web/changelog.js): "See all releases".
    static let releasesURL = URL(string: "https://github.com/gittrahan/support-fins/releases")!

    static let none = Changelog(version: "", releases: [])

    /// The file a DEBUG launch's `-SFChangelogFixture <path>` names to read instead of the bundled list (the UI tests
    /// use it, since the real list is empty until a release is stamped). Compiled out of Release builds (X-15): there
    /// it is always nil, so a Release app never reads a list from a launch argument or a default.
    static func fixtureURL(defaults: UserDefaults = .standard) -> URL? {
        #if DEBUG
        return defaults.string(forKey: "SFChangelogFixture").map { URL(fileURLWithPath: $0) }
        #else
        return nil
        #endif
    }

    /// The bundled list (or the DEBUG fixture, see `fixtureURL`).
    static let bundled: Changelog = {
        guard let url = fixtureURL() ?? Bundle.main.url(forResource: "changelog", withExtension: "json") else { return .none }
        return load(url) ?? .none
    }()

    static func load(_ url: URL) -> Changelog? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(Changelog.self, from: data)
    }
}
