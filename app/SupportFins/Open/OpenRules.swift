import Foundation

/// What the open flow accepts (`web/ui/io.js`; `docs/FFI.md` `load_parts`).
enum OpenRules {
    /// STL, OBJ, 3MF and STEP (`.step` / `.stp`, read through OpenCascade by `sf-step`).
    static let extensions: Set<String> = ["stl", "obj", "3mf", "step", "stp"]

    static func isStep(_ url: URL) -> Bool { ["step", "stp"].contains(url.pathExtension.lowercased()) }

    static func accepts(_ url: URL) -> Bool { extensions.contains(url.pathExtension.lowercased()) }

    struct Unsupported: LocalizedError {
        var errorDescription: String? { "Open an STL, 3MF, STEP or OBJ file." }
    }
}
