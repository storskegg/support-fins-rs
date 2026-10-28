import Foundation
import CoreGraphics
import ImageIO
import Metal
import UniformTypeIdentifiers
import simd
import Viewer

// Headless frame-time benchmark: `swift run -c release viewer-bench [--tris N] [--fins N] [--tines N]
// [--frames N] [--size WxH] [--scenario orbit|panzoom|drag|swap|liveDrag|all] [--no-supports] [--json]`.
// Renders offscreen through the viewer's own colour pass; prints one JSON line a scenario.
@MainActor
func run() async {
    var tris = 1_500_000, fins = 3000, tines = 6000, frames = 600
    var width = 2560, height = 1600
    var scenarios: [BenchScenario] = BenchScenario.allCases
    var supports = true
    var distance: Float = 160
    var png: String?
    var args = CommandLine.arguments.dropFirst()
    while let a = args.popFirst() {
        switch a {
        case "--tris": tris = Int(args.popFirst() ?? "") ?? tris
        case "--fins": fins = Int(args.popFirst() ?? "") ?? fins
        case "--tines": tines = Int(args.popFirst() ?? "") ?? tines
        case "--frames": frames = Int(args.popFirst() ?? "") ?? frames
        case "--size":
            let p = (args.popFirst() ?? "").split(separator: "x").compactMap { Int($0) }
            if p.count == 2 { (width, height) = (p[0], p[1]) }
        case "--scenario": if let s = BenchScenario(rawValue: args.popFirst() ?? "") { scenarios = [s] }
        case "--distance": distance = Float(args.popFirst() ?? "") ?? distance
        case "--png": png = args.popFirst()
        case "--no-supports": supports = false
        default: FileHandle.standardError.write(Data("unknown argument \(a)\n".utf8)); exit(2)
        }
    }
    guard let r = ViewerRenderer() else { FileHandle.standardError.write(Data("no Metal device\n".utf8)); exit(1) }
    let t0 = Date()
    let mesh = StressMesh(triangles: tris, device: r.device)
    r.setMesh(mesh)
    r.setPose(rotation: matrix_identity_float3x3, offset: [0, 0, 51.75])
    r.setFlags(StressScene.flags(of: mesh, thresholdDegrees: 45, rotation: matrix_identity_float3x3, device: r.device))
    if supports { r.setSupports(StressScene.supports(fins: fins, tines: tines, device: r.device)) }
    r.camera = OrbitCamera(target: [0, 0, 40], distance: distance, azimuth: 0.6, elevation: 0.5, aspect: Float(width) / Float(height))
    FileHandle.standardError.write(Data(String(format: "scene: %d triangles, %d fins, %d tines, built in %.2f s\n", mesh.faceCount, fins, tines, Date().timeIntervalSince(t0)).utf8))
    if let png {
        let px = ViewerRenderer.pixels(of: r.renderOffscreen(width: width, height: height))
        let cs = CGColorSpace(name: CGColorSpace.sRGB)!
        let info = CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.noneSkipFirst.rawValue
        let img = px.withUnsafeBytes { raw -> CGImage? in
            let provider = CGDataProvider(data: Data(raw) as CFData)!
            return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4, space: cs,
                           bitmapInfo: CGBitmapInfo(rawValue: info), provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
        }
        let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: png) as CFURL, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(dest, img!, nil)
        CGImageDestinationFinalize(dest)
        return
    }
    let layers = supports ? (0..<2).map { StressScene.supports(fins: fins, tines: tines, device: r.device, seed: UInt64($0) + 11) } : []
    for s in scenarios {
        print(r.benchmark(frames: frames, width: width, height: height, scenario: s, layers: layers).json)
    }
    let p = await r.benchmarkPicks(count: 30, width: width, height: height)
    print(String(format: "{\"pick_ms\": {\"mean\": %.3f, \"p95\": %.3f, \"max\": %.3f}}", p.mean, p.p95, p.max))
}

await run()
