import Darwin
import Foundation
import MachO
import SFCrashCapture

/// Native crash capture (fix plan 1E, docs/FFI.md "Crash reports"): the C handlers in `SFCrashCapture` write a raw
/// dump at the moment of a fatal signal or an uncaught NSException; the next launch reads it (`NativeCrashDump`),
/// names its frames (`NativeSymbolizer`) and writes a report (`NativeCrashReport`).
public enum NativeCrashCapture {
    /// Raw dumps are `native-<unix time>-<pid>.sfdump`.
    public static let dumpPrefix = "native-"
    public static let dumpExtension = "sfdump"

    /// Install the signal and exception handlers (once; a later call moves the dump to `dir`, which is created).
    @discardableResult
    public static func install(dir: URL, appVersion: String) -> Bool {
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return sfcc_install(dir.path, appVersion) == 0
    }

    /// Where this process's dump would go ("" before `install`).
    public static var dumpPath: String { String(cString: sfcc_dump_path()) }

    /// How many of the six fatal signals go to the handler now (6 once installed, unless something replaced one).
    public static var handlersInPlace: Int { Int(sfcc_handlers_in_place()) }

    /// How many images the handler's table knows.
    public static var imageCount: Int { Int(sfcc_image_count()) }

    /// The breadcrumbs and settings a crash writes: `{"breadcrumbs":[...],"settings":<settingsJSON>}`.
    public static func contextJSON(breadcrumbs: [String], settingsJSON: String) -> String {
        let crumbs = (try? JSONSerialization.data(withJSONObject: breadcrumbs)).map { String(decoding: $0, as: UTF8.self) } ?? "[]"
        let settings = settingsJSON.isEmpty ? "null" : settingsJSON
        return "{\"breadcrumbs\":\(crumbs),\"settings\":\(settings)}"
    }

    /// Hand the handler the latest breadcrumbs and settings (preserialized: a crash only copies bytes).
    public static func setContext(breadcrumbs: [String], settingsJSON: String) {
        var text = contextJSON(breadcrumbs: breadcrumbs, settingsJSON: settingsJSON)
        text.withUTF8 { b in b.withMemoryRebound(to: CChar.self) { sfcc_set_context($0.baseAddress, $0.count) } }
    }

    /// The raw dumps in `dir`.
    public static func dumps(in dir: URL) -> [URL] {
        let all = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
        return all.filter { $0.lastPathComponent.hasPrefix(dumpPrefix) && $0.pathExtension == dumpExtension }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }
}

// MARK: - the raw dump

/// A raw dump (the format is described at the top of `SFCrashCapture/capture.c`).
public struct NativeCrashDump: Sendable, Equatable {
    public struct Image: Sendable, Equatable {
        public let load: UInt64
        public let size: UInt64
        public let uuid: String
        public let path: String
        public var name: String { (path as NSString).lastPathComponent }
    }

    public struct Section: Sendable, Equatable {
        public enum Kind: String, Sendable { case signal, exception }
        public let kind: Kind
        public var signal: Int32?
        public var code: Int?
        public var address: UInt64?
        public var mainThread: Bool?
        public var registers: [String: UInt64] = [:]
        public var exceptionName: String?
        public var exceptionReason: String?
        /// For a signal, the first is the pc, the rest return addresses; for an exception, all return addresses.
        public var frames: [UInt64] = []
        /// The index of the frame taken from lr (a signal's second frame, when the stack did not record it).
        public var lrFrame: Int?
    }

    public var version = 0
    public var pid: Int32 = 0
    public var time = Date(timeIntervalSince1970: 0)
    public var appVersion = ""
    public var sections: [Section] = []
    public var images: [Image] = []
    public var context: Data?
    /// The `end` line was written (the trailer made it out).
    public var complete = false

    /// The signal section, if any (an exception's dump has one too: the abort that followed).
    public var signal: Section? { sections.first { $0.kind == .signal } }
    public var exception: Section? { sections.first { $0.kind == .exception } }

