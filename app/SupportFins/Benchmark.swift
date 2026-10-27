import Foundation
import Viewer

/// The frame-time check of #89, driven by the environment so a release build can be measured without the
/// debug overlay: `SF_BENCH_SECONDS=<n>` turns the view round the part for `n` seconds after a short warm-up,
/// prints one JSON line to stdout (and to `SF_BENCH_OUT` when set), and quits.
@MainActor
enum Benchmark {
    static func runIfRequested(_ controller: ViewerController?) async {
        let env = ProcessInfo.processInfo.environment
        guard let controller, let seconds = env["SF_BENCH_SECONDS"].flatMap(Double.init) else { return }
        controller.setBenchmark(true)
        try? await Task.sleep(for: .seconds(4))          // warm-up: pipelines, first analysis, drawables
        controller.renderer.stats.reset()
        try? await Task.sleep(for: .seconds(seconds))
        let s = controller.renderer.stats.summary()
        controller.setBenchmark(false)
        let line = String(format: "{\"triangles\": %d, \"parts\": %d, \"frames\": %d, \"fps\": %.1f, \"gpu_mean_ms\": %.2f, \"gpu_p99_ms\": %.2f, \"frame_p99_ms\": %.2f, \"frame_worst_ms\": %.2f}\n",
                          controller.faceCount, controller.partCount, s.frames, s.fps, s.gpuMeanMs, s.gpuP99Ms, s.intervalP99Ms, s.worstIntervalMs)
        FileHandle.standardOutput.write(Data(line.utf8))
        if let out = env["SF_BENCH_OUT"] { try? line.write(toFile: out, atomically: true, encoding: .utf8) }
        exit(0)
    }
}
