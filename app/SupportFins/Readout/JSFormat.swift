import Foundation

/// The number formatting JS does in `ui/readout.js` and `ui/part.js`, which Swift's `String(format:)` does not
/// match on exact halves (`(12.5).toFixed(0)` is "13", C prints "12").
enum JS {
    /// `Math.round(x)`: ties go up, and a value just under .5 stays down.
    static func round(_ x: Double) -> Double {
        let r = x.rounded(.down)
        return x - r >= 0.5 ? r + 1 : r
    }

    /// `x.toFixed(0)` for x >= 0 (a tie takes the larger integer).
    static func toFixed0(_ x: Double) -> String { String(Int64(round(x))) }

    /// `x.toFixed(1)` for x >= 0. An exact tie (an odd multiple of 0.25, such as 0.25 or 2.25) takes the larger n
    /// as the spec says, where C's `%.1f` rounds it to even; every other value C already rounds exactly.
    static func toFixed1(_ x: Double) -> String {
        let q = x * 4 // exact: a power-of-two scaling
        if q == q.rounded(), Int64(q) % 2 != 0 {
            return String(format: "%.1f", (x * 10 + 0.5).rounded(.down) / 10) // x*10 is exactly k + 0.5 here
        }
        return String(format: "%.1f", x)
    }

    /// `tn(singular, plural, n, params)` of `web/ui/i18n.js`: the text for a count, in the system language. The web
    /// looks the plural key up first (the language's plural forms live on it) and picks the form by the language's
    /// plural rule; the catalog has both keys with the same forms, English included (`tools/i18n/catalog.ts`).
    /// A text that never shows its count still varies by it: the count goes on the key as a last, hidden `%lld`.
    static func tn(_ singular: String, _ plural: String, _ n: Int, _ params: [String: String] = [:], in language: L10n.Language? = nil) -> String {
        L10n.text(plural, ["n": String(n)].merging(params) { $1 }, hiddenCount: plural.contains("{n}") ? nil : n, in: language)
    }

    /// `t(key, params)` of `web/ui/i18n.js`: the web's English string in the system language (the catalog is
    /// generated from the web's tables), with its `{param}`s replaced. A string the web has no translation for stays
    /// English, as it does on the web.
    static func t(_ key: String, _ params: [String: String] = [:], in language: L10n.Language? = nil) -> String {
        L10n.text(key, params, hiddenCount: nil, in: language)
    }
}
