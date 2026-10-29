import Foundation
import SupportFinsCore
import Testing
import Viewer

@testable import Support_Fins

/// A closed window goes away (C-01, C-02 in docs/audit-2026-10/concurrency.md): its `ViewerModel`, the part, the
/// build model and the engine's `Builder` all deinitialise once the window drops its model, which is all SwiftUI
/// does when a window closes. The features hold their window weakly, so whatever was still running then (a build, a
/// parked model's rebuild, Draw's kit) finds no window and stops instead of reaching a freed one.
///
/// Only weak references are read after a window is dropped.
@MainActor
@Suite("Window lifetimes", .serialized)
struct WindowLifetimeTests {
    /// What one window held, weakly: its model, part, build and builder, and the shared models and features around them.
    private struct Held {
        private struct Ref { weak var object: AnyObject?; var name: String }
        private var refs: [Ref] = []
        var part: Part? { refs[1].object as? Part }
        var builder: Builder? { refs[3].object as? Builder }
        var board: PlateBoard? { refs[4].object as? PlateBoard }
        var draw: DrawModel? { refs[5].object as? DrawModel }

        @MainActor init(_ w: ViewerModel) {
            let named: [(String, AnyObject?)] = [
                ("window", w), ("part", w.session.parts.first?.part), ("build", w.wiring?.build), ("builder", w.wiring?.build?.builder),
                ("board", w.features[PlatesFeature.self]?.board), ("draw", w.features[DrawFeature.self]?.model),
                ("session", w.session), ("controller", w.controller), ("toolbar", w.toolbar), ("wiring", w.wiring),
            ]
            refs = named.map { Ref(object: $0.1, name: $0.0) } + w.features.instances.map { Ref(object: $0, name: "\(type(of: $0))") }
        }

        /// What is still alive, by name.
        var alive: [String] { refs.compactMap { $0.object == nil ? nil : $0.name } }
    }