    /// Nil when this is not a dump (no `SFNATIVE` line) or holds no section.
    public static func parse(_ data: Data) -> NativeCrashDump? {
        var r = Reader(bytes: [UInt8](data))
        guard let first = r.line(), first.hasPrefix("SFNATIVE ") else { return nil }
        var d = NativeCrashDump()
        d.version = Int(first.dropFirst(9)) ?? 0
        var current: Section?
        func close() { if let c = current { d.sections.append(c) }; current = nil }
        while let line = r.line() {
            let (key, value) = split(line)
            switch key {
            case "pid": d.pid = Int32(value) ?? 0
            case "time": d.time = Date(timeIntervalSince1970: TimeInterval(Int64(value) ?? 0))
            case "app": d.appVersion = String(value)
            case "section":
                close()
                if let kind = Section.Kind(rawValue: String(value)) { current = Section(kind: kind) }
            case "signal": current?.signal = Int32(value)
            case "code": current?.code = Int(value)
            case "addr": current?.address = hex(value)
            case "main-thread": current?.mainThread = value == "1"
            case "reg":
                let (name, v) = split(Substring(value))
                if let n = hex(v) { current?.registers[String(name)] = n }
            case "frame": if let a = hex(value) { current?.frames.append(a) }
            case "frame-lr":
                if let a = hex(value), var c = current {
                    c.lrFrame = c.frames.count
                    c.frames.append(a)
                    current = c
                }
            case "name", "reason", "context":
                guard let n = Int(value), let blob = r.blob(n) else { return d.sections.isEmpty && current == nil ? nil : finish() }
                if key == "context" { d.context = Data(blob) } else {
                    let s = String(decoding: blob, as: UTF8.self)
                    if key == "name" { current?.exceptionName = s } else { current?.exceptionReason = s }
                }
            case "image":
                let parts = value.split(separator: " ", maxSplits: 3, omittingEmptySubsequences: false)
                if parts.count == 4, let load = hex(parts[0]), let size = hex(parts[1]) {
                    d.images.append(Image(load: load, size: size, uuid: String(parts[2]), path: String(parts[3])))
                }
            case "end": d.complete = true
            default: break
            }
        }
        return finish()
        func finish() -> NativeCrashDump? {
            close()
            return d.sections.isEmpty ? nil : d
        }
    }

    /// The image an address falls in.
    public func image(containing address: UInt64) -> Image? {
        images.first { address >= $0.load && address < $0.load &+ max($0.size, 1) }
    }

    /// The context's breadcrumbs and settings (the JSON `NativeCrashCapture.setContext` wrote).
    public var contextObject: [String: Any]? {
        guard let context, !context.isEmpty else { return nil }
        return (try? JSONSerialization.jsonObject(with: context)) as? [String: Any]
    }

    private static func split(_ line: Substring) -> (Substring, Substring) {
        guard let sp = line.firstIndex(of: " ") else { return (line, "") }
        return (line[..<sp], line[line.index(after: sp)...])
    }

    private static func split(_ line: String) -> (String, Substring) {
        let (k, v) = split(Substring(line))
        return (String(k), v)
    }

    private static func hex(_ s: Substring) -> UInt64? {
        UInt64(s.hasPrefix("0x") ? s.dropFirst(2) : s, radix: 16)
    }

    private struct Reader {
        let bytes: [UInt8]
        var i = 0
        mutating func line() -> String? {
            guard i < bytes.count else { return nil }
            let start = i
            while i < bytes.count && bytes[i] != 0x0A { i += 1 }
            let s = String(decoding: bytes[start..<i], as: UTF8.self)
            if i < bytes.count { i += 1 }
            return s
        }
        mutating func blob(_ n: Int) -> ArraySlice<UInt8>? {
            guard n >= 0, i + n <= bytes.count else { return nil }
            let b = bytes[i..<(i + n)]
            i += n
            if i < bytes.count && bytes[i] == 0x0A { i += 1 }
            return b
        }
    }
}

// MARK: - names for frames

/// Names the frames of a dump. An image loaded in this process with the same UUID is asked with dladdr (the
/// system's libraries, and the app itself when it has not changed since the crash); otherwise the image's file on
/// disk is read, when it is there with the same UUID (its symbol table). A frame neither knows keeps image + offset.
/// Swift, C++ and Rust names are demangled (Rust: the engine's `crash_demangle`).
public final class NativeSymbolizer {
    public struct Frame: Sendable, Equatable {
        public let address: UInt64
        public let image: String?
        public let imageOffset: UInt64?
        public let symbol: String?
        public let symbolOffset: UInt64?
    }

    private let images: [NativeCrashDump.Image]
    private let loaded: [String: UInt]
    private var files: [String: MachOSymbols?] = [:]

