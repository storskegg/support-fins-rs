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

    /// `x.toFixed(1)`. A tie at one decimal needs x = (2n+1)/20, which no double is, so C's exact rounding agrees.
    static func toFixed1(_ x: Double) -> String { String(format: "%.1f", x) }

    /// The English `tn`: `singular` for 1, else `plural`, with `{n}` and any other `{key}` replaced.
    static func tn(_ singular: String, _ plural: String, _ n: Int, _ params: [String: String] = [:]) -> String {
        t(n == 1 ? singular : plural, ["n": String(n)].merging(params) { $1 })
    }

    /// The English `t`: the key with its `{param}`s replaced.
    static func t(_ key: String, _ params: [String: String] = [:]) -> String {
        var s = key
        for (k, v) in params { s = s.replacingOccurrences(of: "{\(k)}", with: v) }
        return s
    }
}
