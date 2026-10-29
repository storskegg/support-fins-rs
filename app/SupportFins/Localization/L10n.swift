import Foundation

/// Where the app's strings come from: `Resources/Localizable.xcstrings`, generated from the web's translation tables
/// (`tools/i18n`, `just i18n-gen`). The key of an entry is the web's English string, spelt the way Swift's string
/// interpolation spells it: `{n}` is `%lld`, any other `{name}` is `%@`. SwiftUI looks literals up itself
/// (`Text("Draw")`); strings built in code (the readouts, tooltips held in constants) go through `JS.t` / `JS.tn`,
/// which build the same key the web's `t()` / `tn()` would look up.
enum L10n {
    /// A language's strings, to see the app in a language other than the system's (the tests do): its `.lproj` of the
    /// main bundle and a locale of it for the numbers. `nil` is the system language.
    struct Language: Sendable {
        let bundle: Bundle
        let locale: Locale
        init?(_ code: String) {
            guard let path = Bundle.main.path(forResource: code, ofType: "lproj"), let b = Bundle(path: path) else { return nil }
            bundle = b
            locale = Locale(identifier: code)
        }
    }

    /// Marks the count a text never shows (`tools/i18n/catalog.ts`): everything between two of these is dropped.
    static let hiddenMark = "\u{2063}"

    /// The localized `key` with its `{name}` placeholders replaced by `params`. `n` is the count (an integer), every
    /// other parameter a string. `hiddenCount` is the count of a `tn()` text that does not show it.
    static func text(_ key: String, _ params: [String: String], hiddenCount: Int?, in language: Language? = nil) -> String {
        let bundle = language?.bundle ?? Bundle.main, locale = language?.locale ?? Locale.current
        let pieces = key.split(separator: "{", omittingEmptySubsequences: false)
        var names: [(name: String, rest: Substring)] = []
        var head = pieces.first ?? ""
        for p in pieces.dropFirst() {
            if let close = p.firstIndex(of: "}"), p[p.startIndex..<close].allSatisfy({ $0.isLetter || $0.isNumber || $0 == "_" }), close != p.startIndex {
                names.append((String(p[p.startIndex..<close]), p[p.index(after: close)...]))
            } else if names.isEmpty {
                head += "{" + p
            } else {
                names[names.count - 1].rest += "{" + p
            }
        }
        // no arguments: the key as it stands (the compiler does not escape a literal's `%` either)
        if names.isEmpty && hiddenCount == nil {
            return String(localized: String.LocalizationValue(key), bundle: bundle, locale: locale)
        }
        var interp = String.LocalizationValue.StringInterpolation(literalCapacity: key.utf8.count, interpolationCount: names.count + 1)
        interp.appendLiteral(String(head))
        for (name, rest) in names {
            if name == "n", let v = params["n"], let n = Int(v) {
                interp.appendInterpolation(n)
            } else {
                interp.appendInterpolation(params[name] ?? "{\(name)}")
            }
            interp.appendLiteral(String(rest))
        }
        if let n = hiddenCount { interp.appendInterpolation(n) }
        var out = String(localized: String.LocalizationValue(stringInterpolation: interp), bundle: bundle, locale: locale)
        if hiddenCount != nil {
            while let a = out.range(of: hiddenMark), let b = out.range(of: hiddenMark, range: a.upperBound..<out.endIndex) {
                out.removeSubrange(a.lowerBound..<b.upperBound)
            }
        }
        return out
    }
}
