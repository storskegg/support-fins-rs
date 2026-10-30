import Foundation

/// Small 3MF packages for the tests, written with stored (uncompressed) ZIP entries. The engine reads them through
/// `load_parts` like any file.
enum TestThreeMF {
    private static let contentTypes = "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/></Types>"
    private static let rels = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>"
    private static let triangle = "<mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"10\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"10\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh>"

    private static func model(objects: String, item: Int) -> String {
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\"><resources>\(objects)</resources><build><item objectid=\"\(item)\"/></build></model>"
    }

    /// One triangle, and the session text (if any) as the app writes it, `Metadata/support_fins.json`.
    static func triangle(session: String?) -> Data {
        var files = [("3D/3dmodel.model", model(objects: "<object id=\"1\">\(triangle)</object>", item: 1))]
        if let session { files.append(("Metadata/support_fins.json", session)) }
        return zip(files)
    }

    /// Object 1 is a triangle, object k has `fan` components of k - 1: `fan`^`depth` triangles once expanded.
    static func fanOut(depth: Int, fan: Int) -> Data {
        var objects = "<object id=\"1\" type=\"model\">\(triangle)</object>"
        for k in 2...(depth + 1) {
            objects += "<object id=\"\(k)\" type=\"model\"><components>" + String(repeating: "<component objectid=\"\(k - 1)\"/>", count: fan) + "</components></object>"
        }
        return zip([("3D/3dmodel.model", model(objects: objects, item: depth + 1))])
    }

    private static func zip(_ extra: [(String, String)]) -> Data {
        let files = [("[Content_Types].xml", contentTypes), ("_rels/.rels", rels)] + extra
        var out = Data(), central = Data()
        func le<T: FixedWidthInteger>(_ v: T) -> [UInt8] { withUnsafeBytes(of: v.littleEndian) { Array($0) } }
        for (name, text) in files {
            let body = Data(text.utf8), nameBytes = Data(name.utf8), crc = crc32(body), offset = UInt32(out.count)
            let common = le(UInt16(20)) + le(UInt16(0)) + le(UInt16(0)) + le(UInt16(0)) + le(UInt16(0x21)) + le(crc) + le(UInt32(body.count)) + le(UInt32(body.count))
            out += le(UInt32(0x04034b50)) + common + le(UInt16(nameBytes.count)) + le(UInt16(0)) + nameBytes + body
            central += le(UInt32(0x02014b50)) + le(UInt16(20)) + common + le(UInt16(nameBytes.count)) + le(UInt16(0)) + le(UInt16(0))
                + le(UInt16(0)) + le(UInt16(0)) + le(UInt32(0)) + le(offset) + nameBytes
        }
        let start = UInt32(out.count)
        out += central
        out += le(UInt32(0x06054b50)) + le(UInt16(0)) + le(UInt16(0)) + le(UInt16(files.count)) + le(UInt16(files.count))
            + le(UInt32(central.count)) + le(start) + le(UInt16(0))
        return out
    }

    private static func crc32(_ data: Data) -> UInt32 {
        var crc: UInt32 = 0xFFFF_FFFF
        for byte in data {
            crc ^= UInt32(byte)
            for _ in 0..<8 { crc = crc & 1 == 1 ? (crc >> 1) ^ 0xEDB8_8320 : crc >> 1 }
        }
        return ~crc
    }
}
