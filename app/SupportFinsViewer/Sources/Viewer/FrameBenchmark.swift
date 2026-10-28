import Dispatch
import Foundation
import Metal
import QuartzCore
import simd

/// What moves during a benchmark run.
public enum BenchScenario: String, CaseIterable, Sendable {
    /// The camera turns round the part (azimuth and a little elevation).
    case orbit
    /// The camera pans and dollies back and forth.
    case panzoom
    /// A ring drag: the pose turns every frame and a fresh flags buffer (the engine's `face_flags` result)
    /// arrives every `flagsEvery` frames.
    case drag
    /// A new build arrives every `flagsEvery * 8` frames: the fins and pad are replaced (`setSupports`) by one of
    /// the layers passed in, alternating, and the flags buffer with it.
    case swap
}

/// Mean, 95th percentile and worst of a series of milliseconds.
public struct MillisecondStats: Sendable {
    public var mean = 0.0, p95 = 0.0, p99 = 0.0, max = 0.0
    init(_ seconds: [Double]) {
        guard !seconds.isEmpty else { return }
        let s = seconds.sorted()
        mean = seconds.reduce(0, +) / Double(seconds.count) * 1000
        p95 = s[min(s.count - 1, Int(Double(s.count) * 0.95))] * 1000
        p99 = s[min(s.count - 1, Int(Double(s.count) * 0.99))] * 1000
        max = s[s.count - 1] * 1000
    }
}

public struct FrameBenchmarkResult: Sendable {
    public var scenario: BenchScenario
    public var frames: Int
    public var width: Int, height: Int
    public var triangles: Int
    /// Main-thread time to update the scene and encode and commit one frame.
    public var cpu: MillisecondStats
    /// `gpuEndTime - gpuStartTime` of each command buffer.
    public var gpu: MillisecondStats
    /// Commit-to-commit interval (what a display link would see with the display not limiting it).
    public var interval: MillisecondStats
    public var fps: Double

    public var json: String {
        func s(_ m: MillisecondStats) -> String { String(format: "{\"mean\": %.3f, \"p95\": %.3f, \"p99\": %.3f, \"max\": %.3f}", m.mean, m.p95, m.p99, m.max) }
        return "{\"scenario\": \"\(scenario.rawValue)\", \"triangles\": \(triangles), \"size\": \"\(width)x\(height)\", \"frames\": \(frames), "
            + "\"cpu_ms\": \(s(cpu)), \"gpu_ms\": \(s(gpu)), \"interval_ms\": \(s(interval)), \"fps\": \(String(format: "%.1f", fps))}"
    }
}

