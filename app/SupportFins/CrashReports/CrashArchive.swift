import Foundation
import SupportFinsCore

/// Save crash report…: a zip (the engine's `zip_store`) of the report files, the breadcrumbs of the last session and,
/// only when the user ticked the box, the model file that was open.
enum CrashArchive {
    /// The entry name for a file: ASCII only (the zip writer stores names verbatim), a folder up front.
    static func entryName(_ folder: String, _ file: URL) -> String {
        let ascii = file.lastPathComponent.unicodeScalars.map { $0.isASCII && $0 != "/" ? String($0) : "_" }.joined()
        return "\(folder)/\(ascii)"
    }

    static func make(reports: [URL], extras: [URL] = [], model: URL?) throws -> Data {
        var names: [String] = []
        var datas: [Data] = []
        for f in reports + extras {
            guard let d = try? Data(contentsOf: f) else { continue }
            names.append(entryName("reports", f))
            datas.append(d)
        }
        if let model {
            names.append(entryName("model", model))
            datas.append(try Data(contentsOf: model))
        }
        return Data(try crashZip(names: names, datas: datas))
    }
}
