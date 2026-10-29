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
    /// A web string cut at its `{name}` placeholders: the text before the first, then each name with the text after it.
    private static func placeholders(_ key: String) -> (head: Substring, names: [(name: String, rest: Substring)]) {
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
        return (head, names)
    }

    /// Whether the language translates `key` (a `tn()` plural key; `hiddenCount` as for `text`). The catalog key is
    /// the one `catalogKey` of `tools/i18n/catalog.ts` writes: `{n}` is `%lld`, other names `%@`, `%` doubled when
    /// there are arguments. Xcode copies an untranslated key into every language with the key itself as its text, so
    /// both a missing key and one that comes back unchanged mean the language has no translation.
    static func translates(_ key: String, hiddenCount: Int?, in language: Language? = nil) -> Bool {
        let (head, names) = placeholders(key)
        let args = !names.isEmpty || hiddenCount != nil
        func esc(_ s: Substring) -> String { args ? s.replacingOccurrences(of: "%", with: "%%") : String(s) }
        var format = esc(head)
        for (name, rest) in names { format += (name == "n" ? "%lld" : "%@") + esc(rest) }
        if hiddenCount != nil { format += "%lld" }
        let missing = "\u{1}missing"
        let found = (language?.bundle ?? Bundle.main).localizedString(forKey: format, value: missing, table: nil)
        return found != missing && found != format
    }

    static func text(_ key: String, _ params: [String: String], hiddenCount: Int?, in language: Language? = nil) -> String {
        let bundle = language?.bundle ?? Bundle.main, locale = language?.locale ?? Locale.current
        let (head, names) = placeholders(key)
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