    /// `loaded`: UUID -> header address of the images in this process (by default, every image dyld has loaded).
    public init(images: [NativeCrashDump.Image], loaded: [String: UInt] = NativeSymbolizer.loadedImages()) {
        self.images = images
        self.loaded = loaded
    }

    /// `isReturnAddress`: the address is just after a call; the name is looked up one byte back, inside the call.
    public func frame(_ address: UInt64, isReturnAddress: Bool) -> Frame {
        guard let image = images.first(where: { address >= $0.load && address < $0.load &+ max($0.size, 1) }) else {
            return Frame(address: address, image: nil, imageOffset: nil, symbol: nil, symbolOffset: nil)
        }
        let offset = address - image.load
        let probe = isReturnAddress && offset > 0 ? offset - 1 : offset
        var name: (String, UInt64)?
        if let header = loaded[image.uuid] { name = Self.dladdrName(header: header, offset: probe) }
        if name == nil, let symbols = symbols(for: image) { name = symbols.lookup(offset: probe) }
        let delta = name.map { $0.1 + (probe == offset ? 0 : 1) }
        return Frame(address: address, image: image.name, imageOffset: offset, symbol: name.map { Self.demangle($0.0) }, symbolOffset: delta)
    }

    /// The section's frames, named. The lr frame goes when it names the function the pc is in: a function that
    /// pushed its frame keeps its last call's return in lr, and its real caller is the next frame.
    public func frames(of section: NativeCrashDump.Section) -> [Frame] {
        var out = section.frames.enumerated().map { i, a in frame(a, isReturnAddress: !(section.kind == .signal && i == 0)) }
        if let i = section.lrFrame, i < out.count, i > 0 {
            let (pc, lr) = (out[0], out[i])
            if lr.address == pc.address || (lr.symbol != nil && lr.symbol == pc.symbol && lr.image == pc.image) { out.remove(at: i) }
        }
        return out
    }

    private func symbols(for image: NativeCrashDump.Image) -> MachOSymbols? {
        if let cached = files[image.path] { return cached }
        let s = MachOSymbols(path: image.path, uuid: image.uuid)
        files[image.path] = s
        return s
    }

    /// The images loaded in this process: UUID -> header address.
    public static func loadedImages() -> [String: UInt] {
        var out: [String: UInt] = [:]
        for i in 0..<_dyld_image_count() {
            guard let h = _dyld_get_image_header(i), let uuid = uuidOf(header: h) else { continue }
            out[uuid] = UInt(bitPattern: h)
        }
        return out
    }

    private static func uuidOf(header: UnsafePointer<mach_header>) -> String? {
        guard header.pointee.magic == MH_MAGIC_64 else { return nil }
        var p = UnsafeRawPointer(header).advanced(by: MemoryLayout<mach_header_64>.size)
        for _ in 0..<header.pointee.ncmds {
            let lc = p.load(as: load_command.self)
            if lc.cmd == UInt32(LC_UUID) {
                let u = p.load(as: uuid_command.self).uuid
                return UUID(uuid: u).uuidString
            }
            p = p.advanced(by: Int(lc.cmdsize))
        }
        return nil
    }

    private static func dladdrName(header: UInt, offset: UInt64) -> (String, UInt64)? {
        var info = Dl_info()
        guard let ptr = UnsafeRawPointer(bitPattern: header &+ UInt(offset)), dladdr(ptr, &info) != 0,
              UInt(bitPattern: info.dli_fbase) == header, let sname = info.dli_sname, let saddr = info.dli_saddr
        else { return nil }
        let name = String(cString: sname)
        if name == "<redacted>" { return nil }
        return (name, UInt64(UInt(bitPattern: ptr) - UInt(bitPattern: saddr)))
    }

    // MARK: demangling

    /// A readable name: Rust (legacy or v0), Swift or C++; anything else as it is.
    public static func demangle(_ symbol: String) -> String {
        if let rust = crashDemangle(symbol: symbol) { return rust }
        if symbol.hasPrefix("$s") || symbol.hasPrefix("$S") || symbol.hasPrefix("_T0") || symbol.hasPrefix("$e") {
            if let s = swiftDemangle(symbol) { return s }
        }
        if symbol.hasPrefix("_Z"), let s = cxxDemangle(symbol) { return s }
        return symbol
    }