    /// Waits (up to 5 s) for everything `held` to go: a Task still running when the window closed lets go of it
    /// once it next resumes.
    private func alive(_ held: [Held]) async -> [String] {
        for _ in 0..<500 {
            if held.allSatisfy(\.alive.isEmpty) { return [] }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return held.enumerated().flatMap { k, h in h.alive.map { "window \(k): \($0)" } }
    }

    private func tURL(_ name: String = "lifetime") throws -> URL {
        let tris = TestParts.box(-5, 5, -5, 5, 0, 20) + TestParts.box(-30, 30, -30, 30, 20, 24)
        return URL(fileURLWithPath: try TestParts.write(tris, name: "\(name)-\(UUID().uuidString).stl"))
    }

    /// A lumpy sphere of `n * n * 2` faces (overhangs under its equator).
    private func sphereURL(_ n: Int) throws -> URL {
        var tris: [[Float]] = []
        tris.reserveCapacity(n * n * 2)
        func at(_ i: Int, _ j: Int) -> [Float] {
            let th = Double(i) / Double(n) * .pi, ph = Double(j) / Double(n) * 2 * .pi
            let r = 20 + 2 * sin(5 * ph) * sin(3 * th)
            return [Float(r * sin(th) * cos(ph)), Float(r * sin(th) * sin(ph)), Float(30 + r * cos(th))]
        }
        for i in 0..<n { for j in 0..<n {
            let a = at(i, j), b = at(i + 1, j), c = at(i + 1, j + 1), d = at(i, j + 1)
            tris.append(a + b + c); tris.append(a + c + d)
        } }
        return URL(fileURLWithPath: try TestParts.write(tris, name: "lifetime-sphere-\(n).stl"))
    }

    /// A window showing `url` with its fins built.
    private func window(_ url: URL, waitForFins: Bool = true) async throws -> ViewerModel {
        let w = ViewerModel()
        try #require(w.controller != nil, "no Metal device")
        await w.load(url: url)
        try #require(w.hasPart, "\(w.status ?? "")")
        w.session.perform("Add Fins") { w.session.finsOn = true }
        if waitForFins {
            for _ in 0..<6000 where w.wiring?.build?.latest == nil { try await Task.sleep(for: .milliseconds(10)) }
            try #require(w.wiring?.build?.latest != nil, "the fins never landed")
        }
        return w
    }

    /// The process's thread count (each part's `Builder` and `Suggester` run a thread of their own).
    private func threads() -> Int {
        var list: thread_act_array_t?
        var count: mach_msg_type_number_t = 0
        guard task_threads(mach_task_self_, &list, &count) == KERN_SUCCESS, let list else { return -1 }
        for i in 0..<Int(count) { mach_port_deallocate(mach_task_self_, list[i]) }
        vm_deallocate(mach_task_self_, vm_address_t(UInt(bitPattern: list)), vm_size_t(Int(count) * MemoryLayout<thread_act_t>.stride))
        return Int(count)
    }

    private func residentMB() -> Double {
        var info = mach_task_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<natural_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count) }
        }
        return kr == KERN_SUCCESS ? Double(info.resident_size) / 1_048_576 : -1
    }

    @Test func anEmptyWindowIsFreed() async throws {
        var held: [Held] = []
        do { held.append(Held(ViewerModel())) }
        #expect(await alive(held) == [])
    }

    /// Ten windows, small parts and big ones, each with its fins: every window, part, build and builder goes.
    @Test func tenWindowsWithAPartAreFreed() async throws {
        let small = try tURL(), big = try sphereURL(250)
        let before = threads()
        var held: [Held] = []
        for k in 0..<10 {
            do {
                let w = try await window(k.isMultiple(of: 2) ? small : big)
                let h = Held(w)
                #expect(h.part != nil && h.builder != nil && h.board != nil && h.draw != nil, "the window holds a part, a builder, a board and Draw")
                held.append(h)
            }
        }
        let left = await alive(held)
        try await Task.sleep(for: .milliseconds(300))      // the engine's worker threads end once their handles go
        print("LIFETIME 10 windows: \(held.count - Set(left.map { $0.split(separator: ":")[0] }).count)/10 freed; alive \(left); threads before \(before) after \(threads())")
        #expect(left == [])
    }

    /// Five windows with a 320k-face part and its fins, opened and closed: the memory comes back.
    @Test func residentMemoryStaysFlatOverFiveWindows() async throws {
        let url = try sphereURL(400)
        var after: [Int] = []
        for _ in 0..<5 {
            var held: [Held] = []
            do { held.append(Held(try await window(url))) }
            _ = await alive(held)
            try await Task.sleep(for: .milliseconds(300))
            after.append(Int(residentMB()))
        }
        print("LIFETIME resident MB after each close \(after)")
        #expect(after.last! - after.first! <= 20, "resident MB after each close \(after)")
    }

    // MARK: closed while busy

    /// Closed while its fins build: the build's delivery finds no window.
    @Test func aWindowClosedMidBuildGoesQuietly() async throws {
        let url = try sphereURL(250)
        var held: [Held] = []
        do {
            let w = try await window(url, waitForFins: false)
            for _ in 0..<400 where w.wiring?.build?.building != true { try await Task.sleep(for: .milliseconds(5)) }
            #expect(w.wiring?.build?.building == true)
            held.append(Held(w))
        }
        #expect(await alive(held) == [])
        try await Task.sleep(for: .seconds(1))     // whatever was in flight lands now, and must not reach the window
    }

    /// Closed while a parked model's fins rebuild (the 150 ms timer pending, and the rebuild awaiting the engine).
    @Test func aWindowClosedMidParkGoesQuietly() async throws {
        var held: [Held] = []
        for wait in [0, 200] {
            do {
                let w = try await window(try tURL("park-a"))
                await w.load(url: try tURL("park-b"), adding: true)
                let board = try #require(w.features[PlatesFeature.self]?.board)
                #expect(board.plate.models.count == 2)
                var s = w.session.settings
                s.tineDensity = s.tineDensity == 70 ? 60 : 70
                w.session.setSettings(s)
                board.scheduleParked()              // the parked model's fins went stale (`PlatesFeature`'s onChange)
                if wait > 0 {
                    // close as soon as the rebuild is under way (awaiting the engine)
                    for _ in 0..<500 where board.parkedBuilding == nil { try await Task.sleep(for: .milliseconds(2)) }
                    #expect(board.parkedTask != nil && board.parkedBuilding != nil, "closed while a parked model rebuilds")
                } else {
                    #expect(board.parkedTimer != nil)
                }
                held.append(Held(w))
            }
        }
        #expect(await alive(held) == [])
        try await Task.sleep(for: .seconds(1))
    }

    /// Closed while Draw makes its kit.
    @Test func aWindowClosedMidDrawGoesQuietly() async throws {
        var held: [Held] = []
        do {
            let w = try await window(try sphereURL(150), waitForFins: false)
            var s = w.session.settings
            s.mode = .draw
            w.session.setSettings(s)
            let draw = try #require(w.features[DrawFeature.self]?.model)
            draw.sync()
            #expect(!draw.kitReady)
            held.append(Held(w))
        }
        #expect(await alive(held) == [])
        try await Task.sleep(for: .seconds(1))
    }
}