extension ViewerRenderer {
    /// Render `frames` frames offscreen through the real colour pass (`encodeScene`), the camera and the pose
    /// moving by `scenario`, with up to three frames in flight as a `MTKView` allows, and report the CPU and GPU
    /// frame times. `warmup` frames are drawn first and not counted. Nothing waits for the display, so `fps` is
    /// what the machine could do, not what a display shows.
    public func benchmark(frames: Int, warmup: Int = 30, width: Int = 2560, height: Int = 1600,
                          scenario: BenchScenario = .orbit, flagsEvery: Int = 4,
                          layers: [SupportLayer] = []) -> FrameBenchmarkResult {
        let colorDesc = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: Self.colorFormat, width: width, height: height, mipmapped: false)
        colorDesc.usage = [.renderTarget]
        colorDesc.storageMode = .private
        let colors = (0..<3).map { _ in device.makeTexture(descriptor: colorDesc)! }
        let depth = makeDepth(width: width, height: height)
        let gate = DispatchSemaphore(value: 3)
        let gpuTimes = LockedList(), cpuTimes = LockedList(), commits = LockedList()
        let bg = Palette.background
        let baseCamera = camera, basePose = (rotation, offset)
        let flagsLength = max(totalFaces, 1)
        let spare = Spare(bytes: [UInt8](repeating: FaceFlags([.over, .kept]).rawValue, count: flagsLength))
        let continuousWas = continuous
        continuous = true
        defer { camera = baseCamera; setPose(rotation: basePose.0, offset: basePose.1); continuous = continuousWas }
        camera.aspect = Float(width) / Float(height)
        viewport = SIMD2(Float(width), Float(height))
        var done = DispatchGroup()
        for frame in -warmup..<frames {
            gate.wait()
            let t0 = CACurrentMediaTime()
            let t = Float(max(frame, 0)) / Float(max(frames, 1))
            switch scenario {
            case .orbit:
                camera.azimuth = baseCamera.azimuth + t * 2 * .pi
                camera.elevation = baseCamera.elevation + 0.2 * sin(t * 4 * .pi)
            case .panzoom:
                camera.target = baseCamera.target + SIMD3(30 * sin(t * 6 * .pi), 0, 15 * cos(t * 4 * .pi))
                camera.distance = baseCamera.distance * (1 + 0.4 * sin(t * 4 * .pi))
            case .swap:
                camera.azimuth = baseCamera.azimuth + t * 2 * .pi
                if frame >= 0, frame % (flagsEvery * 8) == 0, !layers.isEmpty {
                    setSupports(layers[(frame / (flagsEvery * 8)) % layers.count])
                    setFlags(device.makeBuffer(bytes: spare.bytes, length: flagsLength, options: .storageModeShared)!)
                }
            case .drag:
                let rot = rotationMatrix(degrees: SIMD3(0, 0, 360 * t * 2))
                setPose(rotation: rot, offset: basePose.1)
                if frame % flagsEvery == 0, !meshes.isEmpty {
                    // an analysis result: a new shared buffer of one byte a face, as the engine hands over
                    let b = device.makeBuffer(bytes: spare.bytes, length: flagsLength, options: .storageModeShared)!
                    setFlags(b)
                }
            }
            guard let cb = queue.makeCommandBuffer() else { gate.signal(); continue }
            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = colors[(frame + warmup) % 3]
            pass.colorAttachments[0].loadAction = .clear
            pass.colorAttachments[0].storeAction = .store
            pass.colorAttachments[0].clearColor = MTLClearColor(red: Double(bg.x), green: Double(bg.y), blue: Double(bg.z), alpha: 1)
            pass.depthAttachment.texture = depth
            pass.depthAttachment.loadAction = .clear
            pass.depthAttachment.storeAction = .dontCare
            pass.depthAttachment.clearDepth = 1
            let enc = cb.makeRenderCommandEncoder(descriptor: pass)!
            encodeScene(into: enc)
            enc.endEncoding()
            let counted = frame >= 0
            done.enter()
            let group = done
            cb.addCompletedHandler { cb in
                if counted { gpuTimes.add(cb.gpuEndTime - cb.gpuStartTime) }
                gate.signal()
                group.leave()
            }
            cb.commit()
            let t1 = CACurrentMediaTime()
            if counted { cpuTimes.add(t1 - t0); commits.add(t1) }
        }
        done.wait()
        done = DispatchGroup()
        let stamps = commits.values
        let intervals = zip(stamps.dropFirst(), stamps).map { $0 - $1 }
        let span = (stamps.last ?? 0) - (stamps.first ?? 0)
        return FrameBenchmarkResult(scenario: scenario, frames: frames, width: width, height: height, triangles: totalFaces,
                                    cpu: MillisecondStats(cpuTimes.values), gpu: MillisecondStats(gpuTimes.values),
                                    interval: MillisecondStats(intervals), fps: span > 0 ? Double(stamps.count - 1) / span : 0)
    }

    /// Latency of `count` face picks (an ID pass over the part and a one-pixel read-back), at the middle of the view.
    public func benchmarkPicks(count: Int, width: Int = 2560, height: Int = 1600) async -> MillisecondStats {
        var times: [Double] = []
        _ = await pickFace(x: width / 2, y: height / 2, width: width, height: height)
        for i in 0..<count {
            let t0 = CACurrentMediaTime()
            _ = await pickFace(x: width / 2 + i % 7, y: height / 2, width: width, height: height)
            times.append(CACurrentMediaTime() - t0)
        }
        return MillisecondStats(times)
    }
}

private final class LockedList: @unchecked Sendable {
    private let lock = NSLock()
    private var list: [Double] = []
    func add(_ v: Double) { lock.lock(); list.append(v); lock.unlock() }
    var values: [Double] { lock.lock(); defer { lock.unlock() }; return list }
}

private struct Spare: @unchecked Sendable { let bytes: [UInt8] }