    private typealias SwiftDemangleFn = @convention(c) (UnsafePointer<CChar>?, Int, UnsafeMutablePointer<CChar>?, UnsafeMutablePointer<Int>?, UInt32) -> UnsafeMutablePointer<CChar>?
    private typealias CxxDemangleFn = @convention(c) (UnsafePointer<CChar>?, UnsafeMutablePointer<CChar>?, UnsafeMutablePointer<Int>?, UnsafeMutablePointer<Int32>?) -> UnsafeMutablePointer<CChar>?

    private static func function<T>(_ name: String, as: T.Type) -> T? {
        guard let p = dlsym(UnsafeMutableRawPointer(bitPattern: -2), name) else { return nil }   // RTLD_DEFAULT
        return unsafeBitCast(p, to: T.self)
    }

    private static func swiftDemangle(_ s: String) -> String? {
        guard let f = function("swift_demangle", as: SwiftDemangleFn.self) else { return nil }
        return s.withCString { c in
            guard let out = f(c, strlen(c), nil, nil, 0) else { return nil }
            defer { free(out) }
            return String(cString: out)
        }
    }

    private static func cxxDemangle(_ s: String) -> String? {
        guard let f = function("__cxa_demangle", as: CxxDemangleFn.self) else { return nil }
        var status: Int32 = 0
        return s.withCString { c in
            guard let out = f(c, nil, nil, &status), status == 0 else { return nil }
            defer { free(out) }
            return String(cString: out)
        }
    }
}

/// An image file's symbol table (its defined symbols, by address), for frames of an image no longer loaded.
final class MachOSymbols {
    private let textVMAddr: UInt64
    private let symbols: [(UInt64, String)]

    /// Nil when the file is missing, not 64-bit Mach-O, or has no slice with this UUID.
    init?(path: String, uuid: String) {
        guard !path.isEmpty, let data = try? Data(contentsOf: URL(fileURLWithPath: path), options: .mappedIfSafe) else { return nil }
        let bytes = [UInt8](data)
        func u32(_ o: Int, big: Bool = false) -> UInt32? {
            guard o >= 0, o + 4 <= bytes.count else { return nil }
            let v = UInt32(bytes[o]) | UInt32(bytes[o + 1]) << 8 | UInt32(bytes[o + 2]) << 16 | UInt32(bytes[o + 3]) << 24
            return big ? v.byteSwapped : v
        }
        func u64(_ o: Int) -> UInt64? {
            guard let lo = u32(o), let hi = u32(o + 4) else { return nil }
            return UInt64(lo) | UInt64(hi) << 32
        }
        var slices: [Int] = []
        if u32(0) == 0xBEBAFECA, let n = u32(4, big: true) {          // FAT_MAGIC, big-endian
            for i in 0..<Int(min(n, 32)) { if let off = u32(8 + i * 20 + 8, big: true) { slices.append(Int(off)) } }
        } else if u32(0) == 0xBFBAFECA, let n = u32(4, big: true) {   // FAT_MAGIC_64
            for i in 0..<Int(min(n, 32)) {
                if let hi = u32(8 + i * 32 + 8, big: true), let lo = u32(8 + i * 32 + 12, big: true) { slices.append(Int(UInt64(hi) << 32 | UInt64(lo))) }
            }
        } else {
            slices = [0]
        }
        for base in slices {
            guard u32(base) == MH_MAGIC_64, let ncmds = u32(base + 16) else { continue }
            var p = base + 32
            var foundUUID: String?
            var text: UInt64?
            var symtab: (Int, Int, Int, Int)?
            for _ in 0..<ncmds {
                guard let cmd = u32(p), let size = u32(p + 4), size >= 8 else { break }
                if cmd == UInt32(LC_UUID), p + 24 <= bytes.count {
                    let b = Array(bytes[(p + 8)..<(p + 24)])
                    foundUUID = UUID(uuid: (b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15])).uuidString
                } else if cmd == UInt32(LC_SEGMENT_64), p + 32 <= bytes.count {
                    let name = String(decoding: bytes[(p + 8)..<(p + 24)].prefix { $0 != 0 }, as: UTF8.self)
                    if name == "__TEXT" { text = u64(p + 24) }
                } else if cmd == UInt32(LC_SYMTAB), let so = u32(p + 8), let ns = u32(p + 12), let st = u32(p + 16), let ss = u32(p + 20) {
                    symtab = (Int(so), Int(ns), Int(st), Int(ss))
                }
                p += Int(size)
            }
            guard foundUUID == uuid, let text, let (symoff, nsyms, stroff, strsize) = symtab else { continue }
            var list: [(UInt64, String)] = []
            for i in 0..<nsyms {
                let e = base + symoff + i * 16
                guard e + 16 <= bytes.count, let strx = u32(e), let value = u64(e + 8) else { break }
                let type = bytes[e + 4]
                guard type & 0xE0 == 0, type & 0x0E == 0x0E, value != 0, Int(strx) < strsize else { continue }   // not a stab; N_SECT
                let s = base + stroff + Int(strx)
                guard s < bytes.count else { continue }
                var end = s
                while end < bytes.count && bytes[end] != 0 { end += 1 }
                var name = String(decoding: bytes[s..<end], as: UTF8.self)
                if name.hasPrefix("_") { name.removeFirst() }   // the C-level underscore dladdr leaves out
                if !name.isEmpty { list.append((value, name)) }
            }
            list.sort { $0.0 < $1.0 }
            textVMAddr = text
            symbols = list
            return
        }
        return nil
    }

    /// The symbol at or before `offset` into the image, and how far past it.
    func lookup(offset: UInt64) -> (String, UInt64)? {
        let target = textVMAddr &+ offset
        var lo = 0, hi = symbols.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if symbols[mid].0 <= target { lo = mid + 1 } else { hi = mid }
        }
        guard lo > 0 else { return nil }
        let (addr, name) = symbols[lo - 1]
        return (name, target - addr)
    }
}

