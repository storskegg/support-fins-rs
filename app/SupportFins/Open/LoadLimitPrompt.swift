import Foundation
import SupportFinsCore

/// The "Load anyway?" question a file over a load limit asks (`FfiError.LimitExceeded`, docs/FFI.md "Load limits"):
/// plain words with the numbers the engine measured, never the engine's own English sentence and never a limit
/// written down here -- the limits are read from the error because they will be tuned.
struct LoadLimitPrompt: Identifiable, Equatable {
    let id = UUID()
    var kind: LimitKind
    /// Triangles, bytes or millimetres by `kind`.
    var estimate: Double
    var limit: Double
    /// The file's name, for the title (none for a generated part).
    var name: String?

    /// What a triangle costs once loaded, measured by track 2B (a 7.7 M-triangle file loads in 2.3 GB).
    static let bytesPerTriangle = 280.0

    init(kind: LimitKind, estimate: Double, limit: Double, name: String?) {
        self.kind = kind
        self.estimate = estimate
        self.limit = limit
        self.name = name
    }

    init?(_ error: FfiError, name: String?) {
        guard case .LimitExceeded(let kind, let estimate, let limit, _) = error else { return nil }
        self.init(kind: kind, estimate: estimate, limit: limit, name: name)
    }

    func title(in language: L10n.Language? = nil) -> String {
        guard let name else { return JS.t("Load this file anyway?", in: language) }
        return JS.t("Load {name} anyway?", ["name": name], in: language)
    }

    func message(in language: L10n.Language? = nil) -> String {
        let locale = language?.locale ?? .current
        switch kind {
        case .triangles:
            return JS.t("This file expands to {count} triangles, which may need about {memory} of memory.",
                        ["count": Self.count(estimate, locale), "memory": Self.memory(estimate * Self.bytesPerTriangle, locale)], in: language)
        case .bytes:
            return JS.t("This file unpacks to {size} (the limit is {limit}).",
                        ["size": Self.archiveSize(estimate, locale), "limit": Self.archiveSize(limit, locale)], in: language)
        case .dimension:
            return JS.t("This model is {size} across (the limit is {limit}).",
                        ["size": Self.length(estimate, locale), "limit": Self.length(limit, locale)], in: language)
        }
    }

    private static func number(_ x: Double, digits: Int, _ locale: Locale) -> String {
        let f = NumberFormatter()
        f.locale = locale
        f.numberStyle = .decimal
        f.maximumFractionDigits = digits
        f.minimumFractionDigits = 0
        return f.string(from: NSNumber(value: x)) ?? String(x)
    }

    static func count(_ n: Double, _ locale: Locale) -> String { number(n.rounded(), digits: 0, locale) }

    /// "about N GB": decimal gigabytes, whole once past 10, one decimal below; megabytes under a gigabyte.
    static func memory(_ bytes: Double, _ locale: Locale) -> String {
        if bytes >= 1e9 { return number(bytes / 1e9, digits: bytes >= 1e10 ? 0 : 1, locale) + " GB" }
        return number(max(1, bytes / 1e6), digits: 0, locale) + " MB"
    }

    /// An archive's unpacked size in binary units, so the 2 GiB limit reads "2 GB".
    static func archiveSize(_ bytes: Double, _ locale: Locale) -> String {
        let gib = 1_073_741_824.0, mib = 1_048_576.0
        if bytes >= gib { return number(bytes / gib, digits: 1, locale) + " GB" }
        return number(max(1, bytes / mib), digits: 0, locale) + " MB"
    }

    /// A length given in millimetres, as mm, m or km.
    static func length(_ mm: Double, _ locale: Locale) -> String {
        if mm >= 1_000_000 { return number(mm / 1_000_000, digits: 1, locale) + " km" }
        if mm >= 1000 { return number(mm / 1000, digits: 1, locale) + " m" }
        return number(mm, digits: 1, locale) + " mm"
    }
}
