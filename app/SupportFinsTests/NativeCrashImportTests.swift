import Darwin
import Foundation
import Testing

import SupportFinsCore
@testable import Support_Fins

/// A function whose name says "panicking", like the Rust panic machinery's frames in a panic that aborted.
@_cdecl("sf_test_rust_panicking_stand_in")
@inline(never)
public func sfTestRustPanickingStandIn() {}

/// Native crash capture, the next launch (1E): a raw dump in the crash folder becomes one readable report, the Rust
/// panic report of a panic that aborted takes the native stack instead of a second report, MetricKit's later copy
/// says it was captured natively, and tests write their dumps into a folder of their own. The dumps here are canned
/// (written by the test): nothing crashes. The handlers themselves are proved in a child process
/// (SupportFinsCoreTests, NativeCrashTests).
@MainActor
@Suite("Native crash reports")
struct NativeCrashImportTests {
    private func tempFolder() -> CrashFolder {
        let url = FileManager.default.temporaryDirectory.appending(path: "sf-native-import-\(UUID().uuidString)", directoryHint: .isDirectory)
        let folder = CrashFolder(url: url)
        folder.create()
        return folder
    }

    private static func address(of symbol: String) throws -> UInt {
        let p = try #require(dlsym(UnsafeMutableRawPointer(bitPattern: -2), symbol), "\(symbol) is loaded")
        return UInt(bitPattern: p)
    }

    /// A raw dump as the handler writes it, with this process's images (so the frames name themselves via dladdr).
    private func cannedDump(signal: Int32, code: Int, frames: [UInt], time: Date, context: String?) throws -> String {
        let loaded = NativeSymbolizer.loadedImages()
        var images: [String] = []
        for f in frames {
            var info = Dl_info()
            guard dladdr(UnsafeRawPointer(bitPattern: f), &info) != 0 else { continue }
            let header = UInt(bitPattern: info.dli_fbase)
            let uuid = try #require(loaded.first { $0.value == header }?.key)
            images.append("image 0x\(String(header, radix: 16)) 0x10000000 \(uuid) \(String(cString: info.dli_fname))")
        }
        var text = "SFNATIVE 1\npid 777\ntime \(Int(time.timeIntervalSince1970))\napp 0.9 (12)\nsection signal\nsignal \(signal)\ncode \(code)\naddr 0x8\nmain-thread 1\n"
        text += "reg pc 0x\(String(frames[0], radix: 16))\nreg lr 0x0\nreg fp 0x0\nreg sp 0x0\n"
        text += frames.map { "frame 0x\(String($0, radix: 16))\n" }.joined()
        text += images.map { $0 + "\n" }.joined()
        if let context { text += "context \(context.utf8.count)\n\(context)\n" }
        return text + "end\n"
    }

    private func plantDump(_ folder: CrashFolder, _ text: String) throws -> URL {
        let url = folder.url.appending(path: "native-1793327040-777.sfdump")
        try Data(text.utf8).write(to: url)
        return url
    }

