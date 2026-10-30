import Darwin
import Foundation
import Testing

@testable import SupportFinsCore

/// Native crash capture (fix plan 1E). The handlers are proved in a child: `sf-crash-probe` (a command-line tool, so
/// macOS shows no crash dialog) installs them and crashes on purpose; the test checks it died of the right signal and
/// that its dump parses and names the function that crashed. Nothing here crashes the test process.
@Suite("Native crash capture")
struct NativeCrashTests {
    private final class Marker {}

    private static var probe: URL {
        Bundle(for: Marker.self).bundleURL.deletingLastPathComponent().appending(path: "sf-crash-probe")
    }

    private func tempDir() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appending(path: "sf-native-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// Run the probe in `mode`; its exit, its stdout (the dump's path) and the dump.
    private func crash(_ mode: String) throws -> (signal: Int32?, status: Int32, dump: NativeCrashDump?, path: String, dir: URL) {
        let dir = try tempDir()
        let p = Process()
        p.executableURL = Self.probe
        p.arguments = [dir.path, mode]
        let out = Pipe()
        p.standardOutput = out
        p.standardError = FileHandle.nullDevice
        try p.run()
        p.waitUntilExit()
        let path = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        let signal = p.terminationReason == .uncaughtSignal ? p.terminationStatus : nil
        let dump = (try? Data(contentsOf: URL(fileURLWithPath: path))).flatMap(NativeCrashDump.parse)
        return (signal, p.terminationStatus, dump, path, dir)
    }

    @Test(arguments: [
        ("segv", SIGSEGV, "probeCrashSegv"),
        ("bus", SIGBUS, "sfprobe_bus_error"),
        ("ill", SIGILL, "sfprobe_illegal_instruction"),
        ("fpe", SIGFPE, "probeCrashFpe"),
        ("abort", SIGABRT, "probeCrashAbort"),
        ("trap", SIGTRAP, "probeCrashSwiftTrap"),
    ])
    func aFatalSignalWritesADumpThatNamesTheCrashingFunction(mode: String, signal: Int32, function: String) throws {
        #expect(FileManager.default.isExecutableFile(atPath: Self.probe.path), "the probe is built next to the tests: \(Self.probe.path)")
        let r = try crash(mode)
        defer { try? FileManager.default.removeItem(at: r.dir) }
        #expect(r.signal == signal, "\(mode): the child died of \(NativeCrashReport.signalName(signal)) (default action after the dump), got status \(r.status)")
        let dump = try #require(r.dump, "\(mode): a dump at \(r.path)")
        #expect(dump.complete && dump.version == 1 && dump.appVersion == "probe 1.0 (1)")
        let section = try #require(dump.signal)
        #expect(section.signal == signal)
        #expect(section.registers["pc"] != nil && section.registers["fp"] != nil)
        #expect(dump.images.count > 10 && dump.images.contains { $0.name == "sf-crash-probe" }, "the image table: \(dump.images.count)")
        if mode == "segv" { #expect(section.address == 0x10 && [Int(SEGV_MAPERR), Int(SEGV_ACCERR)].contains(section.code ?? 0)) }
        if mode == "fpe" || mode == "abort" { #expect([0, Int(SI_USER)].contains(section.code ?? -1), "a sent signal (pthread_kill sends code 0)") }

        let symbolizer = NativeSymbolizer(images: dump.images)
        let frames = symbolizer.frames(of: section)
        let report = NativeCrashReport.make(dump, symbolizer: symbolizer)
        #expect(frames.contains { $0.symbol?.contains(function) == true }, "\(mode): \(function) is named:\n\(report.text)")
        #expect(frames.contains { $0.symbol == "main" } && frames.last?.symbol == "start" && frames.last?.image == "dyld",
                "the walk reaches the probe's entry and dyld's start:\n\(report.text)")
        #expect(Set(frames.map(\.address)).count == frames.count, "no frame twice (a stale lr is dropped):\n\(report.text)")
        #expect(report.text.contains(NativeCrashReport.signalName(signal)) && report.text.contains(function))

        // the context the probe set
        let ctx = try #require(dump.contextObject)
        #expect((ctx["breadcrumbs"] as? [String])?.last?.hasSuffix("edit: Add Fins") == true)
        #expect(((ctx["settings"] as? [String: Any])?["options"] as? [String: Any])?["nozzle"] as? Double == 0.4)
    }

    @Test func anUncaughtExceptionWritesItsNameReasonAndStackThenTheAbort() throws {
        let r = try crash("exception")
        defer { try? FileManager.default.removeItem(at: r.dir) }
        #expect(r.signal == SIGABRT, "the runtime aborts after the handler; got \(r.status)")
        let dump = try #require(r.dump, "a dump at \(r.path)")
        let e = try #require(dump.exception)
        #expect(e.exceptionName == "NSInvalidArgumentException" && e.exceptionReason == "probe: the model has no part 7")
        let symbolizer = NativeSymbolizer(images: dump.images)
        let frames = symbolizer.frames(of: e)
        #expect(frames.contains { $0.symbol?.contains("probeCrashException") == true }, "\(NativeCrashReport.make(dump, symbolizer: symbolizer).text)")
        #expect(dump.signal?.signal == SIGABRT, "the abort's section is in the same dump: one dump, not two")
        #expect(NativeCrashDump.parse(try Data(contentsOf: URL(fileURLWithPath: r.path))) != nil)
        #expect(NativeCrashCapture.dumps(in: r.dir).count == 1)
    }

    @Test func aCleanExitLeavesNoDump() throws {
        let r = try crash("none")
        defer { try? FileManager.default.removeItem(at: r.dir) }
        #expect(r.signal == nil && r.status == 0)
        #expect(NativeCrashCapture.dumps(in: r.dir).isEmpty, "the dump is opened only at a crash")
    }

    // MARK: the symbolizer on a canned dump

    /// A dump written by hand: one frame in an image this process has loaded (libsystem_c, found by its UUID),
    /// one in an image that is gone, one outside every image; a context; an exception with a newline in its reason.
    @Test func aCannedDumpParsesAndSymbolizes() throws {
        let abortAddr = UInt(bitPattern: dlsym(UnsafeMutableRawPointer(bitPattern: -2), "abort"))
        var info = Dl_info()
        #expect(dladdr(UnsafeRawPointer(bitPattern: abortAddr), &info) != 0)
        let header = UInt(bitPattern: info.dli_fbase)
        let loaded = NativeSymbolizer.loadedImages()
        let uuid = try #require(loaded.first { $0.value == header }?.key)
        // the crashed process had libsystem_c elsewhere (another slide): only the offset carries over
        let load: UInt64 = 0x1_8000_0000
        let pc = load + UInt64(abortAddr - header) + 8
        let gone = "00000000-1111-2222-3333-444444444444"
        let reason = "line one\nline two"
        let context = #"{"breadcrumbs":["a","b"],"settings":{"options":{"x":1}}}"#
        let text = """
        SFNATIVE 1
        pid 4242
        time 1793327040
        app 1.2 (34)
        section exception
        name 24
        NSRangeExceptionXXXXXXXX
        reason \(reason.utf8.count)
        \(reason)
        main-thread 1
        frame 0x\(String(pc + 4, radix: 16))
        section signal
        signal 6
        code 65537
        addr 0x0
        main-thread 0
        reg pc 0x\(String(pc, radix: 16))
        reg lr 0x20000100
        reg fp 0x16f000000
        reg sp 0x16effff00
        frame 0x\(String(pc, radix: 16))
        frame-lr 0x20000100
        frame 0x999
        image 0x\(String(load, radix: 16)) 0x1000000 \(uuid) /usr/lib/system/libsystem_c.dylib
        image 0x20000000 0x10000 \(gone) /nowhere/OldSupportFins
        context \(context.utf8.count)
        \(context)
        end

        """
        let dump = try #require(NativeCrashDump.parse(Data(text.utf8)))
        #expect(dump.pid == 4242 && dump.appVersion == "1.2 (34)" && dump.complete)
        #expect(dump.time == Date(timeIntervalSince1970: 1793327040))
        #expect(dump.sections.map(\.kind) == [.exception, .signal])
        #expect(dump.exception?.exceptionReason == reason && dump.exception?.exceptionName == "NSRangeExceptionXXXXXXXX")
        #expect(dump.signal?.signal == SIGABRT && dump.signal?.code == Int(SI_USER) && dump.signal?.mainThread == false)
        #expect(dump.signal?.registers["lr"] == 0x2000_0100)
        #expect(dump.images.count == 2 && dump.images[0].name == "libsystem_c.dylib")
        #expect((dump.contextObject?["breadcrumbs"] as? [String]) == ["a", "b"])

        let s = NativeSymbolizer(images: dump.images, loaded: loaded)
        let frames = s.frames(of: try #require(dump.signal))
        #expect(frames[0].symbol == "abort" && frames[0].symbolOffset == 8 && frames[0].image == "libsystem_c.dylib", "\(frames[0])")
        #expect(frames[1].symbol == nil && frames[1].image == "OldSupportFins" && frames[1].imageOffset == 0x100, "an image that is gone keeps image + offset")
        #expect(frames[2].image == nil && frames[2].symbol == nil, "outside every image")
        let ex = s.frames(of: try #require(dump.exception))
        #expect(ex[0].symbol == "abort" && ex[0].symbolOffset == 12, "a return address is looked up one byte back, and the offset is the real one")

        let report = NativeCrashReport.make(dump, symbolizer: s)
        #expect(report.text.contains("abort + 8") && report.text.contains("OldSupportFins + 0x100"))
        #expect(report.text.contains("SIGABRT (6)") && report.text.contains("SI_USER"))
        #expect(report.text.contains("Uncaught exception: NSRangeExceptionXXXXXXXX") && report.text.contains("line two"))
        #expect((report.json["images"] as? [[String: Any]])?.count == 2)
    }

    @Test func aTruncatedDumpKeepsWhatItHas() {
        let text = "SFNATIVE 1\npid 1\nsection signal\nsignal 11\nframe 0x10\ncontext 999\n{\"brea"
        let dump = NativeCrashDump.parse(Data(text.utf8))
        #expect(dump?.signal?.signal == SIGSEGV && dump?.signal?.frames == [0x10] && dump?.complete == false)
        #expect(NativeCrashDump.parse(Data("hello".utf8)) == nil, "not a dump")
        #expect(NativeCrashDump.parse(Data("SFNATIVE 1\npid 1\n".utf8)) == nil, "no section: nothing to report")
    }

    @Test func namesAreDemangled() {
        #expect(NativeSymbolizer.demangle("$s14sf_crash_probe14probeCrashSegvyyF") == "sf_crash_probe.probeCrashSegv() -> ()")
        #expect(NativeSymbolizer.demangle("_RNvCs1234_7sf_core5build") == "sf_core::build", "Rust v0 (the engine's release mangling)")
        #expect(NativeSymbolizer.demangle("_ZN7sf_core5build17h0123456789abcdefE") == "sf_core::build", "Rust legacy")
        #expect(NativeSymbolizer.demangle("_ZN8manifold4Impl5ApplyEv") == "manifold::Impl::Apply()", "C++")
        #expect(NativeSymbolizer.demangle("abort") == "abort")
    }

    @Test func theContextIsJSON() throws {
        let text = NativeCrashCapture.contextJSON(breadcrumbs: ["open \"a\"", "edit"], settingsJSON: #"{"options":{"nozzle":0.4},"tunables":null}"#)
        let o = try #require(try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        #expect(o["breadcrumbs"] as? [String] == ["open \"a\"", "edit"])
        #expect(NativeCrashCapture.contextJSON(breadcrumbs: [], settingsJSON: "") == #"{"breadcrumbs":[],"settings":null}"#)
    }
}
