import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    /// `public.standard-tesselated-geometry-format` (the system's spelling).
    static let stl = UTType("public.standard-tesselated-geometry-format") ?? UTType(filenameExtension: "stl") ?? .data
    static let obj = UTType("public.geometry-definition-format") ?? UTType(filenameExtension: "obj") ?? .data
    static let threeMF = UTType("com.storskegg.supportfins.3mf") ?? UTType(filenameExtension: "3mf") ?? .data
    /// What the system calls a `.3mf` on this Mac: there is no system type for it, so whichever app declared one
    /// first (Shapr3D's, for instance) names it. The file's own type is what a document open is matched on.
    static let threeMFBySuffix = UTType(filenameExtension: "3mf", conformingTo: .data)
    /// STEP has no system type either: this app declares one (`project.yml`), and whichever app declared a type for
    /// the suffix first names it on this Mac, so the suffix types are listed too.
    static let step = UTType("com.storskegg.supportfins.step") ?? UTType(filenameExtension: "step") ?? .data
    static let stepBySuffix = ["step", "stp"].compactMap { UTType(filenameExtension: $0, conformingTo: .data) }
}

/// A part file opened read-only (a part import; a 3MF with a session restores it, `Session/`). The window loads the file from its URL
/// through the engine; the document itself keeps only the name.
struct PartDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.stl, .obj, .threeMF, .step] + (UTType.threeMFBySuffix.map { [$0] } ?? []) + UTType.stepBySuffix
    static let writableContentTypes: [UTType] = []

    var filename: String

    init(configuration: ReadConfiguration) throws {
        filename = configuration.file.filename ?? "Part"
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        throw CocoaError(.featureUnsupported)
    }
}
