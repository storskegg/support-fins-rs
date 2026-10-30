import Foundation
import Testing

/// The upstream checkout's stress models (`prototype/stress/models/<name>.stl`), found through the repo's UPSTREAM file.
func modelPath(_ name: String) throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }  // Tests/SupportFinsCoreTests/<file> -> repo root
    let upstream = try String(contentsOf: url.appendingPathComponent("UPSTREAM"), encoding: .utf8)
    let line = try #require(upstream.split(separator: "\n").first { $0.hasPrefix("checkout=") })
    var root = String(line.dropFirst("checkout=".count)).trimmingCharacters(in: .whitespaces)
    if root.hasPrefix("~") { root = NSHomeDirectory() + root.dropFirst() }
    return root + "/prototype/stress/models/\(name).stl"
}