// MARK: - the readable report

/// A dump as a report: the JSON object and the text block the crash folder keeps (`crash-native-*.json`/`.txt`).
public enum NativeCrashReport {
    public static func signalName(_ s: Int32) -> String {
        switch s {
        case SIGSEGV: "SIGSEGV"
        case SIGBUS: "SIGBUS"
        case SIGILL: "SIGILL"
        case SIGFPE: "SIGFPE"
        case SIGABRT: "SIGABRT"
        case SIGTRAP: "SIGTRAP"
        default: "signal \(s)"
        }
    }

    public static func codeName(signal: Int32, code: Int) -> String? {
        switch (signal, Int32(code)) {
        case (SIGSEGV, SEGV_MAPERR): "SEGV_MAPERR (address not mapped)"
        case (SIGSEGV, SEGV_ACCERR): "SEGV_ACCERR (no permission)"
        case (SIGBUS, BUS_ADRALN): "BUS_ADRALN"
        case (SIGBUS, BUS_ADRERR): "BUS_ADRERR (no such address, e.g. past a mapped file's end)"
        case (SIGBUS, BUS_OBJERR): "BUS_OBJERR"
        case (SIGILL, ILL_ILLOPC): "ILL_ILLOPC (illegal opcode)"
        case (SIGILL, ILL_ILLTRP): "ILL_ILLTRP"
        case (SIGFPE, FPE_INTDIV): "FPE_INTDIV"
        case (SIGFPE, FPE_FLTDIV): "FPE_FLTDIV"
        case (SIGTRAP, TRAP_BRKPT), (SIGTRAP, 0): "a trap instruction (a failed Swift runtime check, or __builtin_trap)"
        case (_, SI_USER): "SI_USER (sent by kill)"
        case (_, 0): "sent by the process itself (abort, raise or pthread_kill)"
        default: nil
        }
    }

    /// One frame as a line: `  3  libsystem_c.dylib  0x0000000186960a00  abort + 124`.
    public static func line(_ i: Int, _ f: NativeSymbolizer.Frame) -> String {
        let index = String(i).padding(toLength: 3, withPad: " ", startingAt: 0)
        let image = (f.image ?? "???").padding(toLength: max(28, (f.image ?? "").count + 1), withPad: " ", startingAt: 0)
        let addr = String(format: "0x%016llx", f.address)
        let what: String
        if let s = f.symbol { what = "\(s) + \(f.symbolOffset ?? 0)" } else if let o = f.imageOffset { what = "\(f.image ?? "?") + 0x\(String(o, radix: 16))" } else { what = "?" }
        return "  \(index)\(image)\(addr)  \(what)"
    }