    private func json(_ url: URL) throws -> [String: Any] {
        try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    @Test func aNativeDumpBecomesTheOneReportOfTheRunThatCrashed() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        // the run that crashed: its marker, its saved breadcrumbs, and the dump its handler wrote
        let first = CrashReporter(folder: folder)
        first.start(register: false)
        first.note("open file: older.stl (STL), 1 objects, 12 triangles")
        first.saveNow()
        let context = NativeCrashCapture.contextJSON(breadcrumbs: ["t  open file: a.stl (STL), 1 objects, 12 triangles", "t  edit: Add Fins"],
                                                     settingsJSON: "{\"options\":\(FinSettings().optionsJSON()),\"tunables\":\(FinSettings().tunablesJSON())}")
        let abortAt = try Self.address(of: "abort")
        let caller = try Self.address(of: "sf_test_rust_panicking_stand_in")
        let raw = try plantDump(folder, try cannedDump(signal: SIGSEGV, code: Int(SEGV_ACCERR), frames: [abortAt + 8, caller + 4], time: Date(), context: context))

        let next = CrashReporter(folder: folder)
        next.start(register: false)
        #expect(!FileManager.default.fileExists(atPath: raw.path), "the raw dump is gone once it is a report")
        let reports = folder.reports()
        #expect(reports.count == 1, "one report, not a session report too: \(reports.map(\.stem))")
        let report = try #require(reports.first)
        #expect(report.stem.hasPrefix("crash-native-") && report.files.count == 2)
        #expect(next.showsBanner && next.closedUnexpectedly)

        let j = try json(try #require(report.files.first { $0.pathExtension == "json" }))
        #expect(j["kind"] as? String == "native-signal" && j["appVersion"] as? String == "0.9 (12)")
        #expect((j["breadcrumbs"] as? [String])?.last == "t  edit: Add Fins", "the dump's own breadcrumbs, the latest")
        #expect(((j["settings"] as? [String: Any])?["options"] as? [String: Any]) != nil, "the settings JSON")
        let frames = try #require((j["signal"] as? [String: Any])?["frames"] as? [[String: Any]])
        #expect(frames.first?["symbol"] as? String == "abort" && frames.first?["symbolOffset"] as? Int == 8)
        let text = try String(contentsOf: try #require(report.files.first { $0.pathExtension == "txt" }), encoding: .utf8)
        #expect(text.contains("Support Fins crashed (SIGSEGV)") && text.contains("abort + 8") && text.contains("sf_test_rust_panicking_stand_in + 4"), "\(text)")
        #expect(text.contains("Last actions (oldest first):") && text.contains("edit: Add Fins"))
    }

    @Test func aDumpWithoutContextTakesTheSavedBreadcrumbsAndNeedsNoMarker() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        let r0 = CrashReporter(folder: folder)
        r0.start(register: false)
        r0.note("menu: Export Again")
        r0.stop()                                   // the marker went (the crash came after the normal quit began)
        _ = try plantDump(folder, try cannedDump(signal: SIGBUS, code: Int(BUS_ADRERR), frames: [try Self.address(of: "abort") + 8], time: Date(), context: nil))
        let r = CrashReporter(folder: folder)
        r.start(register: false)
        let report = try #require(folder.reports().first)
        #expect(folder.reports().count == 1 && report.stem.hasPrefix("crash-native-"))
        #expect(r.showsBanner && !r.closedUnexpectedly, "a native report raises the banner by itself")
        let j = try json(try #require(report.files.first { $0.pathExtension == "json" }))
        #expect((j["breadcrumbs"] as? [String])?.last?.hasSuffix("menu: Export Again") == true)
    }

    /// A panic that aborts: the panic hook writes `crash-rust-*`, then the abort's SIGABRT writes a dump whose stack runs
    /// through the panic machinery. One report: the Rust one, with the native stack joined.
    @Test func aRustPanicThatAbortedGivesOneReport() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        folder.writeMarker(now: Date().addingTimeInterval(-30))
        let rust = folder.url.appending(path: "crash-rust-2026-10-30T02-24-00-000-0")
        try Data(#"{"kind":"rust-panic","message":"probe"}"#.utf8).write(to: rust.appendingPathExtension("json"))
        try Data("Support Fins crash report (Rust panic)\n".utf8).write(to: rust.appendingPathExtension("txt"))
        let frames = [try Self.address(of: "abort") + 8, try Self.address(of: "sf_test_rust_panicking_stand_in") + 4]
        _ = try plantDump(folder, try cannedDump(signal: SIGABRT, code: 0, frames: frames, time: Date(), context: nil))

        let r = CrashReporter(folder: folder)
        r.start(register: false)
        let reports = folder.reports()
        #expect(reports.map(\.stem) == ["crash-rust-2026-10-30T02-24-00-000-0"], "no second report: \(reports.map(\.stem))")
        let j = try json(rust.appendingPathExtension("json"))
        #expect(j["message"] as? String == "probe" && j["abortedTheApp"] as? Bool == true)
        #expect(((j["native"] as? [String: Any])?["signal"] as? [String: Any])?["name"] as? String == "SIGABRT")
        let text = try String(contentsOf: rust.appendingPathExtension("txt"), encoding: .utf8)
        #expect(text.hasPrefix("Support Fins crash report (Rust panic)") && text.contains("The panic aborted the app") && text.contains("abort + 8"))
        #expect(r.showsBanner, "the run ended: the marker raises the banner")
    }

    @Test func metricKitsLaterCopySaysItWasCapturedNatively() throws {
        let folder = tempFolder()
        defer { try? FileManager.default.removeItem(at: folder.url) }
        let crashTime = Date().addingTimeInterval(-600)
        _ = try plantDump(folder, try cannedDump(signal: SIGSEGV, code: 1, frames: [try Self.address(of: "abort") + 8], time: crashTime, context: nil))
        let r = CrashReporter(folder: folder)
        r.start(register: false)
        let native = try #require(folder.reports().first { $0.stem.hasPrefix("crash-native-") })
        r.record(metrics: [
            .init(kind: "crash", json: Data("{}".utf8), begin: crashTime.addingTimeInterval(-3600), end: crashTime.addingTimeInterval(3600), signal: Int(SIGSEGV)),
            .init(kind: "crash", json: Data("{}".utf8), begin: crashTime.addingTimeInterval(-3600), end: crashTime.addingTimeInterval(3600), signal: Int(SIGABRT)),
        ])
        let mk = folder.reports().filter { $0.stem.hasPrefix("crash-metrickit-crash") }
        #expect(mk.count == 2)
        let marked = try mk.map { try json(try #require($0.files.first { $0.pathExtension == "json" }))["alsoCapturedNatively"] as? String }
        #expect(marked.compactMap { $0 } == [native.stem], "the SIGSEGV copy names the native report; the SIGABRT one is another crash: \(marked)")
    }

    @Test func testsWriteTheirDumpsIntoAPerRunFolderOfTheirOwn() {
        let none = UserDefaults(suiteName: "sf-none-\(UUID().uuidString)")!
        let dir = CrashReporter.nativeTestFolder(environment: ["XCTestConfigurationFilePath": "/x", "SF_CRASH_RUN": "gate-123"], defaults: none)
        #expect(dir?.standardizedFileURL.path(percentEncoded: false).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                == FileManager.default.temporaryDirectory.appending(path: "sf-native-crash/gate-123").standardizedFileURL.path(percentEncoded: false).trimmingCharacters(in: CharacterSet(charactersIn: "/")), "\(String(describing: dir))")
        #expect(CrashReporter.nativeTestFolder(environment: ["XCTestConfigurationFilePath": "/x", "SF_CRASH_RUN": "../../Logs"], defaults: none)?
            .standardizedFileURL.path.contains("/sf-native-crash/") == true, "a run name cannot climb out")
        #expect(CrashReporter.nativeTestFolder(environment: [:], defaults: none) == nil, "the app proper: the crash folder")
        // this very test host installed its handlers at launch, into its run's folder, never the user's
        let path = NativeCrashCapture.dumpPath
        #expect(path.contains("/sf-native-crash/") && !path.contains("Library/Logs"), "the test host's dump: \(path)")
        #expect(NativeCrashCapture.imageCount > 100, "the image table is filled")
        #expect(NativeCrashCapture.handlersInPlace == 6, "every fatal signal still reaches the handler in the test host (XCTest did not replace it)")
    }
}
