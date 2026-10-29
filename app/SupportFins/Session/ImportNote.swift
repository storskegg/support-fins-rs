import Foundation

/// The import note of the status card in the system language. The engine writes it in English, as `renderImportNote`
/// (`web/ui/io.js:26-70`) builds it: `3MF: a; b.` of clauses the web passes through `t()` / `tn()`. Each clause is
/// matched against the web's templates and said again through `JS.t` / `JS.tn`; one that matches none stays as it is.
enum ImportNote {
    /// The web's clauses: a text, or a `tn()` singular and plural.
    private static let clauses: [(String, String?)] = [
        ("imported “{name}” of {total} objects", nil),
        ("merged {n} of {total} objects into one part", nil),
        ("merged {n} bodies into one part", nil),
        ("ignored {n} support/non-printable body", "ignored {n} support/non-printable bodies"),
        ("cut out {n} negative part", "cut out {n} negative parts"),
        ("in your slicer, walls inside a negative part show once you slice", nil),
        ("negative parts not cut: the part is not a closed solid, so its fins are for the uncut shape", nil),
        ("negative parts not cut: they would remove the whole part", nil),
        ("negative parts not cut: the cutter did not load (try opening the file again)", nil),
        ("{n} negative part not cut: it is not a closed solid", "{n} negative parts not cut: they are not closed solids"),
        ("kept {n} modifier/support blocker for the 3MF export", "kept {n} modifiers/support blockers for the 3MF export"),
        ("converted from {unit} to mm", nil),
        ("restored {n} hand-placed support and the settings", "restored {n} hand-placed supports and the settings"),
        ("restored the settings", nil),
        ("restored {n} hand-placed support; the plate keeps its settings", "restored {n} hand-placed supports; the plate keeps its settings"),
        ("the plate keeps its settings", nil),
        ("saved supports not loaded: the part changed since it was exported", nil),
        ("saved supports not loaded: the saved session is damaged", nil),
        ("tessellated at 0.01 mm", nil),
    ]

    static func localized(_ note: String, in language: L10n.Language? = nil) -> String {
        guard let colon = note.range(of: ": "), note.hasSuffix("."), ["3MF", "OBJ", "STEP"].contains(String(note[..<colon.lowerBound])) else { return note }
        let tag = note[..<colon.lowerBound]
        let body = note[colon.upperBound...].dropLast()
        let parts = body.components(separatedBy: "; ").map { clause($0, in: language) }
        return "\(tag): \(parts.joined(separator: "; "))."
    }

    private static func clause(_ text: String, in language: L10n.Language?) -> String {
        for (one, many) in clauses {
            if let params = match(text, template: one) {
                if let many { return JS.tn(one, many, Int(params["n"] ?? "") ?? 1, params, in: language) }
                return JS.t(one, params, in: language)
            }
            if let many, let params = match(text, template: many) {
                return JS.tn(one, many, Int(params["n"] ?? "") ?? 2, params, in: language)
            }
        }
        return text
    }

    /// The placeholders of `template` read out of `text`, or nil when `text` is not that template.
    private static func match(_ text: String, template: String) -> [String: String]? {
        var names: [String] = []
        var pattern = ""
        var rest = Substring(template)
        while let open = rest.firstIndex(of: "{"), let close = rest[open...].firstIndex(of: "}") {
            pattern += NSRegularExpression.escapedPattern(for: String(rest[..<open]))
            names.append(String(rest[rest.index(after: open)..<close]))
            pattern += "(.+?)"
            rest = rest[rest.index(after: close)...]
        }
        pattern += NSRegularExpression.escapedPattern(for: String(rest))
        guard let re = try? NSRegularExpression(pattern: "^" + pattern + "$"),
              let m = re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) else { return nil }
        var out: [String: String] = [:]
        for (i, name) in names.enumerated() { if let r = Range(m.range(at: i + 1), in: text) { out[name] = String(text[r]) } }
        return out
    }
}