    static func frameJSON(_ f: NativeSymbolizer.Frame) -> [String: Any] {
        var o: [String: Any] = ["address": String(format: "0x%llx", f.address)]
        if let i = f.image { o["image"] = i }
        if let off = f.imageOffset { o["imageOffset"] = String(format: "0x%llx", off) }
        if let s = f.symbol { o["symbol"] = s; o["symbolOffset"] = f.symbolOffset ?? 0 }
        return o
    }

    /// The facts of the dump (signal, exception, frames, the images the frames are in) as JSON, and the stacks and
    /// signal as text. The caller adds the versions, the breadcrumbs and the settings.
    public static func make(_ dump: NativeCrashDump, symbolizer: NativeSymbolizer) -> (json: [String: Any], text: String, frames: [NativeSymbolizer.Frame]) {
        var json: [String: Any] = ["crashTime": ISO8601DateFormatter().string(from: dump.time), "pid": Int(dump.pid), "complete": dump.complete]
        var text = ""
        var used = Set<String>()
        var all: [NativeSymbolizer.Frame] = []
        if let e = dump.exception {
            let frames = symbolizer.frames(of: e)
            all += frames
            frames.compactMap(\.image).forEach { used.insert($0) }
            json["exception"] = ["name": e.exceptionName ?? "", "reason": e.exceptionReason ?? "", "frames": frames.map(frameJSON)]
            text += "Uncaught exception: \(e.exceptionName ?? "?")\nReason: \(e.exceptionReason ?? "")\n"
            text += "\nThe exception's stack (\(e.mainThread == true ? "main thread" : "a background thread")):\n"
            text += frames.enumerated().map { line($0, $1) + "\n" }.joined()
            text += "\n"
        }
        if let s = dump.signal {
            let sig = s.signal ?? 0
            let frames = symbolizer.frames(of: s)
            all += frames
            frames.compactMap(\.image).forEach { used.insert($0) }
            var o: [String: Any] = ["signal": Int(sig), "name": signalName(sig), "frames": frames.map(frameJSON)]
            if let c = s.code { o["code"] = c; if let n = codeName(signal: sig, code: c) { o["codeName"] = n } }
            if let a = s.address { o["faultAddress"] = String(format: "0x%llx", a) }
            if let m = s.mainThread { o["mainThread"] = m }
            o["registers"] = s.registers.mapValues { String(format: "0x%llx", $0) }
            json["signal"] = o
            text += "Signal: \(signalName(sig)) (\(sig))"
            if let c = s.code { text += ", code \(c)" + (codeName(signal: sig, code: c).map { " \($0)" } ?? "") }
            if let a = s.address { text += String(format: ", fault address 0x%llx", a) }
            text += "\nCrashed thread: \(s.mainThread == true ? "main" : "a background thread")\n"
            if !s.registers.isEmpty {
                text += "Registers: " + ["pc", "lr", "fp", "sp"].compactMap { k in s.registers[k].map { String(format: "\(k) 0x%llx", $0) } }.joined(separator: "  ") + "\n"
            }
            text += "\nThe crashed thread's stack:\n" + frames.enumerated().map { line($0, $1) + "\n" }.joined()
        }
        let images = dump.images.filter { used.contains($0.name) }
        json["images"] = images.map { ["name": $0.name, "path": $0.path, "uuid": $0.uuid, "load": String(format: "0x%llx", $0.load)] as [String: Any] }
        json["imageCount"] = dump.images.count
        if !images.isEmpty {
            text += "\nImages of these frames:\n" + images.map { String(format: "  0x%016llx  ", $0.load) + "\($0.uuid)  \($0.path)\n" }.joined()
        }
        if !dump.complete { text += "\n(The dump ends early: the crash handler did not finish.)\n" }
        return (json, text, all)
    }

    /// The stack shows a Rust panic that aborted (a panic while panicking, or in a function that cannot unwind):
    /// the engine's panic hook wrote its own report first, which this one joins.
    public static func isRustPanicAbort(signal: Int32?, frames: [NativeSymbolizer.Frame]) -> Bool {
        guard signal == SIGABRT else { return false }
        let marks = ["rust_panic", "panic_abort", "panicking", "abort_internal", "panic_nounwind", "rust_begin_unwind"]
        return frames.contains { f in f.symbol.map { s in marks.contains { s.contains($0) } } ?? false }
    }
}
