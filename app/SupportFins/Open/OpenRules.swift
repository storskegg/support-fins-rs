import Foundation

/// What the open flow accepts (`web/ui/io.js`; `docs/FFI.md` `load_parts`).
enum OpenRules {
    /// STL, OBJ and 3MF. STEP is M6, so a dropped .step is refused for now (the web reads it).
    static let extensions: Set<String> = ["stl", "obj", "3mf"]

    static func accepts(_ url: URL) -> Bool { extensions.contains(url.pathExtension.lowercased()) }

    struct Unsupported: LocalizedError {
        var errorDescription: String? { "Open an STL, OBJ or 3MF file. STEP import is not available yet." }
    }
}
