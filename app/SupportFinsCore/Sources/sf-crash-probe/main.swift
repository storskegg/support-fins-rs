// sf-crash-probe <dir> <mode>: installs the native crash handlers (SFCrashCapture) with its dump in <dir>, sets a
// context, prints the dump's path, then crashes as <mode> says: segv, bus, ill, fpe, abort, trap (a Swift runtime
// trap) or exception (an uncaught NSException). A command-line child: macOS shows no crash dialog for it.
// NativeCrashProbeTests (SupportFinsCoreTests) runs it for every mode and reads the dump back.
import Darwin
import Foundation
import SFCrashCapture
import SFCrashProbeC

@inline(never) func probeCrashSegv() {
    let p = UnsafeMutablePointer<Int>(bitPattern: 0x10)!
    p.pointee = 42
}

@inline(never) func probeCrashFpe() { raise(SIGFPE) }

@inline(never) func probeCrashAbort() { abort() }

@inline(never) func probeCrashSwiftTrap(_ n: Int) {
    let list = [1, 2, 3]
    print(list[n])   // index out of range: the Swift runtime traps
}

@inline(never) func probeCrashException() {
    NSException(name: .invalidArgumentException, reason: "probe: the model has no part 7", userInfo: nil).raise()
}

let args = CommandLine.arguments
guard args.count == 3 else {
    FileHandle.standardError.write(Data("usage: sf-crash-probe <dir> <segv|bus|ill|fpe|abort|trap|exception|none>\n".utf8))
    exit(2)
}
let dir = args[1], mode = args[2]
guard sfcc_install(dir, "probe 1.0 (1)") == 0 else { exit(3) }
let context = #"{"breadcrumbs":["2026-10-30T02:24:00.000Z  open file: probe.stl (STL), 1 objects, 12 triangles","2026-10-30T02:24:01.000Z  edit: Add Fins"],"settings":{"options":{"nozzle":0.4},"tunables":null}}"#
context.withCString { sfcc_set_context($0, strlen($0)) }
print(String(cString: sfcc_dump_path()))
fflush(stdout)

switch mode {
case "segv": probeCrashSegv()
case "bus": _ = sfprobe_bus_error(dir + "/bus-probe.bin")
case "ill": sfprobe_illegal_instruction()
case "fpe": probeCrashFpe()
case "abort": probeCrashAbort()
case "trap": probeCrashSwiftTrap(args.count + 4)
case "exception": probeCrashException()
case "none": exit(0)
default: exit(2)
}
// a crash mode that did not crash
exit(4)
