import AppKit
import SwiftUI
import Testing

@testable import Support_Fins

/// Suggestion S4: the search box's placeholder read "Search settin…" in the inspector. The box sits in a row of the
/// inspector's grouped form; at the inspector's narrowest (240 pt) its text field must hold the whole placeholder, in
/// every language the app ships.
@Suite("Search field layout")
@MainActor
struct SearchFieldLayoutTests {
    private func fields(in view: NSView) -> [NSTextField] {
        var out: [NSTextField] = []
        if let f = view as? NSTextField, f.isEditable { out.append(f) }
        for v in view.subviews { out += fields(in: v) }
        return out
    }

    /// The placeholder in each language, as the catalog has it ("Search settings" where a language has none).
    private func placeholders() throws -> [String: String] {
        var out = ["en": "Search settings"]
        for lang in ["de", "zh-Hans", "pt", "ar", "nl"] {
            guard let path = Bundle.main.path(forResource: lang, ofType: "lproj"), let b = Bundle(path: path) else { continue }
            out[lang] = b.localizedString(forKey: "Search settings", value: "Search settings", table: nil)
        }
        #expect(out.count == 6, "every language's bundle: \(out.keys.sorted())")
        return out
    }

    @Test(arguments: [240.0, 280.0])
    func thePlaceholderFitsInTheInspectorsForm(width: Double) throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: 120), styleMask: [.titled],
                              backing: .buffered, defer: false)
        let hosting = NSHostingView(rootView: Form { Section { SearchField(search: SettingsSearch()) } }
            .formStyle(.grouped).frame(width: width, height: 120))
        window.contentView = hosting
        window.layoutIfNeeded()
        for _ in 0..<5 {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
            hosting.layoutSubtreeIfNeeded()
        }
        let field = try #require(fields(in: hosting).first, "the search box's text field")
        let font = field.font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize)
        for (lang, text) in try placeholders() {
            let need = NSAttributedString(string: text, attributes: [.font: font]).size().width
            #expect(field.frame.width >= need + 4, "\(lang): \"\(text)\" needs \(need) pt, the field is \(field.frame.width) pt wide")
        }
    }
}
