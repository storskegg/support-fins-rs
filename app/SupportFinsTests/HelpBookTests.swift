import Foundation
import Testing

@testable import Support_Fins

/// The Help Book (`Help/SupportFins.help`, built from the web's pages by `tools/helpbook`) is in the app and registered, so
/// the system's Help > Support Fins Help opens it.
@MainActor
@Suite("Help Book")
struct HelpBookTests {
    private var book: URL? { Bundle.main.url(forResource: "SupportFins", withExtension: "help") }

    @Test func theBookIsRegisteredInTheAppsInfo() throws {
        #expect(Bundle.main.object(forInfoDictionaryKey: "CFBundleHelpBookFolder") as? String == "SupportFins.help")
        let name = Bundle.main.object(forInfoDictionaryKey: "CFBundleHelpBookName") as? String
        #expect(name == "com.storskegg.SupportFins.help")
        let book = try #require(book, "SupportFins.help is not in the app")
        let info = try #require(NSDictionary(contentsOf: book.appending(path: "Contents/Info.plist")) as? [String: Any])
        #expect(info["CFBundleIdentifier"] as? String == name, "the book's identifier is the name the app registers")
        #expect(info["HPDBookAccessPath"] as? String == "index.html" && info["HPDBookTitle"] as? String == "Support Fins Help")
        #expect(FileManager.default.fileExists(atPath: book.appending(path: "Contents/Resources/SupportFins.helpindex").path))
    }

    @Test func everyPageTheHomeLinksToIsInTheBookAndEveryLocalLinkLandsOnAPage() throws {
        let lproj = try #require(book).appending(path: "Contents/Resources/en.lproj")
        let pages = try FileManager.default.contentsOfDirectory(atPath: lproj.path).filter { $0.hasSuffix(".html") }
        #expect(pages.count >= 10)
        let home = try String(contentsOf: lproj.appending(path: "index.html"), encoding: .utf8)
        #expect(home.contains("<title>Support Fins Help</title>"))
        for page in pages {
            let text = try String(contentsOf: lproj.appending(path: page), encoding: .utf8)
            #expect(text.contains("<h1>"), "\(page)")
            let links = text.matches(of: /href="([^"]+)"/).map { String($0.1) }.filter { !$0.hasPrefix("http") }
            for link in links {
                let file = link.split(separator: "#").first.map(String.init) ?? link
                #expect(FileManager.default.fileExists(atPath: lproj.appending(path: file).standardizedFileURL.path), "\(page) links to \(link)")
            }
            if page != "index.html" { #expect(home.contains("href=\"\(page)\"") || page == "index.html", "\(page) is not reachable from the home page") }
        }
    }

    @Test func theHelpMenuHasTheSystemsHelpItemAndTheLinks() throws {
        let titles = FeatureSet.entries(of: .help, in: nil).flatMap(\.snapshot)
        #expect(titles.contains("Support Fins on GitHub") && titles.contains("Support the project") && titles.contains("Report a problem"))
    }
}
