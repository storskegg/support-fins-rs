import Testing
import UniformTypeIdentifiers
@testable import Support_Fins

@MainActor
@Suite("Part documents")
struct PartDocumentTests {
    @Test func opensTheThreeMeshFormatsReadOnly() {
        #expect(PartDocument.readableContentTypes.count >= 3)
        #expect(PartDocument.writableContentTypes.isEmpty)
        // STL and OBJ are system types; 3MF is declared by the app's Info.plist (not registered in a test host)
        for ext in ["stl", "obj"] {
            let t = UTType(filenameExtension: ext)
            #expect(t != nil && PartDocument.readableContentTypes.contains { t!.conforms(to: $0) }, "\(ext)")
        }
        #expect(PartDocument.readableContentTypes.contains(.threeMF))
    }

    @Test func theGeneratedLoaderGivesACube() async throws {
        let model = ViewerModel(loader: GeneratedLoader())
        await model.load(url: nil)
        #expect(model.status == nil)
        #expect(model.controller?.faceCount == 12)
    }
}
