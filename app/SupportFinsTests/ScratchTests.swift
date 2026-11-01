import Foundation
import Testing

@testable import Support_Fins

/// The tests' scratch files stay in this run's folder, which goes away with the run (user suggestion S7: 36,500 folders
/// had piled up in the app container's tmp).
@Suite("Scratch files")
struct ScratchTests {
    @Test func everyScratchDirectoryIsInThisRunsFolder() throws {
        let d = try Scratch.directory()
        defer { Scratch.remove(d) }
        #expect(d.deletingLastPathComponent().standardizedFileURL == Scratch.root.standardizedFileURL)
        #expect(Scratch.root.deletingLastPathComponent().lastPathComponent == "sf-app-tests")
        #expect(Scratch.root.lastPathComponent.hasPrefix("\(getpid())-"))
    }

    @Test func testPartsAreWrittenInThisRunsFolder() throws {
        let path = try TestParts.write(TestParts.box(0, 1, 0, 1, 0, 1), name: "scratch-tests-box.stl")
        #expect(URL(fileURLWithPath: path).deletingLastPathComponent().standardizedFileURL == Scratch.root.standardizedFileURL)
    }

    /// A run that was killed leaves its folder; a later run removes it once it is a day old, and leaves a fresh one
    /// (another run going on now) alone.
    @Test func theSweepRemovesOnlyStaleFolders() throws {
        let parent = try Scratch.directory()
        defer { Scratch.remove(parent) }
        let fm = FileManager.default
        let stale = parent.appending(path: "123-stale", directoryHint: .isDirectory)
        let fresh = parent.appending(path: "456-fresh", directoryHint: .isDirectory)
        for d in [stale, fresh] {
            try fm.createDirectory(at: d, withIntermediateDirectories: true)
            try Data("x".utf8).write(to: d.appending(path: "part.stl"))
        }
        try fm.setAttributes([.modificationDate: Date().addingTimeInterval(-2 * Scratch.staleAge)], ofItemAtPath: stale.path)
        Scratch.sweep(parent, olderThan: Scratch.staleAge)
        #expect(!fm.fileExists(atPath: stale.path))
        #expect(fm.fileExists(atPath: fresh.appending(path: "part.stl").path))
    }

    /// The export's temp directory (the system's item-replacement directory, `TemporaryItems/NSIRD_*`) goes away with
    /// the write: every export used to leave an empty one behind (1,972 in the container).
    @Test func anAtomicWriteLeavesNoTempDirectory() throws {
        let dir = try Scratch.directory()
        defer { Scratch.remove(dir) }
        let target = dir.appending(path: "out.stl")
        var temp: URL?
        let wrote = try AtomicWrite.write(to: target) { path in
            temp = URL(fileURLWithPath: path)
            try Data("solid".utf8).write(to: URL(fileURLWithPath: path))
        }
        #expect(wrote)
        #expect(try Data(contentsOf: target) == Data("solid".utf8))
        let home = try #require(temp).deletingLastPathComponent()
        #expect(!FileManager.default.fileExists(atPath: home.path), "\(home.path) is left")
    }
}
